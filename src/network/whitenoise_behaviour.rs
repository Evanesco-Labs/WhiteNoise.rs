use libp2p::{PeerId, gossipsub, identify, Swarm};
use smallvec::SmallVec;
use tokio::sync::{mpsc};
use futures::{channel::oneshot};
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
use libp2p::kad::{Kademlia, KademliaEvent};
use libp2p::kad::record::store::MemoryStore;
use libp2p::gossipsub::{GossipsubMessage, GossipsubEvent, IdentTopic};
use libp2p::kad::kbucket::NodeStatus;

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


#[derive(NetworkBehaviour)]
pub struct WhitenoiseServerBehaviour {
    pub whitenoise_behaviour: WhitenoiseBehaviour,
    pub gossip_sub: gossipsub::Gossipsub,
    pub kad_dht: Kademlia<MemoryStore>,
    pub identify_behaviour: identify::Identify,
    #[behaviour(ignore)]
    pub publish_channel: mpsc::UnboundedSender<GossipsubMessage>,
}

impl NetworkBehaviourEventProcess<()> for WhitenoiseServerBehaviour {
    fn inject_event(&mut self, message: ()) {
        info!("receive inner behaviour message:{:?}", message);
    }
}

impl NetworkBehaviourEventProcess<GossipsubEvent> for WhitenoiseServerBehaviour {
    fn inject_event(&mut self, event: GossipsubEvent) {
        match event {
            GossipsubEvent::Message { propagation_source, message_id, message } => {
                self.publish_channel.send(message);
            }
            GossipsubEvent::Subscribed { peer_id, .. } => {}
            _ => {}
        }
    }
}

