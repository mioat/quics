use std::marker::PhantomData;

use tokio::io::{AsyncReadExt, AsyncWriteExt, Result};

use crate::{Provider, ToBytes};

pub struct Server<R, RS>
where
    R: Provider<RS>,
    RS: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static,
{
    accept: R,
    _accept_stream: PhantomData<RS>,
}

impl<R, RS> Server<R, RS>
where
    R: Provider<RS>,
    RS: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static,
{
    pub fn with(accept: R) -> Self {
        Self {
            accept,
            _accept_stream: PhantomData,
        }
    }

    pub async fn start(&mut self) {
        while let Some(stream) = self.accept.fetch().await {
            tokio::spawn(async move { Self::handle_bidirectional(stream).await });
        }
    }

    async fn handle_bidirectional(mut stream: RS) -> Result<()> {
        use crate::request::Request;
        use crate::response::Response;
        use crate::Streamable;

        let request = Request::read_from(&mut stream).await?;

        match request {
            Request::TCPConnect(address) => {
                use tokio::io::copy_bidirectional;
                use tokio::net::TcpStream;

                let address = address.to_socket_address().await?;
                let mut connect = TcpStream::connect(address).await?;

                stream.write_all(&Response::Succeed.to_bytes()).await?;

                copy_bidirectional(&mut stream, &mut connect).await?
            }
        };

        Ok(())
    }
}
