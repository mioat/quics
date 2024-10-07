use std::{error::Error, future::Future, net::SocketAddr};

use quics_protocol::{
    request::{Address, Request},
    Provider,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, Result},
    net::TcpStream,
    sync::mpsc::{self, Receiver},
};

use crate::{error, info};

pub struct SocksServer {
    inner: Receiver<(TcpStream, Request)>,
}

impl SocksServer {
    pub async fn with(address: String) -> std::result::Result<Self, Box<dyn Error>> {
        use tokio::net::TcpListener;

        let address: SocketAddr = address.parse()?;
        let listener = TcpListener::bind(address).await?;

        let (sender, receiver) = mpsc::channel(1);

        tokio::spawn(async move {
            loop {
                let sender = sender.clone();
                match listener.accept().await {
                    Ok((stream, _address)) => {
                        tokio::spawn(async move {
                            let request = match Self::handle(stream).await {
                                Ok(value) => value,
                                Err(_error) => {
                                    error!("{}", _error);
                                    return;
                                }
                            };

                            if let Some(value) = request {
                                if sender.send(value).await.is_err() {
                                    return;
                                }
                            }
                        });
                    }

                    Err(_error) => {
                        error!("failed to accept: {:?}", _error);
                    }
                };
            }
        });

        Ok(Self { inner: receiver })
    }

    async fn handle(mut stream: TcpStream) -> Result<Option<(TcpStream, Request)>> {
        use socks::socks5::{
            Address as SocksAddress, Method as Socks5Method, Request as Socks5Request,
            Response as Socks5Response,
        };

        use socks::Streamable;

        let methods = <Vec<Socks5Method> as Streamable>::read(&mut stream).await?;

        // Authentication
        let method = <Self as Authentication>::select(methods).await?;
        <Socks5Method as Streamable>::write(&method, &mut stream).await?;

        // Process Authentication
        if !matches!(method, Socks5Method::NoAuthentication) {
            <Self as Authentication>::process(&mut stream).await?;
        }

        // Read Request
        let request = <Socks5Request as Streamable>::read(&mut stream).await?;

        info!("SOCKS5 {:?}", request);

        match request {
            Socks5Request::Connect(address) => {
                Socks5Response::unspecified_success()
                    .write(&mut stream)
                    .await?;

                let address = match address {
                    SocksAddress::IPv4(value) => Address::IPv4(value),
                    SocksAddress::IPv6(value) => Address::IPv6(value),
                    SocksAddress::Domain(domain, port) => Address::Domain(domain, port),
                };

                return Ok(Some((stream, Request::TCPConnect(address))));
            }

            Socks5Request::Associate(address) => {
                use tokio::net::UdpSocket;

                // IPV4 & IPV6
                let socket_addr = match socks5_udp::to_socket_address(address).await? {
                    SocketAddr::V4(_) => "0.0.0.0:0",
                    SocketAddr::V6(_) => "[::]:0",
                };

                let inbound = UdpSocket::bind(socket_addr).await?;
                let outbound = UdpSocket::bind(socket_addr).await?;

                let address = SocksAddress::from_socket_address(inbound.local_addr()?);
                Socks5Response::Success(address).write(&mut stream).await?;

                tokio::select! {
                    // TCP Stream closed
                    _ = stream.read_u8() => {}

                    // UDP Transfer
                    _ = socks5_udp::relay(inbound, outbound) => {}
                }

                return Ok(None);
            }
            _ => {
                Socks5Response::CommandNotSupported
                    .write(&mut stream)
                    .await?;

                return Err(std::io::Error::other("unsupported socks command"));
            }
        };
    }
}

impl Authentication for SocksServer {
    async fn select(_methods: Vec<socks::socks5::Method>) -> Result<socks::socks5::Method> {
        Ok(socks::socks5::Method::NoAuthentication)
    }

    async fn process<T>(_stream: &mut T) -> Result<()>
    where
        T: AsyncReadExt + AsyncWriteExt + Unpin + Send,
    {
        Ok(())
    }
}

pub trait Authentication {
    fn select(
        methods: Vec<socks::socks5::Method>,
    ) -> impl Future<Output = Result<socks::socks5::Method>> + Send;
    fn process<S>(stream: &mut S) -> impl Future<Output = Result<()>> + Send
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin + Send;
}

impl Provider<(TcpStream, Request)> for SocksServer {
    async fn fetch(&mut self) -> Option<(TcpStream, Request)> {
        self.inner.recv().await
    }
}

mod socks5_udp {
    use socks::socks5::{Address, UdpPacket};
    use socks::{Streamable, ToBytes};
    use tokio::net::UdpSocket;

    use super::*;

    pub async fn to_socket_address(address: Address) -> Result<SocketAddr> {
        use std::io::Error;

        match address {
            Address::Domain(domain, port) => {
                use tokio::net::lookup_host;
                let response = lookup_host((domain.as_str(), port)).await?;
                let address = response.into_iter().next().ok_or_else(|| {
                    Error::other(format!("could not resolve domain '{}'", domain))
                })?;

                Ok(address)
            }
            Address::IPv4(addr) => Ok(addr.into()),
            Address::IPv6(addr) => Ok(addr.into()),
        }
    }

    async fn handle_udp_response(inbound: &UdpSocket, outbound: &UdpSocket) -> Result<()> {
        let mut buffer = vec![0u8; 8192];

        loop {
            let (size, remote_addr) = outbound.recv_from(&mut buffer).await?;

            let data = (&buffer[..size]).into();
            let address = Address::from_socket_address(remote_addr);
            let packet = UdpPacket::un_frag(address, data);

            inbound.send(&packet.to_bytes()).await?;
        }
    }

    async fn handle_udp_request(inbound: &UdpSocket, outbound: &UdpSocket) -> Result<()> {
        let mut buffer = vec![0u8; 8192];

        loop {
            let (size, client_addr) = inbound.recv_from(&mut buffer).await?;

            inbound.connect(client_addr).await?;

            let packet = UdpPacket::read(&mut &buffer[..size]).await?;
            let address = to_socket_address(packet.address).await?;

            outbound.send_to(&packet.data, address).await?;
        }
    }

    pub async fn relay(inbound: UdpSocket, outbound: UdpSocket) -> Result<()> {
        use tokio::try_join;

        match try_join!(
            handle_udp_request(&inbound, &outbound),
            handle_udp_response(&inbound, &outbound)
        ) {
            Ok(_) => {}
            Err(error) => return Err(error),
        }

        Ok(())
    }
}
