use std::marker::PhantomData;

use tokio::io::{AsyncReadExt, AsyncWriteExt, Result};

use crate::{Provider, Resolver, ToBytes};

pub struct Server<R, RE, RS>
where
    R: Provider<RS>,
    RE: Resolver + Clone + Send + 'static,
    RS: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static,
{
    accept: R,
    resolver: RE,
    _accept_stream: PhantomData<RS>,
}

impl<R, RS, RE> Server<R, RE, RS>
where
    R: Provider<RS>,
    RE: Resolver + Clone + Send + Sync + 'static,
    RS: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static,
{
    pub fn with(accept: R, resolver: RE) -> Self {
        Self {
            accept,
            resolver,
            _accept_stream: PhantomData,
        }
    }

    pub async fn start(&mut self) {
        while let Some(stream) = self.accept.fetch().await {
            let resolver = self.resolver.clone();
            tokio::spawn(async move { Self::handle_bidirectional(stream, resolver).await });
        }
    }

    async fn handle_bidirectional(mut stream: RS, resolver: RE) -> Result<()> {
        use crate::request::Request;
        use crate::response::Response;
        use crate::Streamable;

        let request = Request::read_from(&mut stream).await?;

        match request {
            Request::TCPConnect(address) => {
                use tokio::io::copy_bidirectional;
                use tokio::net::TcpStream;

                let address = address.to_socket_address(&resolver).await?;
                let mut connect = TcpStream::connect(address).await?;

                stream.write_all(&Response::Succeed.to_bytes()).await?;

                copy_bidirectional(&mut stream, &mut connect).await?
            }
        };

        Ok(())
    }
}
