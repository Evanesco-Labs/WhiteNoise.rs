use whitenoisers::{sdk::{host, host::RunMode}, account, network::{self, node::Node}};
use log::{info, debug, warn, error};
use env_logger::Builder;
use account::account_service::Account;
use whitenoisers::sdk::host::start_server;
use whitenoisers::sdk::client::{WhiteNoiseClient, Client};
use libp2p::{PeerId, Transport, mplex, gossipsub, identify, Swarm};
use whitenoisers::gossip_proto;
use prost::Message;
use futures::StreamExt;
use libp2p::core::{identity, upgrade, Multiaddr};
use futures::channel::mpsc;
use libp2p::gossipsub::{GossipsubMessage, GossipsubEvent, ValidationMode, MessageAuthenticity, IdentTopic as Topic, MessageId};
use libp2p::noise::{X25519Spec, Keypair, NoiseConfig};
use libp2p::tcp::TcpConfig;
use libp2p::kad::{Kademlia, KademliaEvent, KademliaConfig};
use libp2p::kad::store::MemoryStore;
use libp2p::swarm::{NetworkBehaviourEventProcess, SwarmBuilder};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hasher, Hash};
use std::time::Duration;
use libp2p::NetworkBehaviour;
use futures::future::FutureExt;

#[test]
fn test_ecies_ed25519() {
    let message = "hello ecies".as_bytes();
    let keytype = account::key_types::KeyType::ED25519;
    let keypair = libp2p::identity::Keypair::generate_ed25519();

    let whitenoise_id = Account::from_keypair_to_whitenoise_id(&keypair);
    println!("whitenosie_id {}", whitenoise_id);

    let mut cyphertext = Vec::new();
    if let libp2p::identity::Keypair::Ed25519(key) = keypair.clone() {
        cyphertext = Account::ecies_ed25519_encrypt(&key.public().encode(), message);
        let plaintext = Account::ecies_ed25519_decrypt(&keypair, cyphertext.as_slice()).unwrap();
        assert_eq!(message.to_vec(), plaintext);
    }
}

#[test]
fn test_ecies_secp256k1() {
    let message = "hello ecies".as_bytes();
    let keytype = account::key_types::KeyType::SECP256K1;
    let keypair = libp2p::identity::Keypair::generate_secp256k1();

    let whitenoise_id = Account::from_keypair_to_whitenoise_id(&keypair);
    println!("whitenosie_id {}", whitenoise_id);

    if let libp2p::identity::Keypair::Secp256k1(key) = keypair.clone() {
        let cyphertext = Account::ecies_encrypt(&key.public().encode(), message);
        let plaintext = Account::ecies_decrypt(&keypair, cyphertext.as_slice()).unwrap();
        assert_eq!(message.to_vec(), plaintext);
    }
}

