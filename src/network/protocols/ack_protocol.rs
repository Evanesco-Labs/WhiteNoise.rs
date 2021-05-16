use async_trait::async_trait;

use libp2p::{
    request_response::{RequestResponseCodec, ProtocolName},
    core::upgrade::{read_one, write_with_len_prefix},
};

use crate::{command_proto};
use futures::{prelude::*};

use tokio::io;
use prost::Message;

use log::{debug};

const MAX_SIZE: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct AckProtocol();

#[derive(Clone)]
pub struct AckCodec();

#[derive(Debug, Clone, PartialEq)]
pub struct AckRequest(pub command_proto::Ack);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckResponse(Vec<u8>);

impl ProtocolName for AckProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/ack".as_bytes()
    }
}

#[async_trait]
impl RequestResponseCodec for AckCodec {
    type Protocol = AckProtocol;
    type Request = AckRequest;
    type Response = AckResponse;

    async fn read_request<T>(&mut self, _: &AckProtocol, io: &mut T)
                             -> io::Result<Self::Request>
        where
            T: AsyncRead + Unpin + Send
    {
        debug!("received ack request");
        read_one(io, MAX_SIZE)
            .map(|res| match res {
                Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                Ok(vec) if vec.is_empty() => {
                    debug!("received ack request,but len is zero");
                    Err(io::ErrorKind::InvalidData.into())
                }
                Ok(vec) => {
                    debug!("received ack request,len is good");
                    let zz = command_proto::Ack::decode(vec.as_slice()).unwrap();
                    Ok(AckRequest(zz))
                }
            })
            .await
    }

    async fn read_response<T>(&mut self, _: &AckProtocol, io: &mut T)
                              -> io::Result<Self::Response>
        where
            T: AsyncRead + Unpin + Send
    {
        read_one(io, MAX_SIZE)
            .map(|res| match res {
                Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                Ok(vec) if vec.is_empty() => {
                    debug!("read ack response,but len is zero");
                    let vec: Vec<u8> = Vec::new();
                    Ok(AckResponse(vec))
                }
                Ok(vec) => {
                    debug!("read ack response,len is good");

                    Ok(AckResponse(vec))
                }
            })
            .await
    }

    async fn write_request<T>(&mut self, _: &AckProtocol, io: &mut T, AckRequest(data): AckRequest)
                              -> io::Result<()>
        where
            T: AsyncWrite + Unpin + Send
    {
        let mut buf = Vec::new();
        data.encode(&mut buf);
        write_with_len_prefix(io, buf).await
    }

    async fn write_response<T>(&mut self, _: &AckProtocol, io: &mut T, AckResponse(data): AckResponse)
                               -> io::Result<()>
        where
            T: AsyncWrite + Unpin + Send
    {
        write_with_len_prefix(io, data).await
    }
}