impl NetworkBehaviourEventProcess<KademliaEvent> for WhitenoiseServerBehaviour {
    fn inject_event(&mut self, message: KademliaEvent) {
        match message {
            KademliaEvent::RoutablePeer { peer, address } => {
                info!("routable peer,peer:{:?},addresses:{:?}", peer, address);
            }
            KademliaEvent::RoutingUpdated { peer, addresses, old_peer } => {
                info!("routing updated,peer:{:?},addresses:{:?}", peer, addresses);
            }
            KademliaEvent::UnroutablePeer { peer } => {
                info!("unroutable peer:{}", peer)
            }
            KademliaEvent::QueryResult { id, result, .. } => {
                info!("query result:{:?}", result);
            }
            KademliaEvent::PendingRoutablePeer { peer, address } => {
                info!("pending routable peer,id:{:?},address:{}", peer, address);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<identify::IdentifyEvent> for WhitenoiseServerBehaviour {
    // Called when `identity` produces an event.
    fn inject_event(&mut self, message: identify::IdentifyEvent) {
        match message {
            identify::IdentifyEvent::Received { peer_id, info } => {
                debug!("Received: '{:?}' from {:?}", info, peer_id);
                // gossipsub start connections to node except the bootstrap, after connection starts the identity event, put addresses into the KadDht store.
                let identify::IdentifyInfo { listen_addrs, protocols, .. } = info;
                let mut is_server_node = false;
                protocols.iter().for_each(|x| {
                    let find = x.find("meshsub");
                    if find.is_some() {
                        is_server_node = true;
                    }
                });
                if is_server_node == true {
                    for addr in listen_addrs {
                        debug!("identify addr:{:?}", addr);
                        self.kad_dht.add_address(&peer_id, addr);
                    }
                } else {
                    debug!("not server node");
                }
            }
            identify::IdentifyEvent::Sent { peer_id } => {
                debug!("Sent: '{:?}'", peer_id);
            }
            identify::IdentifyEvent::Error { peer_id, error } => {
                debug!("identify error: '{:?}',error: '{:?}'", peer_id, error);
            }
            identify::IdentifyEvent::Pushed { peer_id } => {}
        }
    }
}

#[derive(NetworkBehaviour)]
pub struct WhitenoiseClientBehaviour {
    pub whitenoise_behaviour: WhitenoiseBehaviour,
    pub identify_behaviour: identify::Identify,
}

impl NetworkBehaviourEventProcess<()> for WhitenoiseClientBehaviour {
    fn inject_event(&mut self, message: ()) {
        info!("receive inner behaviour message:{:?}", message);
    }
}

impl NetworkBehaviourEventProcess<identify::IdentifyEvent> for WhitenoiseClientBehaviour {
    // Called when `identity` produces an event.
    fn inject_event(&mut self, message: identify::IdentifyEvent) {}
}


pub async fn whitenoise_client_event_loop(mut swarm1: Swarm<WhitenoiseClientBehaviour>, mut node_request_receiver: tokio::sync::mpsc::UnboundedReceiver<NodeRequest>) {
    loop {
        tokio::select! {
            event = swarm1.next() => {
                panic!("Unexpected event: {:?}", event);
            }
            Some(node_request) = node_request_receiver.recv() =>{
                debug!("receive node request for client");
                match node_request{
                    NodeRequest::ProxyRequest(node_proxy_request)=>{
                        swarm1.behaviour_mut().whitenoise_behaviour.proxy_behaviour.send_request(&(node_proxy_request.remote_peer_id),node_proxy_request.proxy_request.unwrap().clone());
                    }
                    NodeRequest::CmdRequest(node_cmd_request) =>{
                        swarm1.behaviour_mut().whitenoise_behaviour.cmd_behaviour.send_request(&node_cmd_request.remote_peer_id,node_cmd_request.cmd_request.unwrap().clone());
                    }
                    NodeRequest::AddPeerAddressesRequest(add_peer_addresses) =>{
                        add_peer_addresses.remote_addr.iter().for_each(|x|{
                            swarm1.behaviour_mut().whitenoise_behaviour.ack_behaviour.add_address(&(add_peer_addresses.remote_peer_id),x.clone());
                            swarm1.behaviour_mut().whitenoise_behaviour.proxy_behaviour.add_address(&(add_peer_addresses.remote_peer_id),x.clone());
                        });
                        swarm1.behaviour_mut().whitenoise_behaviour.relay_behaviour.addresses.insert(add_peer_addresses.remote_peer_id, add_peer_addresses.remote_addr);
                    }
                    NodeRequest::AckRequest(node_ack_request)=>{
                        debug!("receive node request for client,prepare to send ack");
                        swarm1.behaviour_mut().whitenoise_behaviour.ack_behaviour.send_request(&(node_ack_request.remote_peer_id),node_ack_request.ack_request.unwrap().clone());

                    }
                    NodeRequest::NewStreamRequest(node_new_stream)=>{
                        let peer_addr = swarm1.behaviour_mut().whitenoise_behaviour.relay_behaviour.addresses_of_peer(&node_new_stream.peer_id);
                        debug!("receive new stream request:{},addresses:{:?}",node_new_stream.peer_id,peer_addr);
                        swarm1.behaviour_mut().whitenoise_behaviour.relay_behaviour.new_stream(&node_new_stream.peer_id);
                    }
                    NodeRequest::GetMainNetsRequest(get_main_nets) =>{}
                    NodeRequest::PublishData(data) =>{}
                }
            }
        }
    }
}

pub async fn whitenoise_server_event_loop(mut swarm1: Swarm<WhitenoiseServerBehaviour>, mut node_request_receiver: tokio::sync::mpsc::UnboundedReceiver<NodeRequest>) {
    let mut listening = false;
    loop {
        tokio::select! {
            event = swarm1.next() => {
                panic!("Unexpected event: {:?}", event);
            }
            Some(node_request) = node_request_receiver.recv() =>{
                debug!("receive node request");
                match node_request{
                    NodeRequest::ProxyRequest(node_proxy_request)=>{
                        swarm1.behaviour_mut().whitenoise_behaviour.proxy_behaviour.send_request(&(node_proxy_request.remote_peer_id),node_proxy_request.proxy_request.unwrap().clone());
                    }
                    NodeRequest::CmdRequest(node_cmd_request) =>{
                        swarm1.behaviour_mut().whitenoise_behaviour.cmd_behaviour.send_request(&node_cmd_request.remote_peer_id,node_cmd_request.cmd_request.unwrap().clone());
                    }
                    NodeRequest::AddPeerAddressesRequest(add_peer_addresses) =>{
                        add_peer_addresses.remote_addr.iter().for_each(|x|{
                            swarm1.behaviour_mut().whitenoise_behaviour.ack_behaviour.add_address(&(add_peer_addresses.remote_peer_id),x.clone());
                            swarm1.behaviour_mut().whitenoise_behaviour.proxy_behaviour.add_address(&(add_peer_addresses.remote_peer_id),x.clone());
                        });
                        swarm1.behaviour_mut().whitenoise_behaviour.relay_behaviour.addresses.insert(add_peer_addresses.remote_peer_id, add_peer_addresses.remote_addr);
                    }
                    NodeRequest::AckRequest(node_ack_request)=>{
                        debug!("prepare to send ack");
                        swarm1.behaviour_mut().whitenoise_behaviour.ack_behaviour.send_request(&(node_ack_request.remote_peer_id),node_ack_request.ack_request.unwrap().clone());

                    }
                    NodeRequest::NewStreamRequest(node_new_stream)=>{
                        let peer_addr = swarm1.behaviour_mut().whitenoise_behaviour.relay_behaviour.addresses_of_peer(&node_new_stream.peer_id);
                        debug!("receive new stream request:{},addresses:{:?}",node_new_stream.peer_id,peer_addr);
                        swarm1.behaviour_mut().whitenoise_behaviour.relay_behaviour.new_stream(&node_new_stream.peer_id);
                    }
                    NodeRequest::GetMainNetsRequest(get_main_nets) =>{
                        debug!("prepare to return main net peers");
                        let peers = get_kad_peers(&mut swarm1,get_main_nets.num);
                        get_main_nets.sender.send(peers);
                    }
                    NodeRequest::PublishData(PublishDataRequest{data,sender}) =>{
                        debug!("prepare to publish data");
                        swarm1.behaviour_mut().gossip_sub.publish(IdentTopic::new("noise_topic"),data);
                        sender.send(true);
                    }
                }
            }
        }
        if !listening {
            for addr in Swarm::listeners(&swarm1) {
                println!("Listening on {:?}", addr);
                listening = true;
            }
        }
    }
}


pub fn get_kad_peers(swarm1: &mut Swarm<WhitenoiseServerBehaviour>, number: i32) -> Vec<request_proto::NodeInfo> {
    let mut peers = Vec::new();
    let mut peer_ids = Vec::new();
    let mut cnt = 0;
    swarm1.behaviour_mut().kad_dht.kbuckets().for_each(|kbucket_ref| {
        kbucket_ref.iter().for_each(|entry_ref_view| {
            if cnt < number {
                let node = entry_ref_view.node;
                let status = entry_ref_view.status;
                match status {
                    NodeStatus::Disconnected => {}
                    NodeStatus::Connected => {
                        let peer_id = node.key.preimage().clone().to_base58();
                        peer_ids.push(node.key.preimage().clone());
                        let addresses = node.value.clone();
                        let mut addrs = Vec::new();
                        addresses.iter().for_each(|address| {
                            let address_str = address.to_string();
                            let p2p_str: Vec<&str> = address_str.matches("p2p").collect();
                            if p2p_str.len() <= 0 {
                                addrs.push(address_str);
                            } else {}
                        });
                        let node_info = request_proto::NodeInfo { id: peer_id, addr: addrs };
                        peers.push(node_info);
                        cnt = cnt + 1;
                    }
                }
            }
        })
    });
    return peers;
}