//Tests Entry node publish encrypted gossip message, and decrypted by keypair of the answer client.
#[async_std::test]
async fn test_gossip_encryption() {
    // init log
    let env = env_logger::Env::new().filter_or("MY_LOG", "info");
    let mut builder = Builder::new();
    builder.parse_env(env);
    builder.format_timestamp_millis();
    builder.init();

    //start boot strap
    let port = Some(String::from("3351"));
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let key_type = String::from("ed25519");

    let boot = start_server(None, port, key_type.clone(), Some(keypair)).await;
    let boot_id = boot.get_id();
    let mut bootstrap_addr = String::from("/ip4/127.0.0.1/tcp/3351/p2p/");
    bootstrap_addr.push_str(boot_id.as_str());

    std::thread::sleep(Duration::from_secs(1));

    //start entry
    let mut port_int = 6681;
    let entry_keypair = libp2p::identity::Keypair::generate_ed25519();
    let entry = start_server(Some(bootstrap_addr.clone()), Some(port_int.to_string()), key_type.clone(), Some(entry_keypair.clone())).await;
    let entry_peer_id = entry_keypair.public().into_peer_id();

    std::thread::sleep(Duration::from_secs(1));

    //start joint
    let mut port_int = 6682;
    let joint_keypair = libp2p::identity::Keypair::generate_ed25519();
    let joint = start_server(Some(bootstrap_addr.clone()), Some(port_int.to_string()), key_type.clone(), Some(joint_keypair.clone())).await;
    let joint_peer_id = joint_keypair.public().into_peer_id();

    std::thread::sleep(Duration::from_secs(1));

    //start dummy listener. Dummy listener listen gossip messages but doesn't handle.
    let mut dummy = start_dummy_listener(Some("6683".to_string()), Some(bootstrap_addr.clone()));

    //waiting for update peerlist
    std::thread::sleep(std::time::Duration::from_secs(1));

    //start client
    let c1_keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut c1 = WhiteNoiseClient::init(bootstrap_addr.clone(), account::key_types::KeyType::from_text_str(key_type.as_str()), Some(c1_keypair));

    let _peers = c1.get_main_net_peers(10).await;
    assert_eq!(_peers.len(), 3 as usize);

    //register client to entry
    let success = c1.register(entry_peer_id.clone()).await;
    assert!(success);

    //Generate another keypair to dial to. No need to actually start this client.
    let c2_keypair = libp2p::identity::Keypair::generate_ed25519();
    let c2_whtienoise_id = Account::from_keypair_to_whitenoise_id(&c2_keypair);

    //Dial c2.
    //After receive dial request, entry node will broadcast an encrypted gossip msg. Gossip msg is only able to decrypt by c2 keypair.
    let session_id = c1.dial(c2_whtienoise_id.clone()).await;

    //Since there is only one node able to act as Joint. We can simulate Entry Node's process to generate a gossip msg in plaintext.
    let gossip_expect = gossip_proto::Negotiate { join: joint_peer_id.to_base58(), session_id: session_id.clone(), destination: c2_whtienoise_id.clone(), sig: Vec::new() };
    let mut neg_data = Vec::new();
    gossip_expect.encode(&mut neg_data).unwrap();


    //Dummy node listen for the encrypted gossip message send by Entry Node.
    let gossipsub_message = dummy.publish_receiver.next().await.unwrap();

    //Decode this gossip message with c2 keypair.
    let encrypted_neg = gossip_proto::EncryptedNeg::decode(gossipsub_message.data.as_slice()).unwrap();
    let cypher = encrypted_neg.cypher;
    let decrypt_res = match c2_keypair {
        identity::Keypair::Ed25519(ref x) => {
            crate::account::account_service::Account::ecies_ed25519_decrypt(&c2_keypair, &cypher).unwrap()
        }
        identity::Keypair::Secp256k1(ref x) => {
            crate::account::account_service::Account::ecies_decrypt(&c2_keypair, &cypher).unwrap()
        }
        _ => {
            panic!("keypair not ed25519 or secp256k1");
        }
    };

    let gossip_message_decoded = gossip_proto::Negotiate::decode(neg_data.as_slice()).unwrap();

    //Check if the gossip message is the same as we expect.
    assert_eq!(gossip_message_decoded, gossip_expect);
}

#[derive(NetworkBehaviour)]
pub struct DummyListenerBehaviour {
    pub gossip_sub: gossipsub::Gossipsub,
    pub kad_dht: Kademlia<MemoryStore>,
    pub identify_behaviour: identify::Identify,
    #[behaviour(ignore)]
    pub publish_channel: mpsc::UnboundedSender<GossipsubMessage>,
}

impl NetworkBehaviourEventProcess<()> for DummyListenerBehaviour {
    fn inject_event(&mut self, message: ()) {
        info!("[WhiteNoise] receive inner behaviour message:{:?}", message);
    }
}

impl NetworkBehaviourEventProcess<GossipsubEvent> for DummyListenerBehaviour {
    fn inject_event(&mut self, event: GossipsubEvent) {
        match event {
            GossipsubEvent::Message { propagation_source, message_id, message } => {
                self.publish_channel.unbounded_send(message).unwrap();
            }
            GossipsubEvent::Subscribed { peer_id, .. } => {}
            _ => {}
        }
    }
}

