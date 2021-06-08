use whitenoisers::{sdk::{host, host::RunMode}, account, network::{self, node::Node}};
use log::{info, debug, warn, error};
use env_logger::Builder;
use whitenoisers::sdk::client::{process_new_session, process_new_stream, WhiteNoiseClient, Client};
use libp2p::core::PeerId;
use std::collections::HashMap;
use whitenoisers::network::utils::from_whitenoise_to_hash;
use bytes::BufMut;

#[async_std::test]
async fn get_mainnet_test() {
    //start boot strap
    let port = Some(String::from("3331"));
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let key_type = String::from("ed25519");

    let boot = start_server(None, port, key_type.clone(), keypair).await;
    let boot_id = boot.get_id();
    let mut bootstrap_addr = String::from("/ip4/127.0.0.1/tcp/3331/p2p/");
    bootstrap_addr.push_str(boot_id.as_str());

    //start nodes
    let cnt = 2;
    let mut node_vec = Vec::new();
    let mut port_int = 6661;
    for _i in 0..cnt {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let node = start_server(Some(bootstrap_addr.clone()), Some(port_int.to_string()), key_type.clone(), keypair).await;
        port_int = port_int + 1;
        node_vec.push(node);
    }

    let mut node_id_map = HashMap::new();
    for node in node_vec.iter() {
        node_id_map.insert(node.get_id(), true);
    }

    //waiting for update peerlist
    std::thread::sleep(std::time::Duration::from_secs(5));

    //start client
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut client = WhiteNoiseClient::init(bootstrap_addr.clone(), account::key_types::KeyType::from_str(key_type.as_str()), Some(keypair));
    let mut peers = client.get_main_net_peers(10).await;
    assert_eq!(peers.len(), cnt as usize);
    for id in peers {
        assert!(node_id_map.contains_key(id.to_string().as_str()));
    }
}

#[async_std::test]
async fn register_test() {
    //start boot strap
    let port = Some(String::from("3341"));
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let key_type = String::from("ed25519");

    let boot = start_server(None, port, key_type.clone(), keypair).await;
    let boot_id = boot.get_id();
    let mut bootstrap_addr = String::from("/ip4/127.0.0.1/tcp/3341/p2p/");
    bootstrap_addr.push_str(boot_id.as_str());

    //start nodes
    let mut port_int = 6671;
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let node = start_server(Some(bootstrap_addr.clone()), Some(port_int.to_string()), key_type.clone(), keypair).await;

    //waiting for update peerlist
    std::thread::sleep(std::time::Duration::from_secs(5));

    //start client
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut client = WhiteNoiseClient::init(bootstrap_addr.clone(), account::key_types::KeyType::from_str(key_type.as_str()), Some(keypair));
    let mut peers = client.get_main_net_peers(10).await;
    assert_eq!(peers.len(), 1 as usize);

    let proxy_id = peers.get(0).unwrap().clone();

    //register to proxy
    let success = client.register(proxy_id.clone()).await;
    assert!(success);

    assert_eq!(Some(proxy_id.clone()), client.node.proxy_id.clone());
    let whitenoise_id = client.get_whitenoise_id();
    let whitenoise_hash_ori = from_whitenoise_to_hash(client.get_whitenoise_id().as_str());

    {
        let guard = node.client_peer_map.read().unwrap();
        assert!(guard.contains_key(client.node.get_id().as_str()));
        let whitenoise_hash = guard.get(client.node.get_id().as_str()).unwrap().as_str();
        assert_eq!(whitenoise_hash_ori.as_str(), whitenoise_hash);
    }

    {
        let guard = node.client_wn_map.read().unwrap();
        assert!(guard.contains_key(whitenoise_hash_ori.as_str()));
        let client_info = guard.get(whitenoise_hash_ori.as_str()).unwrap();
        assert_eq!(whitenoise_id.clone(), client_info.whitenoise_id);
        assert_eq!(client.node.get_id(), client_info.peer_id.to_base58());
    }
}

