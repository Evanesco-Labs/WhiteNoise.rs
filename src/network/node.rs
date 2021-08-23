use libp2p::{Multiaddr, PeerId};
use whitenoise_behaviour::{AddPeerAddresses};
use crate::{request_proto};
use futures::{channel::oneshot};
use prost::Message;
use futures::{channel::mpsc};
use log::{info, debug};
use super::{protocols::relay_behaviour::WrappedStream};
use super::protocols::proxy_protocol::{ProxyRequest};
use super::protocols::ack_protocol::{AckRequest};
use std::collections::VecDeque;
use crate::models::client_info::ClientInfo;
use super::whitenoise_behaviour::{self, NodeRequest, NodeNewStream, NodeProxyRequest};
use crate::account::account_service::Account;
use super::session::{Session};
use super::utils::{from_whitenoise_to_hash, from_request_get_id};
use super::connection::CircuitConn;
use multihash::{Code, MultihashDigest};
use crate::network::const_vb::BUF_LEN;


#[derive(Clone)]
pub struct Probe {
    pub session_id: String,
    pub rand: Vec<u8>,
}

#[derive(Clone)]
pub struct Node {
    pub node_request_sender: mpsc::UnboundedSender<NodeRequest>,
    pub event_bus: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, oneshot::Sender<AckRequest>>>>,
    pub keypair: libp2p::identity::Keypair,
    pub proxy_id: Option<PeerId>,
    pub relay_in_streams: std::sync::Arc<std::sync::RwLock<VecDeque<super::protocols::relay_behaviour::WrappedStream>>>,
    pub relay_out_streams: std::sync::Arc<std::sync::RwLock<VecDeque<super::protocols::relay_behaviour::WrappedStream>>>,
    pub circuit_task: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>>,
    pub client_wn_map: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, ClientInfo>>>,
    pub client_peer_map: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, String>>>,
    pub session_map: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, Session>>>,
    pub circuit_map: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, CircuitConn>>>,
    pub probe_map: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, Probe>>>,
    pub boot_peer_id: Option<libp2p::PeerId>,
}

impl Node {
    pub fn get_id(&self) -> String {
        self.keypair.public().into_peer_id().to_base58()
    }
    pub async fn external_send_node_request_not_wait(&mut self, node_request: NodeRequest) {
        self.node_request_sender.unbounded_send(node_request).unwrap();
    }
    pub async fn external_send_node_request_and_wait(&mut self, key: String, node_request: NodeRequest) -> AckRequest {
        let (ack_request_sender, ack_request_receiver) = oneshot::channel();
        {
            let mut guard = self.event_bus.write().unwrap();
            (*guard).insert(key, ack_request_sender);
        }
        self.node_request_sender.unbounded_send(node_request).unwrap();
        ack_request_receiver.await.unwrap()
    }

    pub async fn get_main_nets(&mut self, cnt: i32, remote_peer_id: PeerId, remote_peer_addr: Multiaddr) -> request_proto::PeersList {
        let mut remote_peer_addr_smallvec = smallvec::SmallVec::<[Multiaddr; 6]>::new();
        remote_peer_addr_smallvec.push(remote_peer_addr.clone());
        let add_peer_addresses = AddPeerAddresses {
            remote_peer_id,
            remote_addr: remote_peer_addr_smallvec,
        };
        self.node_request_sender.unbounded_send(NodeRequest::AddPeerAddressesRequest(add_peer_addresses)).unwrap();

        //generate request for mainnet peers
        let mainnets = request_proto::MainNetPeers {
            max: cnt
        };
        let mut buf_for_main = Vec::new();
        let _data = mainnets.encode(&mut buf_for_main);
        let mut request = request_proto::Request {
            data: buf_for_main,
            req_id: String::from(""),
            from: String::from(""),
            reqtype: request_proto::Reqtype::MainNetPeers as i32,
        };

        let key = from_request_get_id(&request);
        request.req_id = key.clone();

        let proxy_request = ProxyRequest(request.clone());
        let node_proxy_request_get_mainnets = NodeProxyRequest {
            remote_peer_id,
            proxy_request: Some(proxy_request),
            peer_operation: None,
        };
        let AckRequest(data) = self.external_send_node_request_and_wait(key, NodeRequest::ProxyRequest(node_proxy_request_get_mainnets)).await;

        info!("[WhiteNoise] finished get mainnets this turn:{}", data.command_id);
        let peer_list = request_proto::PeersList::decode(data.data.as_slice()).unwrap();
        let mut new_peer_list = request_proto::PeersList { peers: Vec::new() };
        for peer in peer_list.peers.clone() {
            if peer.addr.is_empty() {
                continue;
            }
            new_peer_list.peers.push(peer.clone());
            let mut addr_smallvec = smallvec::SmallVec::<[Multiaddr; 6]>::new();
            peer.addr.iter().for_each(|x| addr_smallvec.push(x.clone().parse().unwrap()));
            let add_addresses_after_getmainnets = AddPeerAddresses {
                remote_peer_id: PeerId::from_bytes(bs58::decode(peer.id).into_vec().unwrap().as_slice()).unwrap(),
                remote_addr: addr_smallvec,
            };
            info!("[WhiteNoise] mainnets address:{:?},peer_id:{:?}", add_addresses_after_getmainnets.remote_addr, add_addresses_after_getmainnets.remote_peer_id);
            self.node_request_sender.unbounded_send(NodeRequest::AddPeerAddressesRequest(add_addresses_after_getmainnets)).unwrap();
        }
        new_peer_list
    }

