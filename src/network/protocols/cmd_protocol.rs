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
pub struct CmdProtocol();

#[derive(Clone)]
pub struct CmdCodec();

#[derive(Debug, Clone)]
pub struct CmdRequest(pub command_proto::Command);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdResponse(Vec<u8>);

impl ProtocolName for CmdProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/cmd".as_bytes()
    }
}

#[async_trait]
impl RequestResponseCodec for CmdCodec {
    type Protocol = CmdProtocol;
    type Request = CmdRequest;
    type Response = CmdResponse;

    async fn read_request<T>(&mut self, _: &CmdProtocol, io: &mut T)
                             -> io::Result<Self::Request>
        where
            T: AsyncRead + Unpin + Send
    {
        read_one(io, MAX_SIZE)
            .map(|res| match res {
                Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                Ok(vec) if vec.is_empty() => {
                    debug!("receved cmd request,but len is zero");
                    Err(io::ErrorKind::InvalidData.into())
                }
                Ok(vec) => {
                    debug!("receved cmd request,len is good");
                    let command = command_proto::Command::decode(vec.as_slice()).unwrap();
                    Ok(CmdRequest(command))
                }
            })
            .await
    }

    async fn read_response<T>(&mut self, _: &CmdProtocol, io: &mut T)
                              -> io::Result<Self::Response>
        where
            T: AsyncRead + Unpin + Send
    {
        read_one(io, MAX_SIZE)
            .map(|res| match res {
                Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                Ok(vec) if vec.is_empty() => {
                    debug!("receved cmd response,but len is zero");
                    let vec: Vec<u8> = Vec::new();
                    Ok(CmdResponse(vec))
                }
                Ok(vec) => {
                    debug!("receved cmd response,len is good");
                    Ok(CmdResponse(vec))
                }
            })
            .await
    }

    async fn write_request<T>(&mut self, _: &CmdProtocol, io: &mut T, CmdRequest(request): CmdRequest)
                              -> io::Result<()>
        where
            T: AsyncWrite + Unpin + Send
    {
        debug!("send cmd request,request id:{}", request.command_id);
        let mut buf_final = Vec::new();
        request.encode(&mut buf_final);
        write_with_len_prefix(io, buf_final).await
    }

    async fn write_response<T>(&mut self, _: &CmdProtocol, io: &mut T, CmdResponse(data): CmdResponse)
                               -> io::Result<()>
        where
            T: AsyncWrite + Unpin + Send
    {
        write_with_len_prefix(io, data).await
    }
}