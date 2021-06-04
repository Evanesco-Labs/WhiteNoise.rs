use bytes::BufMut;
use libp2p::{
    Multiaddr, PeerId,
    identity,
    noise,
};

pub mod request_proto {
    include!(concat!(env!("OUT_DIR"), "/request_proto.rs"));
}

pub mod command_proto {
    include!(concat!(env!("OUT_DIR"), "/command_proto.rs"));
}

pub mod gossip_proto {
    include!(concat!(env!("OUT_DIR"), "/gossip_proto.rs"));
}

pub mod relay_proto {
    include!(concat!(env!("OUT_DIR"), "/relay_proto.rs"));
}

pub mod payload_proto {
    include!(concat!(env!("OUT_DIR"), "/payload_proto.rs"));
}

pub mod chat_proto {
    include!(concat!(env!("OUT_DIR"), "/chat_proto.rs"));
}

pub mod network;
pub mod sdk;
pub mod account;
pub mod models;

use rand::{self};
use crate::network::connection::CircuitConn;

use std::error::Error;
use tokio::io::{self, AsyncBufReadExt};
use multihash::{Code, MultihashDigest};
use tokio::sync::mpsc;
use log::{info, debug};
use network::{node};
use sdk::chat_message::ChatMessage;
use clap::{Arg, App, SubCommand};

use crate::sdk::host::{self, RunMode};
use crate::network::session::{SessionRole};
use env_logger::{Builder, Env};
use crate::network::const_vb::BUF_LEN;
use crate::sdk::client::{WhiteNoiseClient, Client};

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<(), Box<dyn Error>> {
    let env = env_logger::Env::new().filter_or("MY_LOG", "info");
    let mut builder = Builder::new();
    builder.parse_env(env);
    builder.format_timestamp_millis();
    builder.init();

    let args = App::new("whitenoise")
        .version("1.0")
        .author("EVA-Labs")
        .about("whitenoise")
        .subcommand(SubCommand::with_name("start")
            .arg(Arg::with_name("port")
                .help("listen port.")
                .long("port")
                .short("p")
                .takes_value(true))
            .arg(Arg::with_name("bootstrap")
                .short("b")
                .long("bootstrap")
                .help("MultiAddress of the node to bootstrap from.")
                .takes_value(true)))
        .subcommand(SubCommand::with_name("chat")
            .arg(Arg::with_name("bootstrap")
                .short("b")
                .long("bootstrap")
                .help("MultiAddress of the node to bootstrap from.")
                .takes_value(true))
            .arg(Arg::with_name("node")
                .short("n")
                .long("node")
                .help("WhiteNoise ID of the client to connect.")
                .takes_value(true))
            .arg(Arg::with_name("nick")
                .long("nick")
                .help("Nick name for chatting.")
                .takes_value(true)))
        .get_matches();

    #[cfg(feature = "prod")]
    if let Some(x) = args.subcommand_matches("chat") {
        let bootstrap_addr_str = x.value_of("bootstrap").unwrap();
        info!("bootstrap_addr:{}", bootstrap_addr_str);
        let nick_name = String::from(x.value_of("nick").unwrap());
        info!("nick:{}", nick_name);
        let remote_whitenoise_id_option = x.value_of("node");
        let remote_whitenoise_id_str_option = match remote_whitenoise_id_option {
            None => None,
            Some(remote_whitenoise_id) => {
                Some(String::from(remote_whitenoise_id))
            }
        };
        start_client(String::from(bootstrap_addr_str), nick_name, remote_whitenoise_id_str_option.clone()).await;
    } else if let Some(x) = args.subcommand_matches("start") {
        let bootstrap_addr_str_option = x.value_of("bootstrap");
        info!("bootstrap_addr:{:?}", bootstrap_addr_str_option);
        let port_str_option = x.value_of("port");
        info!("port str:{:?}", port_str_option);
        let bootstrap_addr_option = match bootstrap_addr_str_option {
            None => None,
            Some(bootstrap_addr_str) => {
                Some(String::from(bootstrap_addr_str))
            }
        };
        let port_option = match port_str_option {
            None => None,
            Some(x) => Some(String::from(x))
        };
        start_server(bootstrap_addr_option, port_option).await;
    }
    #[cfg(feature = "local-test")]
    if 1 == 1 {
        let bootstrap_addr_str = "/ip4/127.0.0.1/tcp/3331/p2p/16Uiu2HAmSrxoDJCvBH4fNaCf4mE3x2XiByLcvooZWFGUz3VJ2mvt";
        let nick_name = String::from("bob");
        let remote_whitenoise_id_option = if 1 == 1 {
            None
        } else {
            Some(String::from("25ugrrpWrbNUfJgXkSMCSLCEG1trFaeiXx4U9GUo9PDiQ"))
        };

        let remote_whitenoise_id_str_option = match remote_whitenoise_id_option {
            None => None,
            Some(remote_whitenoise_id) => {
                Some(String::from(remote_whitenoise_id))
            }
        };
        start_client(String::from(bootstrap_addr_str), nick_name, remote_whitenoise_id_str_option.clone()).await;
    } else {
        let bootstrap_addr_str_option = Some("/ip4/127.0.0.1/tcp/3331/p2p/16Uiu2HAmBb5gidxpKq8J8UGHkzwoonjotTZc47SwMYsASsLyZbxd");
        let port_str_option = Some("3335");

        let bootstrap_addr_option = match bootstrap_addr_str_option {
            None => None,
            Some(bootstrap_addr_str) => {
                Some(String::from(bootstrap_addr_str))
            }
        };
        let port_option = match port_str_option {
            None => None,
            Some(x) => Some(String::from(x))
        };
        start_server(bootstrap_addr_option, port_option).await;
    }
    Ok(())
}

