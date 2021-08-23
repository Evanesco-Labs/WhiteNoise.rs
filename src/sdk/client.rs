use libp2p::{
    Multiaddr, PeerId,
};

use crate::network::{connection::CircuitConn, node::Node};
use futures::{StreamExt};

use crate::sdk::{host, host::RunMode};
use async_trait::async_trait;
use log::{debug,info};
use crate::account::account_service::Account;

pub async fn process_new_stream(mut node: Node) {
    loop {
        let stream = node.wait_for_relay_stream().await;
        debug!("have new stream");
        async_std::task::spawn(crate::network::relay_event_handler::relay_event_handler(stream.clone(), node.clone(), None));
    }
}

pub async fn process_new_session(node: Node, sender: futures::channel::mpsc::UnboundedSender<String>, exist_session: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, bool>>>) {
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
                    sender.unbounded_send(session_id.clone()).unwrap();
                    exist_session.lock().unwrap().insert(session_id, true);
                }
            }
        }
        async_std::task::sleep(std::time::Duration::from_millis(50)).await;
    };
}

#[allow(dead_code)]
pub struct WhiteNoiseClient {
    pub node: Node,
    bootstrap_addr_str: String,
    bootstrap_peer_id: PeerId,
    pub new_connected_session: futures::channel::mpsc::UnboundedReceiver<String>,
    exist_session: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, bool>>>,
}

#[async_trait]
pub trait Client {
    ///Get nodes of the WhiteNoise network.
    async fn get_main_net_peers(&mut self, cnt: i32) -> Vec<PeerId>;
    ///Register to a node as proxy to access the WhiteNoise network.
    async fn register(&mut self, peer_id: PeerId) -> bool;
    ///Dial another client, and returns a unique session id if dialing success.
    async fn dial(&mut self, remote_id: String) -> String;
    ///Get circuit connection of certain session id.
    fn get_circuit(&self, session_id: &str) -> Option<CircuitConn>;
    ///Send message in a circuit connection with session_id.
    async fn send_message(&self, session_id: &str, data: &[u8]);
    ///Close the circuit connection with session_id.
    async fn disconnect_circuit(&mut self, session_id: String);
    ///Get client's unique WhiteNoise Id.
    fn get_whitenoise_id(&self) -> String;
    ///Pop latest inbound or outbound circuit connection's session id.
    async fn notify_next_session(&mut self) -> Option<String>;
}

impl WhiteNoiseClient {
    pub fn init(bootstrap_addr_str: String, key_type: crate::account::key_types::KeyType, keypair: Option<libp2p::identity::Keypair>) -> Self {
        let node = host::start(None, Some(bootstrap_addr_str.clone()), RunMode::Client, keypair, key_type);
        let parts: Vec<&str> = bootstrap_addr_str.split('/').collect();
        let bootstrap_peer_id_str = parts.last().unwrap();
        let bootstrap_peer_id = PeerId::from_bytes(bs58::decode(bootstrap_peer_id_str).into_vec().unwrap().as_slice()).unwrap();

        let (new_connected_sender, new_connected_receiver) = futures::channel::mpsc::unbounded();
        let exist_session = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        async_std::task::spawn(process_new_stream(node.clone()));
        async_std::task::spawn(process_new_session(node.clone(), new_connected_sender, exist_session.clone()));

        let whitenoise_client = WhiteNoiseClient {
            node,
            bootstrap_addr_str,
            bootstrap_peer_id,
            new_connected_session: new_connected_receiver,
            exist_session,
        };
        info!("[WhiteNoise] local whitenoise id:{}", whitenoise_client.get_whitenoise_id());
        whitenoise_client
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
        self.node.circuit_map.read().unwrap().get(session_id).cloned()
    }
    async fn send_message(&self, session_id: &str, data: &[u8]) {
        self.node.send_message(session_id, data).await;
    }
    async fn disconnect_circuit(&mut self, session_id: String) {
        self.node.handle_close_session(&session_id).await;
    }
    fn get_whitenoise_id(&self) -> String {
        Account::from_keypair_to_whitenoise_id(&self.node.keypair)
    }
    async fn notify_next_session(&mut self) -> Option<String> {
        self.new_connected_session.next().await
    }
}