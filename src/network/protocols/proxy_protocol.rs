use async_trait::async_trait;
use libp2p::{
    request_response::{RequestResponseCodec, ProtocolName},
    core::upgrade::{read_one, write_with_len_prefix},
};

use crate::{request_proto};
use futures::{prelude::*};

use tokio::io::{self};
use prost::Message;

use log::{debug};

#[derive(Debug, Clone)]
pub struct ProxyProtocol();

#[derive(Clone)]
pub struct ProxyCodec();

#[derive(Debug, Clone)]
pub struct ProxyRequest(pub request_proto::Request);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyResponse(Vec<u8>);

impl ProtocolName for ProxyProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/proxy".as_bytes()
    }
}

#[async_trait]
impl RequestResponseCodec for ProxyCodec {
    type Protocol = ProxyProtocol;
    type Request = ProxyRequest;
    type Response = ProxyResponse;

    async fn read_request<T>(&mut self, _: &ProxyProtocol, io: &mut T)
                             -> io::Result<Self::Request>
        where
            T: AsyncRead + Unpin + Send
    {
        read_one(io, 8 * 1024 * 1024)
            .map(|res| match res {
                Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                Ok(vec) if vec.is_empty() => {
                    debug!("receved proxy request,but len is zero");
                    Err(io::ErrorKind::InvalidData.into())
                }
                Ok(vec) => {
                    debug!("receved proxy request,len is good");
                    let zz = vec.as_slice();
                    let request = request_proto::Request::decode(zz).unwrap();
                    Ok(ProxyRequest(request))
                }
            })
            .await
    }

    async fn read_response<T>(&mut self, _: &ProxyProtocol, io: &mut T)
                              -> io::Result<Self::Response>
        where
            T: AsyncRead + Unpin + Send
    {
        read_one(io, 8 * 1024 * 1024)
            .map(|res| match res {
                Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                Ok(vec) if vec.is_empty() => {
                    debug!("receved proxy response,but len is zero");
                    let vec: Vec<u8> = Vec::new();
                    Ok(ProxyResponse(vec))
                }
                Ok(vec) => {
                    debug!("receved proxy response,len is good");
                    Ok(ProxyResponse(vec))
                }
            })
            .await
    }

    async fn write_request<T>(&mut self, _: &ProxyProtocol, io: &mut T, ProxyRequest(request): ProxyRequest)
                              -> io::Result<()>
        where
            T: AsyncWrite + Unpin + Send
    {
        debug!("send proxy request,request id:{}", request.req_id);
        let mut buf_final = Vec::new();
        request.encode(&mut buf_final)?;
        write_with_len_prefix(io, buf_final).await
    }

    async fn write_response<T>(&mut self, _: &ProxyProtocol, io: &mut T, ProxyResponse(data): ProxyResponse)
                               -> io::Result<()>
        where
            T: AsyncWrite + Unpin + Send
    {
        write_with_len_prefix(io, data).await
    }
}