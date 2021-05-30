use libp2p::PeerId;
use smallvec::SmallVec;
use tokio::sync::{oneshot, mpsc};
use crate::network::protocols::proxy_protocol::{ProxyRequest, ProxyCodec, ProxyResponse};
use crate::network::protocols::ack_protocol::{AckRequest, AckCodec, AckResponse};
use crate::network::protocols::cmd_protocol::{CmdRequest, CmdCodec, CmdResponse};
use libp2p::core::Multiaddr;
use crate::request_proto;
use libp2p::request_response::{RequestResponse, RequestResponseEvent, RequestResponseMessage};
use crate::network::protocols::relay_behaviour::{Relay, RelayEvent, WrappedStream};
use std::collections::VecDeque;
use libp2p::swarm::{NetworkBehaviourEventProcess, NetworkBehaviour};
use libp2p::NetworkBehaviour;
use log::{info, debug, warn};

pub struct NodeAckRequest {
    pub remote_peer_id: PeerId,
    pub ack_request: std::option::Option<AckRequest>,
}

pub struct NodeNewStream {
    pub peer_id: PeerId,
}

pub enum PeerOperation {
    Disconnect
}

pub struct NodeProxyRequest {
    pub remote_peer_id: PeerId,
    pub proxy_request: std::option::Option<ProxyRequest>,
    pub peer_operation: std::option::Option<PeerOperation>,
}

pub struct NodeCmdRequest {
    pub remote_peer_id: PeerId,
    pub cmd_request: std::option::Option<CmdRequest>,
}

pub struct AddPeerAddresses {
    pub remote_peer_id: PeerId,
    pub remote_addr: SmallVec<[Multiaddr; 6]>,
}

pub struct GetMainNets {
    pub command_id: String,
    pub remote_peer_id: PeerId,
    pub num: i32,
    pub sender: oneshot::Sender<Vec<request_proto::NodeInfo>>,
}

pub struct PublishDataRequest {
    pub data: Vec<u8>,
    pub sender: oneshot::Sender<bool>,
}

pub enum NodeRequest {
    ProxyRequest(NodeProxyRequest),
    CmdRequest(NodeCmdRequest),
    AckRequest(NodeAckRequest),
    NewStreamRequest(NodeNewStream),
    AddPeerAddressesRequest(AddPeerAddresses),
    GetMainNetsRequest(GetMainNets),
    PublishData(PublishDataRequest),
}


#[derive(NetworkBehaviour)]
pub struct WhitenoiseBehaviour {
    pub proxy_behaviour: RequestResponse<ProxyCodec>,
    pub cmd_behaviour: RequestResponse<CmdCodec>,
    pub ack_behaviour: RequestResponse<AckCodec>,
    pub relay_behaviour: Relay,

    #[behaviour(ignore)]
    pub event_bus: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, oneshot::Sender<AckRequest>>>>,
    #[behaviour(ignore)]
    pub relay_out_streams: std::sync::Arc<std::sync::RwLock<VecDeque<WrappedStream>>>,
    #[behaviour(ignore)]
    pub relay_in_streams: std::sync::Arc<std::sync::RwLock<VecDeque<WrappedStream>>>,
    #[behaviour(ignore)]
    pub proxy_request_channel: mpsc::UnboundedSender<NodeProxyRequest>,
    #[behaviour(ignore)]
    pub cmd_request_channel: mpsc::UnboundedSender<NodeCmdRequest>,
}


impl NetworkBehaviourEventProcess<RelayEvent> for WhitenoiseBehaviour {
    fn inject_event(&mut self, message: RelayEvent) {
        match message {
            RelayEvent::RelayInbound(x) => {
                self.relay_in_streams.write().unwrap().push_back(x);
            }
            RelayEvent::RelayOutbound(x) => {
                self.relay_out_streams.write().unwrap().push_back(x);
            }
            RelayEvent::Disconnect(peer_id) => {
                debug!("whitenoise behaviour connection disconnect for peer_id:{}", peer_id);
                let node_proxy_request = NodeProxyRequest { remote_peer_id: peer_id, proxy_request: None, peer_operation: Some(PeerOperation::Disconnect) };
                self.proxy_request_channel.send(node_proxy_request);
            }
            _ => {
                warn!("unknown relay event poll");
            }
        }
    }
}


