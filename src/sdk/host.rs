use libp2p::{Multiaddr,
             PeerId, Swarm, Transport,
             core::{upgrade::{self}},
             identify, identity, mplex, noise::{NoiseConfig, X25519Spec, Keypair},
             request_response::{ProtocolSupport, RequestResponseConfig, RequestResponse},
             swarm::{SwarmBuilder}, tcp::TokioTcpConfig};
use crate::network::{protocols::cmd_protocol::{CmdCodec, CmdProtocol}, whitenoise_behaviour::{WhitenoiseServerBehaviour, WhitenoiseClientBehaviour}};

use std::{iter};

use tokio::sync::mpsc;
use log::{info, debug, error};
use crate::network::{whitenoise_behaviour::{WhitenoiseBehaviour}};
use crate::network::protocols::proxy_protocol::{ProxyCodec, ProxyProtocol};
use crate::network::protocols::ack_protocol::{AckCodec, AckProtocol};
use crate::network::protocols::relay_behaviour;

use crate::account::account::Account;

use crate::network::whitenoise_behaviour::{self};

use libp2p::kad::{
    Kademlia,
    KademliaConfig,
};
use libp2p::kad::record::store::MemoryStore;

use libp2p::gossipsub::MessageId;
use libp2p::gossipsub::{
    GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::{gossipsub};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::network::node::Node;
use crate::network::proxy_event_handler::process_proxy_request;
use crate::network::cmd_event_handler::process_cmd_request;

pub enum RunMode {
    Client,
    Server,
}

pub fn start(port_option: std::option::Option<String>, bootstrap_addr_option: std::option::Option<String>, run_mode: RunMode) -> Node {
    let (node_request_sender, node_request_receiver) = mpsc::unbounded_channel();
    let id_keys = Account::get_default_account_keypair("./db");
    let peer_id = id_keys.public().into_peer_id();
    info!("local peer id is {:?}", peer_id);

    let noise_keys = Keypair::<X25519Spec>::new().into_authentic(&id_keys).unwrap();
    let trans = TokioTcpConfig::new()
        .nodelay(true)
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::default())
        .timeout(std::time::Duration::from_millis(150))
        .boxed();


    let proxy_protocols = iter::once((ProxyProtocol(), ProtocolSupport::Full));
    let proxy_cfg = RequestResponseConfig::default();
    let proxy_behaviour = RequestResponse::new(ProxyCodec(), proxy_protocols, proxy_cfg);

    let cmd_protocols = iter::once((CmdProtocol(), ProtocolSupport::Full));
    let cmd_cfg = RequestResponseConfig::default();
    let cmd_behaviour = RequestResponse::new(CmdCodec(), cmd_protocols, cmd_cfg);

    let ack_protocols = iter::once((AckProtocol(), ProtocolSupport::Full));
    let ack_cfg = RequestResponseConfig::default();
    let ack_behaviour = RequestResponse::new(AckCodec(), ack_protocols, ack_cfg);

    let identify_behaviour = identify::Identify::new(identify::IdentifyConfig::new(String::from("/ipfs/id/1.0.0"), id_keys.public()).with_initial_delay(std::time::Duration::from_millis(500)).with_interval(std::time::Duration::from_secs(5 * 60)));

    let relay_behaviour = relay_behaviour::Relay {
        out_events: std::collections::VecDeque::new(),
        addresses: std::collections::HashMap::new(),
        dive_events: std::collections::VecDeque::new(),
    };

    let event_bus = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));

    let relay_in_streams = std::sync::Arc::new(std::sync::RwLock::new(std::collections::VecDeque::new()));
    let relay_out_streams = std::sync::Arc::new(std::sync::RwLock::new(std::collections::VecDeque::new()));

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

    let (proxy_request_sender, proxy_request_receiver) = mpsc::unbounded_channel();
    let (cmd_request_sender, cmd_request_receiver) = mpsc::unbounded_channel();
    let node = Node {
        node_request_sender: node_request_sender.clone(),
        event_bus: event_bus.clone(),
        keypair: id_keys.clone(),
        proxy_id: None,
        relay_in_streams: relay_in_streams.clone(),
        relay_out_streams: relay_out_streams.clone(),
        circuit_task: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        client_peer_map: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        client_wn_map: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        session_map: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        circuit_map: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        probe_map: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
    };

    match run_mode {
        RunMode::Client => {
            let whitenoise_behaviour = WhitenoiseBehaviour {
                proxy_behaviour: proxy_behaviour,
                cmd_behaviour: cmd_behaviour,
                ack_behaviour: ack_behaviour,
                event_bus: event_bus,
                relay_in_streams: relay_in_streams,
                relay_out_streams: relay_out_streams,
                relay_behaviour: relay_behaviour,
                proxy_request_channel: proxy_request_sender,
                cmd_request_channel: cmd_request_sender,
            };
            let whitenoise_client_behaviour = WhitenoiseClientBehaviour {
                whitenoise_behaviour: whitenoise_behaviour,
                identify_behaviour: identify_behaviour,
            };
            let mut swarm1 = SwarmBuilder::new(trans, whitenoise_client_behaviour, peer_id)
                .executor(Box::new(|fut| {
                    tokio::spawn(fut);
                }))
                .build();
            match port_option {
                None => {}
                Some(port) => {
                    let mut addr = String::from("/ip4/0.0.0.0/tcp/");
                    addr.push_str(port.as_str());
                    Swarm::listen_on(&mut swarm1, addr.parse().unwrap()).unwrap();
                }
            }
            tokio::spawn(whitenoise_behaviour::whitenoise_client_event_loop(swarm1, node_request_receiver));
        }
        RunMode::Server => {
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

            let whitenoise_behaviour = WhitenoiseBehaviour {
                proxy_behaviour: proxy_behaviour,
                cmd_behaviour: cmd_behaviour,
                ack_behaviour: ack_behaviour,
                event_bus: event_bus,
                relay_in_streams: relay_in_streams,
                relay_out_streams: relay_out_streams,
                relay_behaviour: relay_behaviour,
                proxy_request_channel: proxy_request_sender,
                cmd_request_channel: cmd_request_sender,
            };
            let (publish_request_sender, publish_request_receiver) = mpsc::unbounded_channel();
            let whitenoise_server_behaviour = WhitenoiseServerBehaviour {
                whitenoise_behaviour: whitenoise_behaviour,
                identify_behaviour: identify_behaviour,
                gossip_sub: gossipsub,
                publish_channel: publish_request_sender,
                kad_dht: kad_behaviour,
            };
            tokio::spawn(crate::network::publish_event_handler::process_publish_request(publish_request_receiver, node.clone()));
            let mut swarm1 = SwarmBuilder::new(trans, whitenoise_server_behaviour, peer_id)
                .executor(Box::new(|fut| {
                    tokio::spawn(fut);
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
            tokio::spawn(whitenoise_behaviour::whitenoise_server_event_loop(swarm1, node_request_receiver));
        }
    }

    tokio::spawn(process_proxy_request(proxy_request_receiver, node.clone()));
    tokio::spawn(process_cmd_request(cmd_request_receiver, node.clone()));
    return node;
}