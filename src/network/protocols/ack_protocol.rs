use async_trait::async_trait;

use libp2p::{
    request_response::{RequestResponseCodec,ProtocolName},
    core::upgrade::{read_one,write_with_len_prefix},
};

use crate::{command_proto};
use futures::{prelude::*};

use tokio::io::{self};
use prost::Message;

use log::{debug};

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