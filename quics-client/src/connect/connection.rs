use std::net::SocketAddr;

use quics_protocol::Provider;

pub struct Connection<T> {
    inner: T,
}

pub struct Builder<T> {
    client: T,
    server_name: String,
    server_addr: SocketAddr,
}

mod s2n_quic {
    use s2n_quic::{
        client::{Client as NoiseClient, Connect},
        Connection as NoiseConnection,
    };
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::Receiver;

    use crate::{debug, error};

    use super::{Builder, Connection, Provider, SocketAddr};

    impl Builder<NoiseClient> {
        pub fn new<T>(client: NoiseClient, server_name: String, server_addr: T) -> Self
        where
            T: Into<SocketAddr>,
        {
            Self {
                client,
                server_name,
                server_addr: server_addr.into(),
            }
        }

        pub fn build(self) -> impl Provider<NoiseConnection> {
            let (connection_sender, connection_receiver) = mpsc::channel(1usize);

            tokio::spawn(async move {
                'connection: loop {
                    let connect =
                        Connect::new(self.server_addr).with_server_name(self.server_name.as_str());

                    let mut connection = match self.client.connect(connect).await {
                        Ok(value) => value,
                        Err(error) => {
                            error!(
                                "{:?} failed to establish connection with {}. {}",
                                self.client.local_addr(),
                                self.server_name,
                                error
                            );

                            continue;
                        }
                    };

                    if let Err(_error) = connection.keep_alive(true) {
                        error!("failed to keep alive the connection. {}", _error);
                        continue;
                    };

                    debug!(
                        "{:?} establish connection {} with {:?}",
                        connection.local_addr(),
                        connection.id(),
                        connection.remote_addr()
                    );

                    if let Err(_error) = connection_sender.send(connection).await {
                        break 'connection;
                    }
                }
            });

            Connection {
                inner: connection_receiver,
            }
        }
    }

    impl Provider<NoiseConnection> for Connection<Receiver<NoiseConnection>> {
        async fn fetch(&mut self) -> Option<NoiseConnection> {
            self.inner.recv().await
        }
    }
}