    pub async fn send_ack(&mut self, ack_request: NodeRequest) {
        self.node_request_sender.unbounded_send(ack_request).unwrap();
    }

    pub async fn register_proxy(&mut self, remote_peer_id: PeerId) -> bool {
        let new_proxy = request_proto::NewProxy {
            time: String::from("60m"),
            white_noise_id: Account::from_keypair_to_whitenoise_id(&self.keypair),
        };
        let mut data_buf = Vec::new();
        if new_proxy.encode(&mut data_buf).is_err() {
            return false;
        };
        let local_peer_id = PeerId::from(self.keypair.public());
        debug!("local peer id:{}", local_peer_id.to_base58());
        let mut request = request_proto::Request {
            data: data_buf,
            req_id: String::from(""),
            from: local_peer_id.to_base58(),
            reqtype: request_proto::Reqtype::NewProxy as i32,
        };

        let key = from_request_get_id(&request);
        request.req_id = key.clone();
        let proxy_request = ProxyRequest(request);

        let node_proxy_request = NodeProxyRequest {
            remote_peer_id,
            proxy_request: Some(proxy_request),
            peer_operation: None,
        };
        let AckRequest(data) = self.external_send_node_request_and_wait(key, NodeRequest::ProxyRequest(node_proxy_request)).await;
        info!("[WhiteNoise] finished register proxy this turn:{:?}", data);
        if data.result {
            self.proxy_id = Some(remote_peer_id);
            true
        } else {
            false
        }
    }

    pub async fn wait_for_out_peer_relay_stream(&mut self, peer_id: PeerId) -> Option<WrappedStream> {
        let mut wait_time = 0;
        loop {
            if wait_time >= 5000 {
                return None;
            }
            let len = self.relay_out_streams.read().unwrap().len();
            let mut index = 0_usize;
            loop {
                if index >= len {
                    break;
                }
                let relay_stream_option = self.relay_out_streams.write().unwrap().pop_front();
                if relay_stream_option.is_some() {
                    let relay_stream = relay_stream_option.unwrap();
                    if relay_stream.remote_peer_id == peer_id {
                        return Some(relay_stream);
                    } else {
                        self.relay_out_streams.write().unwrap().push_back(relay_stream);
                    }
                }
                index += 1;
            }
            async_std::task::sleep(std::time::Duration::from_millis(50)).await;
            wait_time += 50;
        }
    }

