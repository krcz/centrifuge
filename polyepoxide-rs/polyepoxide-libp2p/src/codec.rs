//! CBOR codec for libp2p request_response.

use std::io;

use async_trait::async_trait;
use futures::prelude::*;
use libp2p::request_response;
use libp2p::StreamProtocol;

use crate::protocol::{Request, Response, PROTOCOL_NAME};

/// Maximum message size (16 MB).
const MAX_MESSAGE_SIZE: u64 = 16 * 1024 * 1024;

/// CBOR codec for Polyepoxide protocol.
#[derive(Debug, Clone, Default)]
pub struct PolyepoxideCodec;

#[async_trait]
impl request_response::Codec for PolyepoxideCodec {
    type Protocol = StreamProtocol;
    type Request = Request;
    type Response = Response;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_cbor_message(io).await
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_cbor_message(io).await
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_cbor_message(io, &req).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_cbor_message(io, &res).await
    }
}

/// Read a length-prefixed CBOR message.
async fn read_cbor_message<T, M>(io: &mut T) -> io::Result<M>
where
    T: AsyncRead + Unpin + Send,
    M: serde::de::DeserializeOwned,
{
    // Read 4-byte length prefix (big-endian)
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as u64;

    if len > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} bytes", len),
        ));
    }

    // Read message body
    let mut buf = vec![0u8; len as usize];
    io.read_exact(&mut buf).await?;

    // Deserialize
    ciborium::from_reader(&buf[..]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write a length-prefixed CBOR message.
async fn write_cbor_message<T, M>(io: &mut T, msg: &M) -> io::Result<()>
where
    T: AsyncWrite + Unpin + Send,
    M: serde::Serialize,
{
    // Serialize to buffer
    let mut buf = Vec::new();
    ciborium::into_writer(msg, &mut buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if buf.len() as u64 > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} bytes", buf.len()),
        ));
    }

    // Write length prefix (4 bytes, big-endian)
    let len_buf = (buf.len() as u32).to_be_bytes();
    io.write_all(&len_buf).await?;

    // Write message body
    io.write_all(&buf).await?;

    Ok(())
}

/// Returns the protocol identifier.
pub fn protocol() -> StreamProtocol {
    StreamProtocol::new(PROTOCOL_NAME)
}
