use std::io::Result;

use bytes::{BufMut, Bytes, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{Streamable, ToBytes};

#[rustfmt::skip]
mod consts {
    pub const SUCCEED: u8 = 0x01;
    pub const NO_ACCEPTABLE_REQUEST: u8 = 0xFF;
}

pub enum Response {
    Succeed,
    NoAcceptableMethod,
}

impl ToBytes for Response {
    fn to_bytes(self) -> Bytes {
        let mut bytes = BytesMut::new();

        match self {
            Self::Succeed => {
                bytes.put_u8(consts::SUCCEED);
            }
            Self::NoAcceptableMethod => bytes.put_u8(consts::NO_ACCEPTABLE_REQUEST),
        };

        bytes.freeze()
    }
}

impl Streamable for Response {
    async fn read_from<T>(stream: &mut T) -> Result<Self>
    where
        T: AsyncReadExt + Unpin + Send,
    {
        let mut buffer = [0u8; 1];
        stream.read_exact(&mut buffer).await?;

        let response = match buffer[0] {
            consts::SUCCEED => Self::Succeed,
            _ => Self::NoAcceptableMethod,
        };

        Ok(response)
    }

    async fn write_to<T>(self, stream: &mut T) -> Result<()>
    where
        T: AsyncWriteExt + Unpin + Send,
    {
        stream.write_all(&self.to_bytes()).await
    }
}
