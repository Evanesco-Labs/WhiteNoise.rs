use std::error::Error;
use env_logger::{Builder};
use clap::{Arg, App, SubCommand};
use whitenoisers::sdk::client::{WhiteNoiseClient, Client};
use prost::Message;
use futures::{future::FutureExt};
use bytes::BufMut;
use log::{info};

pub const BUF_LEN: usize = 65536;

pub mod chat_proto {
    include!(concat!(env!("OUT_DIR"), "/chat_proto.rs"));
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = App::new("whitenoise-chat")
        .version("1.0")
        .author("EVA-Labs")
        .about("WhiteNoise client chatting example")
        .arg(Arg::with_name("log")
            .long("log")
            .help("log level error, warn, info or debug")
            .takes_value(true))
        .subcommand(SubCommand::with_name("chat")
            .arg(Arg::with_name("bootstrap")
                .short("b")
                .long("bootstrap")
                .help("MultiAddress of the WhiteNoise network bootstrap node.")
                .takes_value(true))
            .arg(Arg::with_name("id")
                .long("id")
                .help("WhiteNoiseID of another client to chat with.")
                .takes_value(true))
            .arg(Arg::with_name("nick")
                .long("nick")
                .help("Nick name for chatting.")
                .takes_value(true))
            .arg(Arg::with_name("ktype")
                .long("keytype")
                .help("Specify crypto suite of account's key. Support \'ed25519\' or \'secp256k1\', \'ed25519\' by default")
                .takes_value(true)))
        .get_matches();

    let log_level = args.value_of("log").unwrap_or("info");
    let env = env_logger::Env::new().filter_or("MY_LOG", log_level);
    let mut builder = Builder::new();
    builder.parse_env(env);
    builder.format_timestamp_millis();
    builder.init();

    if let Some(x) = args.subcommand_matches("chat") {
        let bootstrap_addr_str = x.value_of("bootstrap").unwrap();
        info!("bootstrap_addr:{}", bootstrap_addr_str);
        let nick_name = String::from(x.value_of("nick").unwrap_or("Alice"));
        info!("nick name: {}", nick_name);
        let remote_whitenoise_id_option = x.value_of("id");
        let remote_whitenoise_id_str_option = remote_whitenoise_id_option.map(String::from);
        let key_type = String::from(x.value_of("ktype").unwrap_or("ed25519"));
        start_client(String::from(bootstrap_addr_str), nick_name, remote_whitenoise_id_str_option.clone(), key_type).await;
    }

    Ok(())
}


pub async fn start_client(bootstrap_addr_str: String, nick_name: String, remote_whitenoise_id_option: Option<String>, key_type: String) {
    let parts: Vec<&str> = bootstrap_addr_str.split('/').collect();
    let bootstrap_peer_id_str = parts.last().unwrap();
    info!("bootstrap peer id:{}", bootstrap_peer_id_str);

    let mut whitenoise_client = WhiteNoiseClient::init(bootstrap_addr_str, whitenoisers::account::key_types::KeyType::from_text_str(key_type.as_str()), None);

    let peer_list = whitenoise_client.get_main_net_peers(10i32).await;
    let mut index = rand::random::<usize>();
    index %= peer_list.len();
    let proxy_remote_id = peer_list.get(index).unwrap();
    info!("choose id:{:?} to register", proxy_remote_id);


    whitenoise_client.register(*proxy_remote_id).await;
    //dial
    if remote_whitenoise_id_option.is_some() {
        caller(remote_whitenoise_id_option.unwrap(), whitenoise_client, nick_name).await;
    } else {
        answer(whitenoise_client, nick_name).await;
    }
}

pub async fn answer(mut client: WhiteNoiseClient, nick_name: String) {
    let session_id = client.notify_next_session().await.unwrap();
    let mut circuit_conn = client.get_circuit(session_id.as_str()).unwrap();

    let stdin = async_std::io::stdin();
    let mut buf = [0u8; BUF_LEN];
    loop {
        let mut line = String::new();
        futures::select! {
             _ = stdin.read_line(&mut line).fuse() =>{
                let chat_message = chat_proto::ChatMessage{peer_id: nick_name.clone(),data: line.as_bytes().to_vec()};
                let mut chat_message_encode = Vec::new();
                chat_message.encode(&mut chat_message_encode).unwrap();
                let mut payload = Vec::with_capacity(4+chat_message_encode.len());
                payload.put_u32(chat_message_encode.len() as u32);
                payload.chunk_mut().copy_from_slice(chat_message_encode.as_slice());
                unsafe{
                    payload.advance_mut(chat_message_encode.len());
                }
                client.send_message(session_id.as_str(), &payload).await;
            }
            data = circuit_conn.read(&mut buf).fuse() =>{
            let chat_message = chat_proto::ChatMessage::decode(data.as_slice()).unwrap();
                println!("message from {}: {}",chat_message.peer_id,String::from_utf8_lossy(&chat_message.data));
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
            async_std::task::sleep(std::time::Duration::from_millis(20)).await;
        };

    let stdin = async_std::io::stdin();
    let mut buf = [0u8; BUF_LEN];
    loop {
        let mut line = String::new();
        futures::select! {
            _ = stdin.read_line(&mut line).fuse() =>{
                let chat_message = chat_proto::ChatMessage{peer_id: nick_name.clone(),data: line.as_bytes().to_vec()};
                let mut chat_message_encode = Vec::new();
                chat_message.encode(&mut chat_message_encode).unwrap();
                let mut payload = Vec::with_capacity(4+chat_message_encode.len());
                payload.put_u32(chat_message_encode.len() as u32);
                payload.chunk_mut().copy_from_slice(chat_message_encode.as_slice());
                unsafe{
                    payload.advance_mut(chat_message_encode.len());
                }
                client.send_message(session_id.as_str(), &payload).await;
            }
            data = circuit_conn.read(&mut buf).fuse() =>{
            let chat_message = chat_proto::ChatMessage::decode(data.as_slice()).unwrap();
                println!("message from {}: {}",chat_message.peer_id,String::from_utf8_lossy(&chat_message.data));
            }
        }
    }
}