    pub async fn wait_for_relay_stream(&mut self) -> WrappedStream {
        loop {
            {
                let relay_stream_option = self.relay_in_streams.write().unwrap().pop_front();
                if relay_stream_option.is_some() {
                    return relay_stream_option.unwrap();
                }
                async_std::task::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }

    pub async fn new_relay_stream(&mut self, peer_id: PeerId) -> Option<WrappedStream> {
        let node_new_stream = NodeNewStream { peer_id };
        let node_request = NodeRequest::NewStreamRequest(node_new_stream);
        let node_c = self.clone();
        async_std::task::spawn(async move {
            let node_request_send_res = node_c.node_request_sender.unbounded_send(node_request);
            if node_request_send_res.is_err() {
                info!("[WhiteNoise] send node request error");
            }
        });
        return self.wait_for_out_peer_relay_stream(peer_id).await;
    }

    pub async fn handle_close_session(&mut self, session_id: &str) {
        let session_opt = self.session_map.write().unwrap().remove(session_id);
        if session_opt.is_some() {
            let session = session_opt.unwrap();
            if session.pair_stream.early_stream.is_some() {
                session.pair_stream.early_stream.unwrap().close().await;
            }
            if session.pair_stream.later_stream.is_some() {
                session.pair_stream.later_stream.unwrap().close().await;
            }
        }
        self.circuit_map.write().unwrap().remove(session_id);
        self.probe_map.write().unwrap().remove(session_id);
    }

    pub async fn new_circuit(&mut self, remote_whitenoise_id: String, local_white_noise_id: String, local_peer_id: String, session_id: String, proxy_id: PeerId) {
        let from = from_whitenoise_to_hash(local_white_noise_id.as_str());
        let to = from_whitenoise_to_hash(remote_whitenoise_id.as_str());
        let new_circuit = request_proto::NewCircuit {
            from,
            to,
            session_id,
        };
        let mut data = Vec::new();
        new_circuit.encode(&mut data).unwrap();
        let mut request = request_proto::Request {
            req_id: String::from(""),
            from: local_peer_id,
            reqtype: request_proto::Reqtype::NewCircuit as i32,
            data,
        };

        let key = from_request_get_id(&request);
        request.req_id = key.clone();

        let proxy_request = ProxyRequest(request.clone());

        let node_proxy_request = NodeProxyRequest {
            remote_peer_id: proxy_id,
            proxy_request: Some(proxy_request),
            peer_operation: None,
        };

        let AckRequest(data) = self.external_send_node_request_and_wait(key, NodeRequest::ProxyRequest(node_proxy_request)).await;

        info!("[WhiteNoise] finished new circuit:{}", data.command_id);
    }

    pub async fn dial(&mut self, remote_whitenoise_id: String) -> String {
        let local_whitenoise_id = crate::account::account_service::Account::from_keypair_to_whitenoise_id(&self.keypair);

        let session_id = generate_session_id(remote_whitenoise_id.clone(), local_whitenoise_id.clone());
        info!("[WhiteNoise] session_id:{}", session_id);
        let (index, pub_bytes) = remote_whitenoise_id.split_at(1);
        let mut composit_bytes: Vec<u8> = vec![index.as_bytes()[0]];
        bs58::decode(pub_bytes).into_vec().unwrap().iter().for_each(|x| composit_bytes.push(*x));
        self.circuit_task.write().unwrap().insert(session_id.clone(), composit_bytes);

        //new relay stream
        let stream = self.new_session_to_peer(&self.proxy_id.unwrap(), session_id.clone(), crate::network::session::SessionRole::CallerRole as i32, crate::network::session::SessionRole::EntryRole as i32).await.unwrap();

        let (sender, receiver) = mpsc::unbounded();

        let circuit_conn = CircuitConn {
            id: session_id.clone(),
            out_stream: stream.clone(),
            in_channel_receiver: std::sync::Arc::new(futures::lock::Mutex::new(receiver)),
            in_channel_sender: sender,
            transport_state: None,
        };
        self.circuit_map.write().unwrap().insert(session_id.clone(), circuit_conn);
        self.new_circuit(remote_whitenoise_id, local_whitenoise_id, self.get_id(), session_id.clone(), self.proxy_id.unwrap()).await;
        session_id
    }

    pub async fn send_message(&self, session_id: &str, message: &[u8]) {
        let cc = self.circuit_map.read().unwrap().get(session_id).cloned();
        let mut circuit_conn = cc.unwrap();
        if circuit_conn.transport_state.is_none() {
            info!("[WhiteNoise] shake not finished");
            return;
        }
        let mut buf = [0u8; BUF_LEN];
        log::debug!("message bytes: {:?}", message);
        circuit_conn.write(message, &mut buf).await;
    }

    pub async fn new_session_to_peer(&mut self, peer_id: &PeerId, session_id: String, my_role: i32, other_role: i32) -> Option<WrappedStream> {
        let wraped_stream = self.new_relay_stream(*peer_id).await?;
        crate::network::utils::write_relay_wake_arc(wraped_stream.clone()).await;
        //set session id
        let key = crate::network::utils::write_set_session_with_role_arc(wraped_stream.clone(), session_id.clone(), other_role).await;
        let (r, s) = futures::channel::oneshot::channel();
        {
            let mut guard = self.event_bus.write().unwrap();
            (*guard).insert(key, r);
        }
        let ack_request = s.await.unwrap();
        let AckRequest(_ack) = ack_request;
        async_std::task::spawn(crate::network::relay_event_handler::relay_event_handler(wraped_stream.clone(), self.clone(), Some(session_id.clone())));

        if !self.session_map.read().unwrap().contains_key(&session_id) {
            let pair_stream = crate::network::session::PairStream { early_stream: Some(wraped_stream.clone()), later_stream: None };
            let session = crate::network::session::Session {
                id: session_id.clone(),
                pair_stream,
                session_role: my_role,
            };
            self.session_map.write().unwrap().insert(session_id.clone(), session);
        } else {
            let new_session = self.session_map.read().unwrap().get(&session_id).map(|session| {
                let mut new_session = session.clone();
                new_session.pair_stream.later_stream = Some(wraped_stream.clone());
                new_session
            });
            self.session_map.write().unwrap().insert(session_id.clone(), new_session.unwrap());
        }
        Some(wraped_stream)
    }
}

pub fn generate_session_id(remote_id: String, local_id: String) -> String {
    let now = std::time::SystemTime::now();
    let time_bytes = &now.duration_since(std::time::UNIX_EPOCH).unwrap().as_micros().to_be_bytes()[8..];
    let to_hash = [local_id.as_bytes(), remote_id.as_bytes(), time_bytes].concat();
    let hash = &Code::Sha2_256.digest(&to_hash).to_bytes()[2..];
    bs58::encode(hash).into_string()
}