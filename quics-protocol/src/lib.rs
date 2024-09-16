use std::future::Future;
use std::io::Result;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub mod client;
pub mod request;
pub mod response;
pub mod server;

pub trait Streamable: Sized {
    fn write_to<T>(self, stream: &mut T) -> impl Future<Output = Result<()>> + Send
    where
        T: AsyncWriteExt + Unpin + Send;

    fn read_from<T>(stream: &mut T) -> impl Future<Output = Result<Self>> + Send
    where
        T: AsyncReadExt + Unpin + Send;
}

pub trait ToBytes {
    fn to_bytes(self) -> Bytes;
}

pub trait Provider<T>: Send {
    fn fetch(&mut self) -> impl Future<Output = Option<T>> + Send;
}
