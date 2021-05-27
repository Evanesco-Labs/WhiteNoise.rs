use libp2p::PeerId;
use smallvec::SmallVec;
use tokio::sync::oneshot;
use crate::network::protocols::proxy_protocol::ProxyRequest;
use crate::network::protocols::ack_protocol::AckRequest;
use crate::network::protocols::cmd_protocol::CmdRequest;
use libp2p::core::Multiaddr;
use crate::request_proto;

pub struct NodeAckRequest{
    pub remote_peer_id: PeerId,
    pub ack_request: std::option::Option<AckRequest>
}

pub struct NodeNewStream{
    pub peer_id: PeerId,
}

pub enum PeerOperation{
    Disconnect
}

pub struct NodeProxyRequest{
    pub remote_peer_id: PeerId,
    pub proxy_request: std::option::Option<ProxyRequest>,
    pub peer_operation: std::option::Option<PeerOperation>
}

pub struct NodeCmdRequest{
    pub remote_peer_id: PeerId,
    pub cmd_request: std::option::Option<CmdRequest>,
}

pub struct AddPeerAddresses{
    pub remote_peer_id: PeerId,
    pub remote_addr: SmallVec<[Multiaddr; 6]>
}

pub struct GetMainNets{
    pub command_id: String,
    pub remote_peer_id: PeerId,
    pub num: i32,
    pub sender: oneshot::Sender<Vec<request_proto::NodeInfo>>
}
pub struct PublishDataRequest{
    pub data: Vec<u8>,
    pub sender: oneshot::Sender<bool>
}
pub enum NodeRequest {
    ProxyRequest(NodeProxyRequest),
    CmdRequest(NodeCmdRequest),
    AckRequest(NodeAckRequest),
    NewStreamRequest(NodeNewStream),
    AddPeerAddressesRequest(AddPeerAddresses),
    GetMainNetsRequest(GetMainNets),
    PublishData(PublishDataRequest)
}