impl NetworkBehaviourEventProcess<KademliaEvent> for DummyListenerBehaviour {
    fn inject_event(&mut self, message: KademliaEvent) {
        match message {
            KademliaEvent::RoutablePeer { peer, address } => {
                info!("[WhiteNoise] routable peer,peer:{:?},addresses:{:?}", peer, address);
            }
            KademliaEvent::RoutingUpdated { peer, addresses, old_peer } => {
                info!("[WhiteNoise] routing updated,peer:{:?},addresses:{:?}", peer, addresses);
            }
            KademliaEvent::UnroutablePeer { peer } => {
                info!("[WhiteNoise] unroutable peer:{}", peer)
            }
            KademliaEvent::QueryResult { id, result, .. } => {
                info!("[WhiteNoise] query result:{:?}", result);
            }
            KademliaEvent::PendingRoutablePeer { peer, address } => {
                info!("[WhiteNoise] pending routable peer,id:{:?},address:{}", peer, address);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<identify::IdentifyEvent> for DummyListenerBehaviour {
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

// DummyListener is only used for testing. DummyListener can not handle requests or act as a relay node.
// It just subscribe to the gossip_sub topic and listen to gossip messages.
struct DummyListener {
    publish_receiver: mpsc::UnboundedReceiver<GossipsubMessage>,
}

fn start_dummy_listener(port_option: std::option::Option<String>, bootstrap_addr_option: std::option::Option<String>) -> DummyListener {
    let id_keys = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = id_keys.public().into_peer_id();

    let noise_keys = Keypair::<X25519Spec>::new().into_authentic(&id_keys).unwrap();
    let trans = TcpConfig::new()
        .nodelay(true)
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::default())
        .timeout(std::time::Duration::from_millis(150))
        .boxed();

    let identify_behaviour = identify::Identify::new(identify::IdentifyConfig::new(String::from("/ipfs/id/1.0.0"), id_keys.public()).with_initial_delay(std::time::Duration::from_millis(500)).with_interval(std::time::Duration::from_secs(5 * 60)));

    let mut dht_cfg = KademliaConfig::default();
    let whitnoise_dht_protocol: &[u8] = b"/whitenoise_dht/kad/1.0.0";
    dht_cfg.set_protocol_name(std::borrow::Cow::Borrowed(whitnoise_dht_protocol));
    dht_cfg.set_query_timeout(std::time::Duration::from_secs(5 * 60));
    let store = MemoryStore::new(peer_id.clone());
    let mut kad_behaviour = Kademlia::with_config(peer_id.clone(), store, dht_cfg);
    match bootstrap_addr_option {
        None => {}
        Some(bootstrap_addr) => {
            let parts: Vec<&str> = bootstrap_addr.split('/').collect();
            let bootstrap_peer_id_str = parts.get(parts.len() - 1).unwrap();
            let bootstrap_peer_id = PeerId::from_bytes(bs58::decode(bootstrap_peer_id_str).into_vec().unwrap().as_slice()).unwrap();
            let mut bootstrap_addr_multiaddr: Multiaddr = bootstrap_addr.parse().unwrap();
            let index_opt = bootstrap_addr.find("p2p");
            if index_opt.is_some() {
                let index = index_opt.unwrap();
                let bootstrap_addr_parts = bootstrap_addr.split_at(index - 1);
                bootstrap_addr_multiaddr = bootstrap_addr_parts.0.parse().unwrap();
            }
            kad_behaviour.add_address(&bootstrap_peer_id, bootstrap_addr_multiaddr);
        }
    }

    let message_id_fn = |message: &GossipsubMessage| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        MessageId::from(s.finish().to_string())
    };

    // Set a custom gossipsub
    let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
        .heartbeat_interval(std::time::Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
        .validation_mode(ValidationMode::Permissive) // This sets the kind of message validation. The default is Strict (enforce message signing)
        .message_id_fn(message_id_fn) // content-address messages. No two messages of the
        // same content will be propagated.
        .build()
        .expect("Valid config");
    // build a gossipsub network behaviour
    let mut gossipsub: gossipsub::Gossipsub =
        gossipsub::Gossipsub::new(MessageAuthenticity::Anonymous, gossipsub_config)
            .expect("Correct configuration");
    let topic = Topic::new("noise_topic");
    // subscribes to our topic
    gossipsub.subscribe(&topic).unwrap();


    let (publish_sender, publish_recerver) = mpsc::unbounded();
    let dummy_listener_behaviour = DummyListenerBehaviour {
        gossip_sub: gossipsub,
        kad_dht: kad_behaviour,
        identify_behaviour,
        publish_channel: publish_sender,
    };

    let mut swarm1 = SwarmBuilder::new(trans, dummy_listener_behaviour, peer_id)
        .executor(Box::new(|fut| {
            async_std::task::spawn(fut);
        }))
        .build();

    let to_search: PeerId = identity::Keypair::generate_secp256k1().public().into();
    swarm1.behaviour_mut().kad_dht.get_closest_peers(to_search);

    match port_option {
        None => {}
        Some(port) => {
            let mut addr = String::from("/ip4/0.0.0.0/tcp/");
            addr.push_str(port.as_str());
            Swarm::listen_on(&mut swarm1, addr.parse().unwrap()).unwrap();
        }
    }

    async_std::task::spawn(dummy_event_loop(swarm1));

    return DummyListener {
        publish_receiver: publish_recerver,
    };
}

pub async fn dummy_event_loop(mut swarm1: Swarm<DummyListenerBehaviour>) {
    let mut listening = false;
    loop {
        futures::select! {
            event = swarm1.next().fuse() => {
                panic!("Unexpected event: {:?}", event);
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