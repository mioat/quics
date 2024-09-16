use std::{
    error::Error,
    net::{SocketAddr, SocketAddrV4, SocketAddrV6},
};

use quics_protocol::{
    request::{Request, SocketAddress},
    Provider,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, Result},
    net::TcpStream,
    sync::mpsc::{self, Receiver},
};

use crate::{error, info};

pub struct Socks5 {
    inner: Receiver<(TcpStream, Request)>,
}

impl Socks5 {
    pub async fn with(address: String) -> std::result::Result<Self, Box<dyn Error>> {
        use tokio::net::TcpListener;

        let address: SocketAddr = address.parse()?;
        let listener = TcpListener::bind(address).await?;

        let (sender, receiver) = mpsc::channel(1);

        tokio::spawn(async move {
            loop {
                let sender = sender.clone();
                match listener.accept().await {
                    Ok((mut stream, _address)) => {
                        tokio::spawn(async move {
                            let request = match Self::handle(&mut stream).await {
                                Ok(value) => value,
                                Err(_error) => {
                                    error!("{}", _error);
                                    return;
                                }
                            };

                            if let Err(_error) = sender.send((stream, request)).await {
                                return;
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

    async fn handle(stream: &mut TcpStream) -> std::io::Result<Request> {
        read_authentication(stream).await?;
        stream.write_all(&[0x05, 0x00]).await?;

        let request = read_request(stream).await?;

        match request.inner() {
            Requests::TCPConnect(address) => {
                info!("tcp connect {:?}", address);
                let reply = [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
                stream.write_all(&reply).await?;
                return Ok(Request::TCPConnect(address.into()));
            }

            _ => {
                let reply = [0x05, 0x00, 0x00, 0x07, 0, 0, 0, 0, 0, 0];
                stream.write_all(&reply).await?;
                return Err(std::io::Error::other("unsupported socks command"));
            }
        };
    }
}

impl Provider<(TcpStream, Request)> for Socks5 {
    async fn fetch(&mut self) -> Option<(TcpStream, Request)> {
        self.inner.recv().await
    }
}

/// # Authentication
///
/// ## Stream
/// ```text
///          +----+----------+----------+
///          |VER | NMETHODS | METHODS  |
///          +----+----------+----------+
///          | 1  |    1     | 1 to 255 |
///          +----+----------+----------+
/// ```
pub async fn read_authentication<S>(stream: &mut S) -> Result<SocksPact<Vec<AuthenticationMethod>>>
where
    S: AsyncReadExt + Unpin + Send,
{
    let mut buffer = [0u8; 2];
    stream.read_exact(&mut buffer).await?;

    let method_num = buffer[1] as usize;
    if method_num == 1 {
        stream.read_exact(&mut [0u8; 1]).await?;
        return Ok(SocksPact::new(
            buffer[1],
            vec![AuthenticationMethod::NoAuth],
        ));
    }

    let mut methods = vec![0u8; method_num];
    stream.read_exact(&mut methods).await?;

    let list = methods
        .into_iter()
        .map(|e| AuthenticationMethod::from_u8(e))
        .collect();

    Ok(SocksPact::new(buffer[0], list))
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthenticationMethod {
    NoAuth,              // X'00'
    GSSAPI,              // X'01'
    UsernamePassword,    // X'02'
    IanaAssigned(u8),    // X'03'~X'7F'
    ReservedPrivate(u8), // X'80'~X'FE'
    NoAcceptableMethod,  // X'FF'
}

impl AuthenticationMethod {
    #[rustfmt::skip]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::NoAuth                     => 0x00,
            Self::GSSAPI                     => 0x01,
            Self::UsernamePassword           => 0x03,
            Self::IanaAssigned(value)    => value,
            Self::ReservedPrivate(value) => value,
            Self::NoAcceptableMethod         => 0xFF,
        }
    }

    #[rustfmt::skip]
    pub fn from_u8(value: u8) -> Self {
        match value {
            0x00        => Self::NoAuth,
            0x01        => Self::GSSAPI,
            0x02        => Self::UsernamePassword,
            0x03..=0x7F => Self::IanaAssigned(value),
            0x80..=0xFE => Self::ReservedPrivate(value),
            0xFF        => Self::NoAcceptableMethod,
        }
    }
}

#[derive(Debug)]
pub struct SocksPact<T> {
    version: u8,
    inner: T,
}

impl<T> SocksPact<T> {
    pub fn new(version: u8, inner: T) -> Self {
        Self { version, inner }
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn inner(self) -> T {
        self.inner
    }
}

/// # Request
/// Reads a SOCKS request from the provided stream.
///
/// ## Stream
/// ```text
///          +----+-----+-------+------+----------+----------+
///          |VER | CMD |  RSV  | ATYP | DST.ADDR | DST.PORT |
///          +----+-----+-------+------+----------+----------+
///          | 1  |  1  |   1   |  1   | Variable |    2     |
///          +----+-----+-------+------+----------+----------+
/// ```
///
pub async fn read_request<S>(stream: &mut S) -> Result<SocksPact<Requests>>
where
    S: AsyncReadExt + Unpin + Send,
{
    let mut buffer = [0u8; 3];
    stream.read_exact(&mut buffer).await?;

    let address = {
        use std::io::{Error, ErrorKind};
        use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

        let mut buffer = [0u8; 1];
        stream.read_exact(&mut buffer).await?;

        let address_type = buffer[0];
        let request = match address_type {
            consts::SOCKS5_ADDRESS_TYPE_IPV4 => {
                let mut buffer = [0u8; consts::IPV4_ADDRESS_LENGTH + consts::PORT_LENGTH];
                stream.read_exact(&mut buffer).await?;

                let ip = Ipv4Addr::new(buffer[0], buffer[1], buffer[2], buffer[3]);
                let port = ((buffer[4] as u16) << 8) | (buffer[5] as u16);

                RequestAddress::IPv4(SocketAddrV4::new(ip, port))
            }

            consts::SOCKS5_ADDRESS_TYPE_IPV6 => {
                let mut buffer = [0u8; consts::IPV6_ADDRESS_LENGTH + consts::PORT_LENGTH];
                stream.read_exact(&mut buffer).await?;

                let ip = Ipv6Addr::new(
                    (buffer[0] as u16) << 8 | buffer[1] as u16,
                    (buffer[2] as u16) << 8 | buffer[3] as u16,
                    (buffer[4] as u16) << 8 | buffer[5] as u16,
                    (buffer[6] as u16) << 8 | buffer[7] as u16,
                    (buffer[8] as u16) << 8 | buffer[9] as u16,
                    (buffer[10] as u16) << 8 | buffer[11] as u16,
                    (buffer[12] as u16) << 8 | buffer[13] as u16,
                    (buffer[14] as u16) << 8 | buffer[15] as u16,
                );
                let port = ((buffer[16] as u16) << 8) | (buffer[17] as u16);

                RequestAddress::IPv6(SocketAddrV6::new(ip, port, 0, 0))
            }

            consts::SOCKS5_ADDRESS_TYPE_DOMAIN_NAME => {
                let mut buffer = [0u8; 1];
                stream.read_exact(&mut buffer).await?;
                let domain_len = buffer[0] as usize;

                let mut buffer = vec![0u8; domain_len + consts::PORT_LENGTH];
                stream.read_exact(&mut buffer).await?;

                let domain = std::str::from_utf8(&buffer[0..domain_len])
                    .map_err(|_| Error::new(ErrorKind::InvalidData, "invalid domain name"))?;

                let port = ((buffer[domain_len] as u16) << 8) | (buffer[domain_len + 1] as u16);

                RequestAddress::Domain(domain.to_string(), port)
            }

            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("unsupported socks request address type {}", address_type),
                ))
            }
        };

        request
    };

    let result = match buffer[1] {
        consts::SOCKS5_CMD_TCP_BIND => Requests::TCPBind(address),
        consts::SOCKS5_CMD_TCP_CONNECT => Requests::TCPConnect(address),
        consts::SOCKS5_CMD_UDP_ASSOCIATE => Requests::UDPAssociate(address),
        _ => Requests::NotSupported(address),
    };

    Ok(SocksPact::new(buffer[0], result))
}

#[rustfmt::skip]
mod consts {
    pub const SOCKS5_CMD_TCP_CONNECT:           u8 = 0x01;
    pub const SOCKS5_CMD_TCP_BIND:              u8 = 0x02;
    pub const SOCKS5_CMD_UDP_ASSOCIATE:         u8 = 0x03;

    pub const SOCKS5_ADDRESS_TYPE_IPV4:         u8 = 0x01;
    pub const SOCKS5_ADDRESS_TYPE_DOMAIN_NAME:  u8 = 0x03;
    pub const SOCKS5_ADDRESS_TYPE_IPV6:         u8 = 0x04;

    pub const PORT_LENGTH:                      usize = 2;
    pub const IPV4_ADDRESS_LENGTH:              usize = 4;
    pub const IPV6_ADDRESS_LENGTH:              usize = 16;
}

/// # SOCKS5 Request
#[derive(Debug, Clone, PartialEq)]
pub enum Requests {
    TCPBind(RequestAddress),
    TCPConnect(RequestAddress),
    UDPAssociate(RequestAddress),
    NotSupported(RequestAddress),
}

/// # SOCKS5 Request Address
#[derive(Debug, Clone, PartialEq)]
pub enum RequestAddress {
    IPv4(SocketAddrV4),
    IPv6(SocketAddrV6),
    Domain(String, u16),
}

impl Into<SocketAddress> for RequestAddress {
    fn into(self) -> SocketAddress {
        match self {
            Self::Domain(domain, port) => SocketAddress::Domain(domain, port),
            Self::IPv4(address) => SocketAddress::IPv4(address),
            Self::IPv6(address) => SocketAddress::IPv6(address),
        }
    }
}
