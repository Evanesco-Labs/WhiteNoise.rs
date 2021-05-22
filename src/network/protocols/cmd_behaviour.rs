use libp2p::request_response::{RequestResponse, ProtocolSupport, RequestResponseConfig};
use std::iter;
use crate::network::protocols::cmd_protocol::{CmdCodec, CmdProtocol};

pub type CmdBehaviour = RequestResponse<CmdCodec>;

pub fn new() -> CmdBehaviour {
    let cmd_protocols = iter::once((CmdProtocol(), ProtocolSupport::Full));
    let cmd_cfg = RequestResponseConfig::default();
    RequestResponse::new(CmdCodec(), cmd_protocols, cmd_cfg)
}