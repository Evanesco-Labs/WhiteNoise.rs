use libp2p::request_response::{RequestResponse, ProtocolSupport, RequestResponseConfig};
use crate::network::protocols::proxy_protocol::{ProxyCodec, ProxyProtocol};

type ProxyBehaviour = RequestResponse<ProxyCodec>;

pub fn new() -> ProxyBehaviour {
    let proxy_protocols = std::iter::once((ProxyProtocol(), ProtocolSupport::Full));
    let proxy_config = RequestResponseConfig::default();
    RequestResponse::new(ProxyCodec(), proxy_protocols, proxy_config)
}