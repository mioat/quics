use quics_protocol::Provider;

pub struct Stream<T> {
    inner: T,
}

pub struct Builder<T> {
    connection: T,
}

mod s2n_quic {
    use s2n_quic::stream::BidirectionalStream;
    use s2n_quic::Connection as NoiseConnection;
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::Receiver;

    use crate::{debug, error};

    use super::{Builder, Provider, Stream};

    impl<T> Builder<T>
    where
        T: Provider<NoiseConnection> + 'static,
    {
        pub fn new(connection: T) -> Self {
            Self { connection }
        }

        pub fn build(mut self) -> impl Provider<BidirectionalStream> {
            let (stream_sender, stream_receiver) = mpsc::channel(1usize);

            tokio::spawn(async move {
                'connection: while let Some(mut connection) = self.connection.fetch().await {
                    'stream: loop {
                        let stream = match connection.open_bidirectional_stream().await {
                            Ok(stream) => stream,
                            Err(_error) => {
                                error!(
                                    "connection {} failed to open bidirectional stream. {}",
                                    connection.id(),
                                    _error
                                );
                                break 'stream;
                            }
                        };

                        debug!(
                            "{:?} connection {} open bidirectional stream {}",
                            connection.local_addr(),
                            connection.id(),
                            stream.id()
                        );

                        if let Err(_error) = stream_sender.send(stream).await {
                            break 'connection;
                        }
                    }
                }
            });

            Stream {
                inner: stream_receiver,
            }
        }
    }

    impl Provider<BidirectionalStream> for Stream<Receiver<BidirectionalStream>> {
        async fn fetch(&mut self) -> Option<BidirectionalStream> {
            self.inner.recv().await
        }
    }
}