pub async fn start_server(bootstrap_addr_option: Option<String>, port_option: Option<String>) {
    info!("run_mode:{}", RunMode::Server as i32);
    let mut node = host::start(port_option, bootstrap_addr_option, RunMode::Server);
    loop {
        let wraped_stream = node.wait_for_relay_stream().await;
        debug!("{} have connected", wraped_stream.remote_peer_id.to_base58());
        tokio::spawn(crate::network::relay_event_handler::relay_event_handler(wraped_stream.clone(), node.clone(), None));
    }
}

pub async fn start_client(bootstrap_addr_str: String, nick_name: String, remote_whitenoise_id_option: Option<String>) {
    let parts: Vec<&str> = bootstrap_addr_str.split('/').collect();
    let bootstrap_peer_id_str = parts.get(parts.len() - 1).unwrap();
    info!("bootstrap peer id:{}", bootstrap_peer_id_str);

    let mut whitenoise_client = WhiteNoiseClient::init(bootstrap_addr_str);

    let peer_list = whitenoise_client.get_main_net_peers(10).await;
    let mut index = rand::random::<usize>();
    index = index % peer_list.len();
    let proxy_remote_id = peer_list.get(index).unwrap();
    info!("choose id:{:?} to register", proxy_remote_id);


    whitenoise_client.register(proxy_remote_id.clone()).await;
    //dial
    if remote_whitenoise_id_option.is_some() {
        caller(String::from(remote_whitenoise_id_option.unwrap()), whitenoise_client, nick_name).await;
    } else {
        answer(whitenoise_client, nick_name).await;
    }
}

pub async fn answer(mut client: WhiteNoiseClient, nick_name: String) {
    let session_id = client.notify_next_session().await.unwrap();
    let mut circuit_conn = client.get_circuit(session_id.as_str()).unwrap();

    let mut stdin = io::BufReader::new(io::stdin()).lines();
    let mut buf = [0u8; BUF_LEN];
    loop {
        tokio::select! {
            Ok(Some(line)) = stdin.next_line() =>{
                let chat_message = ChatMessage{peer_id:nick_name.clone(),data: line.as_bytes().to_vec()};
                let mut chat_message_str = serde_json::to_string(&chat_message).unwrap();
                let mut payload = Vec::with_capacity(4+chat_message_str.len());
                payload.put_u32(chat_message_str.len() as u32);
                payload.chunk_mut().copy_from_slice(chat_message_str.as_bytes());
                unsafe{
                    payload.advance_mut(chat_message_str.len());
                }
                client.send_message(session_id.as_str(), &payload).await;
            }
            data = circuit_conn.read(&mut buf) =>{
                let chat_message: ChatMessage = serde_json::from_slice(&data).unwrap();
                println!("receive {},message:{}",chat_message.peer_id,String::from_utf8_lossy(&chat_message.data));

            }
        }
    }
}

pub async fn caller(remote_whitenoise_id: String, mut client: WhiteNoiseClient, nick_name: String) {
    let session_id = client.dial(remote_whitenoise_id).await;
    let mut circuit_conn =
        loop {
            let cc = client.get_circuit(session_id.as_str());
            let cc = cc.unwrap();
            if cc.transport_state.is_some() {
                info!("shake hand finished");
                break cc.clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(20));
        };
    let mut stdin = io::BufReader::new(io::stdin()).lines();
    let mut buf = [0u8; BUF_LEN];
    loop {
        tokio::select! {
            Ok(Some(line)) = stdin.next_line() =>{
                let chat_message = ChatMessage{peer_id:nick_name.clone(),data: line.as_bytes().to_vec()};
                let mut chat_message_str = serde_json::to_string(&chat_message).unwrap();
                let chat_message_str_vec = unsafe{
                    chat_message_str.as_mut_vec()
                };
                let mut payload = Vec::with_capacity(4+chat_message_str_vec.len());
                payload.put_u32(chat_message_str_vec.len() as u32);
                payload.append(chat_message_str_vec);
                // circuit_conn.write(&payload, &mut buf).await;
                client.send_message(session_id.as_str(), &payload).await;
            }
            data = circuit_conn.read(&mut buf) =>{
                let chat_message: ChatMessage = serde_json::from_slice(&data).unwrap();
                println!("receive {},message:{}",chat_message.peer_id,String::from_utf8_lossy(&chat_message.data));
            }
        }
    }
}





