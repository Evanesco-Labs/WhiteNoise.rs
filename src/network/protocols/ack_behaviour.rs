use libp2p::request_response::{RequestResponse, ProtocolSupport, RequestResponseConfig};
use crate::network::protocols::ack_protocol::{AckCodec, AckProtocol};
use std::iter;

pub type AckBehaviour = RequestResponse<AckCodec>;

pub fn new() -> AckBehaviour {
    let ack_protocols = iter::once((AckProtocol(), ProtocolSupport::Full));
    let ack_cfg = RequestResponseConfig::default();
    RequestResponse::new(AckCodec(), ack_protocols, ack_cfg)
}