#[async_std::test]
async fn circuit_connection_test() {
    //start boot strap
    let port = Some(String::from("3351"));
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let key_type = String::from("ed25519");

    let boot = start_server(None, port, key_type.clone(), keypair).await;
    let boot_id = boot.get_id();
    let mut bootstrap_addr = String::from("/ip4/127.0.0.1/tcp/3351/p2p/");
    bootstrap_addr.push_str(boot_id.as_str());

    //start nodes
    let mut port_int = 6681;
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let node = start_server(Some(bootstrap_addr.clone()), Some(port_int.to_string()), key_type.clone(), keypair).await;

    //waiting for update peerlist
    std::thread::sleep(std::time::Duration::from_secs(1));

    //start answer
    let answer_keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut answer = WhiteNoiseClient::init(bootstrap_addr.clone(), account::key_types::KeyType::from_str(key_type.as_str()), Some(answer_keypair));
    let mut peers_caller = answer.get_main_net_peers(10).await;
    assert_eq!(peers_caller.len(), 1 as usize);

    let answer_proxy_id = peers_caller.get(0).unwrap().clone();

    //register to proxy
    let success = answer.register(answer_proxy_id.clone()).await;
    assert!(success);

    //start caller
    let caller_keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut caller = WhiteNoiseClient::init(bootstrap_addr.clone(), account::key_types::KeyType::from_str(key_type.as_str()), Some(caller_keypair));
    let mut peers_answer = caller.get_main_net_peers(10).await;
    assert_eq!(peers_answer.len(), 1 as usize);

    let caller_proxy_id = peers_answer.get(0).unwrap().clone();

    //register to proxy
    let success = caller.register(caller_proxy_id.clone()).await;
    assert!(success);

    //dial
    let session_id_cal = caller.dial(answer.get_whitenoise_id()).await;

    //wait for listen
    std::thread::sleep(std::time::Duration::from_secs(1));
    let session_id_ans = answer.notify_next_session().await.unwrap();

    assert_eq!(session_id_ans.as_str(), session_id_cal.as_str());

    let conn_caller = caller.get_circuit(session_id_cal.as_str()).unwrap();
    assert!(conn_caller.transport_state.is_some());

    let mut conn_ans = answer.get_circuit(session_id_ans.as_str()).unwrap();
    assert!(conn_ans.transport_state.is_some());


    //relay node handle session
    {
        let guard = node.session_map.read().unwrap();
        assert!(guard.contains_key(session_id_cal.as_str()));
    }

    //read and write
    let message = "hello there".as_bytes();
    let mut payload = Vec::with_capacity(4 + message.len());
    payload.put_u32(message.len() as u32);
    payload.chunk_mut().copy_from_slice(message);
    unsafe {
        payload.advance_mut(message.len());
    }
    caller.send_message(session_id_cal.as_str(), &payload).await;

    std::thread::sleep(std::time::Duration::from_secs(1));

    let mut buf = [0u8; 1024];
    let msg_read = conn_ans.read(&mut buf).await;
    assert_eq!(msg_read.as_slice(), message);
}


pub async fn start_server(bootstrap_addr_option: Option<String>, port_option: Option<String>, key_type: String, key_pair: libp2p::identity::Keypair) -> Node {
    let mut node = host::start(port_option, bootstrap_addr_option, RunMode::Server, Some(key_pair), account::key_types::KeyType::from_str(key_type.as_str()));
    let node_c = node.clone();
    async_std::task::spawn(async move {
        loop {
            let wraped_stream = node.wait_for_relay_stream().await;
            debug!("{} have connected", wraped_stream.remote_peer_id.to_base58());
            async_std::task::spawn(network::relay_event_handler::relay_event_handler(wraped_stream.clone(), node.clone(), None));
        }
    });
    node_c
}