impl NetworkBehaviourEventProcess<RequestResponseEvent<ProxyRequest, ProxyResponse>> for WhitenoiseBehaviour {
    fn inject_event(&mut self, message: RequestResponseEvent<ProxyRequest, ProxyResponse>) {
        match message {
            RequestResponseEvent::InboundFailure { peer, request_id, error } => {
                debug!("proxy inbound failure:{:?}", error);
            }
            RequestResponseEvent::OutboundFailure { peer, request_id: req_id, error } => {
                debug!("proxy outbound failure:{:?}", error);
            }
            RequestResponseEvent::Message { peer, message } => {
                debug!("proxy received mssage:{:?}", message);
                match message {
                    RequestResponseMessage::Request { request_id, request, .. } => {
                        let node_proxy_request = NodeProxyRequest {
                            remote_peer_id: peer,
                            proxy_request: Some(request),
                            peer_operation: None,
                        };
                        self.proxy_request_channel.send(node_proxy_request);
                    }
                    _ => {}
                }
            }
            RequestResponseEvent::ResponseSent { peer, request_id } => {
                debug!("proxy send response:{:?}", request_id);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<RequestResponseEvent<CmdRequest, CmdResponse>> for WhitenoiseBehaviour {
    fn inject_event(&mut self, message: RequestResponseEvent<CmdRequest, CmdResponse>) {
        match message {
            RequestResponseEvent::InboundFailure { peer, request_id, error } => {
                debug!("cmd inbound failure:{:?}", error);
            }
            RequestResponseEvent::OutboundFailure { peer, request_id: req_id, error } => {
                debug!("cmd outbound failure:{:?}", error);
            }
            RequestResponseEvent::Message { peer, message } => {
                debug!("cmd received mssage:{:?}", message);
                match message {
                    RequestResponseMessage::Request { request_id, request, .. } => {
                        let node_proxy_request = NodeCmdRequest {
                            remote_peer_id: peer,
                            cmd_request: Some(request),
                        };
                        self.cmd_request_channel.send(node_proxy_request);
                    }
                    _ => {}
                }
            }
            RequestResponseEvent::ResponseSent { peer, request_id } => {
                debug!("cmd send response:{:?}", request_id);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<RequestResponseEvent<AckRequest, AckResponse>> for WhitenoiseBehaviour {
    fn inject_event(&mut self, message: RequestResponseEvent<AckRequest, AckResponse>) {
        match message {
            RequestResponseEvent::InboundFailure { peer, request_id, error } => {
                debug!("ack inbound failure:{:?}", error);
            }
            RequestResponseEvent::OutboundFailure { peer, request_id: req_id, error } => {
                debug!("ack outbound failure:{:?}", error);
            }
            RequestResponseEvent::Message { peer, message } => {
                debug!("ack received mssage:{:?}", message);
                match message {
                    RequestResponseMessage::Request { request_id, request, channel } => {
                        let AckRequest(data) = request.clone();
                        let mut guard = self.event_bus.write().unwrap();
                        debug!("receive {}", data.command_id);
                        let mut sender_option = (*guard).remove(&(data.command_id));
                        match sender_option {
                            Some(mut sender) => {
                                debug!("ack prepare to send");
                                sender.send(request.clone());
                            }
                            None => {
                                debug!("ack prepare to send,but no sender");
                            }
                        }
                    }
                    _ => {}
                }
            }
            RequestResponseEvent::ResponseSent { peer, request_id } => {
                debug!("ack send response:{:?}", request_id);
            }
        }
    }
}
