use bytes::BufMut;
use libp2p::{
    Multiaddr, PeerId,
};

use crate::network::{connection::CircuitConn, node::Node};

use crate::sdk::{host, host::RunMode};
use async_trait::async_trait;
use log::info;

pub async fn process_new_stream(mut node: Node) {
    loop {
        let mut stream = node.wait_for_relay_stream().await;
        info!("have new stream");
        tokio::spawn(crate::network::relay_event_handler::relay_event_handler(stream.clone(), node.clone(), None));
    }
}

pub async fn process_new_session(mut node: Node, sender: tokio::sync::mpsc::UnboundedSender<String>, exist_session: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, bool>>>) {
    loop {
        let len = node.circuit_map.read().unwrap().len();
        if len > 0 {
            let mut session_id_opt = None;
            let mut circuit_conn = None;
            node.circuit_map.read().unwrap().iter().for_each(|x| {
                if !exist_session.lock().unwrap().contains_key(x.0) {
                    session_id_opt = Some(x.0.clone());
                    circuit_conn = Some(x.1.clone());
                }
            });
            if session_id_opt.is_some() {
                let session_id = session_id_opt.unwrap();
                let cc = circuit_conn.unwrap();
                if cc.transport_state.is_some() {
                    sender.send(session_id.clone());
                    exist_session.lock().unwrap().insert(session_id, true);
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
}


pub struct WhiteNoiseClient {
    node: Node,
    bootstrap_addr_str: String,
    bootstrap_peer_id: PeerId,
    new_connected_session: std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>>,
    exist_session: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, bool>>>,
}

#[async_trait]
pub trait Client {
    async fn get_main_net_peers(&mut self, cnt: i32) -> Vec<PeerId>;
    async fn register(&mut self, peer_id: PeerId) -> bool;
    async fn dial(&mut self, remote_id: String) -> String;
    fn get_circuit(&self, session_id: &str) -> Option<CircuitConn>;
    async fn send_message(&self, session_id: &str, data: &[u8]);
    async fn disconnect_circuit(&mut self, session_id: String);
    fn get_whitenoise_id(&self) -> String;
    async fn notify_next_session(&mut self) -> Option<String>;
}

impl WhiteNoiseClient {
    pub fn init(bootstrap_addr_str: String, key_type: crate::account::key_types::KeyType) -> Self {
        let mut node = host::start(None, Some(bootstrap_addr_str.clone()), RunMode::Client, None, key_type);
        let parts: Vec<&str> = bootstrap_addr_str.split('/').collect();
        let bootstrap_peer_id_str = parts.get(parts.len() - 1).unwrap();
        let bootstrap_peer_id = PeerId::from_bytes(bs58::decode(bootstrap_peer_id_str).into_vec().unwrap().as_slice()).unwrap();

        let (new_connected_sender, new_connected_receiver) = tokio::sync::mpsc::unbounded_channel();
        let exist_session = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        tokio::spawn(process_new_stream(node.clone()));
        tokio::spawn(process_new_session(node.clone(), new_connected_sender, exist_session.clone()));
        return WhiteNoiseClient {
            node: node,
            bootstrap_addr_str: bootstrap_addr_str,
            bootstrap_peer_id: bootstrap_peer_id,
            new_connected_session: std::sync::Arc::new(tokio::sync::Mutex::new(new_connected_receiver)),
            exist_session: exist_session.clone(),
        };
    }
}

#[async_trait]
impl Client for WhiteNoiseClient {
    async fn get_main_net_peers(&mut self, _cnt: i32) -> Vec<PeerId> {
        let bootstrap_addr: Multiaddr = self.bootstrap_addr_str.parse().unwrap();
        let peer_list = self.node.get_main_nets(10, self.bootstrap_peer_id, bootstrap_addr).await;
        let mut peer_id_vec = Vec::with_capacity(peer_list.peers.len());
        peer_list.peers.iter().for_each(|x| {
            peer_id_vec.push(PeerId::from_bytes(bs58::decode(x.id.as_str()).into_vec().unwrap().as_slice()).unwrap());
        });
        return peer_id_vec;
    }
    async fn register(&mut self, peer_id: PeerId) -> bool {
        return self.node.register_proxy(peer_id).await;
    }
    async fn dial(&mut self, remote_id: String) -> String {
        return self.node.dial(remote_id).await;
    }
    fn get_circuit(&self, session_id: &str) -> Option<CircuitConn> {
        self.node.circuit_map.read().unwrap().get(session_id).and_then(|x| {
            Some(x.clone())
        })
    }
    async fn send_message(&self, session_id: &str, data: &[u8]) {
        self.node.send_message(session_id, data).await;
    }
    async fn disconnect_circuit(&mut self, session_id: String) {
        self.node.handle_close_session(&session_id).await;
    }
    fn get_whitenoise_id(&self) -> String {
        self.node.get_id()
    }
    async fn notify_next_session(&mut self) -> Option<String> {
        self.new_connected_session.lock().await.recv().await
    }
}