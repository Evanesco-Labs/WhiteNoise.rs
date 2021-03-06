use multihash::{Code, MultihashDigest};
use crate::{request_proto, relay_proto, payload_proto};
use prost::Message;
use bytes::BufMut;
use crate::network::protocols::relay_behaviour::WrappedStream;
use libp2p::swarm::NegotiatedSubstream;
use crate::network::session::{SessionRole, Session};
use libp2p::core::{identity, upgrade};
use libp2p::noise::KeypairIdentity;
use std::io;
use libp2p::futures::TryFutureExt;
use snow::{TransportState, HandshakeState};
use libp2p::core::upgrade::ReadOneError;
use log::{info, debug};
use rand::{RngCore, Rng};
use crate::network::const_vb::{CONFUSION_MAX_PROPORTION, JITTER_MAX_MICROS, CONFUSION_PAD_LEN};
use std::time::Duration;
use std::convert::TryInto;

pub fn from_request_get_id(request: &request_proto::Request) -> String {
    let mut buf = Vec::new();
    request.encode(&mut buf).unwrap();
    from_bytes_get_id(&buf)
}

pub fn from_bytes_get_id(buf: &[u8]) -> String {
    let hash_algorithm = Code::Sha2_256;
    let hash = hash_algorithm.digest(buf);
    let hash_bytes = hash.to_bytes()[2..].to_vec();
    bs58::encode(hash_bytes).into_string()
}

pub fn from_whitenoise_to_hash(whitenoise_id: &str) -> String {
    let (_index, pub_bytes) = whitenoise_id.split_at(1);
    let whitenoise_bytes = bs58::decode(pub_bytes).into_vec().unwrap();
    let hash_algorithm = Code::Sha2_256;
    let hash = hash_algorithm.digest(&whitenoise_bytes);
    let zz = hash.to_bytes()[2..].to_vec();
    bs58::encode(zz.as_slice()).into_string()
}

pub async fn write_relay_arc(mut stream: WrappedStream, mut relay: relay_proto::Relay) -> String {
    relay = confusion_relay(relay);
    let mut relay_data = Vec::new();
    relay.encode(&mut relay_data).unwrap();
    let key = from_bytes_get_id(&relay_data);
    relay.id = key.clone();
    let mut relay_data = Vec::new();
    relay.encode(&mut relay_data).unwrap();
    jitter().await;
    stream.write(relay_data).await;
    key
}

pub async fn write_payload_arc(stream: WrappedStream, buf: &[u8], len: usize, session_id: &str) {
    let mut new_buf = Vec::with_capacity(2 + len);
    new_buf.put_u16(len as u16);
    new_buf.chunk_mut().copy_from_slice(buf);
    unsafe {
        new_buf.advance_mut(len);
    }
    let relay_msg = relay_proto::RelayMsg {
        session_id: String::from(session_id),
        data: new_buf,
    };
    let mut relay_msg_data = Vec::new();
    relay_msg.encode(&mut relay_msg_data).unwrap();
    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::Data as i32,
        data: relay_msg_data,
        confusion: Vec::new(),
    };
    write_relay_arc(stream.clone(), relay).await;
}

pub fn handle_remote_handshake_payload(payload: &[u8], remote_static: &[u8]) -> bool {
    let noise_shakehand_payload = payload_proto::NoiseHandshakePayload::decode(payload).unwrap();

    let id_key = noise_shakehand_payload.identity_key;
    let id_sig = noise_shakehand_payload.identity_sig;

    let id_pub_key = identity::PublicKey::from_protobuf_encoding(&id_key).unwrap();
    id_pub_key.verify(&[b"noise-libp2p-static-key:", remote_static].concat(), &id_sig)
}

pub async fn generate_handshake_payload(identity: KeypairIdentity) -> Vec<u8> {
    let pb = payload_proto::NoiseHandshakePayload {
        identity_key: identity.public.clone().into_protobuf_encoding(),
        identity_sig: identity.signature.unwrap(),
        ..Default::default()
    };
    info!("[WhiteNoise] public key:{}", bs58::encode(pb.identity_key.as_slice()).into_string());
    info!("[WhiteNoise] signature:{}", bs58::encode(pb.identity_sig.as_slice()).into_string());
    let mut msg = Vec::with_capacity(pb.encoded_len());
    pb.encode(&mut msg).unwrap();
    msg
}

pub async fn write_relay(stream: &mut NegotiatedSubstream, mut relay: relay_proto::Relay) -> String {
    let mut relay_data = Vec::new();
    relay = confusion_relay(relay);
    relay.encode(&mut relay_data).unwrap();
    let key = from_bytes_get_id(&relay_data);
    relay.id = key.clone();
    let mut relay_data = Vec::new();
    relay.encode(&mut relay_data).unwrap();

    jitter().await;
    upgrade::write_with_len_prefix(stream, &relay_data).map_err(|e| {
        info!("[WhiteNoise] write relay error:{:?}", e);
        io::Error::new(io::ErrorKind::InvalidData, e)
    }).await.unwrap();
    key
}

pub async fn write_disconnect_arc(stream: WrappedStream, session_id: String) {
    let disconnect_relay = relay_proto::Disconnect {
        session_id,
        err_code: 0,
    };
    let mut data = Vec::new();
    disconnect_relay.encode(&mut data).unwrap();
    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::Disconnect as i32,
        data,
        confusion: Vec::new(),
    };
    write_relay_arc(stream.clone(), relay).await;
}

pub async fn write_relay_wake_arc(stream: WrappedStream) {
    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::Wake as i32,
        data: Vec::new(),
        confusion: Vec::new(),
    };
    write_relay_arc(stream.clone(), relay).await;
}

pub async fn write_relay_wake(stream: &mut NegotiatedSubstream) {
    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::Wake as i32,
        data: Vec::new(),
        confusion: Vec::new(),
    };
    write_relay(stream, relay).await;
}

pub async fn write_set_session_with_role_arc(stream: WrappedStream, session_id: String, session_role: i32) -> String {
    let cmd = relay_proto::SetSessionIdMsg {
        session_id,
        role: session_role,
    };
    let mut data = Vec::new();
    cmd.encode(&mut data).unwrap();

    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::SetSessionId as i32,
        data,
        confusion: Vec::new(),
    };
    return write_relay_arc(stream.clone(), relay).await;
}

pub async fn write_set_session_arc(stream: WrappedStream, session_id: String) -> String {
    let cmd = relay_proto::SetSessionIdMsg {
        session_id,
        role: SessionRole::EntryRole as i32,
    };
    let mut data = Vec::new();
    cmd.encode(&mut data).unwrap();

    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::SetSessionId as i32,
        data,
        confusion: Vec::new(),
    };
    return write_relay_arc(stream.clone(), relay).await;
}

pub async fn write_set_session(stream: &mut NegotiatedSubstream, session_id: String) -> String {
    let cmd = relay_proto::SetSessionIdMsg {
        session_id,
        role: SessionRole::EntryRole as i32,
    };
    let mut data = Vec::new();
    cmd.encode(&mut data).unwrap();

    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::SetSessionId as i32,
        data,
        confusion: Vec::new(),
    };
    return write_relay(stream, relay).await;
}

pub async fn write_payload(stream: &mut NegotiatedSubstream, buf: &[u8], len: usize, session_id: &str) {
    let mut new_buf = Vec::with_capacity(2 + len);

    new_buf.put_u16(len as u16);
    buf.iter().for_each(|x| new_buf.put_u8(*x));

    let relay_msg = relay_proto::RelayMsg {
        session_id: String::from(session_id),
        data: new_buf,
    };
    let mut relay_msg_data = Vec::new();
    relay_msg.encode(&mut relay_msg_data).unwrap();
    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::Data as i32,
        data: relay_msg_data,
        confusion: Vec::new(),
    };
    write_relay(stream, relay).await;
}

pub async fn write_encrypted_payload_arc(stream: WrappedStream, payload: &[u8], buf: &mut [u8], session_id: &str, noise: &mut TransportState) {
    let len = noise.write_message(payload, buf).unwrap();
    let buf_tmp = &buf[..len];
    write_payload_arc(stream.clone(), buf_tmp, len, session_id).await;
}

pub async fn write_encrypted_payload(stream: &mut NegotiatedSubstream, payload: &[u8], buf: &mut [u8], session_id: &str, noise: &mut TransportState) {
    let len = noise.write_message(payload, buf).unwrap();
    let buf_tmp = &buf[..len];
    write_payload(stream, buf_tmp, len, session_id).await;
}

pub async fn write_handshake_payload_arc(stream: WrappedStream, buf: &[u8], len: usize, session_id: &str) {
    write_payload_arc(stream.clone(), buf, len, session_id).await;
}

pub async fn write_handshake_payload(stream: &mut NegotiatedSubstream, buf: &[u8], len: usize, session_id: &str) {
    write_payload(stream, buf, len, session_id).await;
}

pub async fn read_payload_arc(stream: WrappedStream) -> Vec<u8> {
    let relay = loop {
        let relay_inner_rest = read_from_negotiated_arc(stream.clone()).await;
        let relay_inner = relay_inner_rest.unwrap();
        if relay_inner.r#type == (relay_proto::Relaytype::Data as i32) {
            break relay_inner;
        }
    };

    let relay_msg = relay_proto::RelayMsg::decode(relay.data.as_slice()).unwrap();
    debug!("[WhiteNoise] read decrypt relay msg data len:{}", relay_msg.data.len());
    let buf_len = relay_msg.data[0] as usize * 256 + relay_msg.data[1] as usize;
    info!("[WhiteNoise] relay data len:{},real buf len:{}", relay_msg.data.len(), buf_len);
    relay_msg.data[2..(2 + buf_len)].to_vec()
}

pub async fn read_payload(stream: &mut NegotiatedSubstream) -> Vec<u8> {
    let relay = loop {
        let relay_inner_rest = read_from_negotiated(stream).await;
        let relay_inner = relay_inner_rest.unwrap();
        if relay_inner.r#type == (relay_proto::Relaytype::Data as i32) {
            break relay_inner;
        }
    };

    let relay_msg = relay_proto::RelayMsg::decode(relay.data.as_slice()).unwrap();
    debug!("read decrypt relay msg data len:{}", relay_msg.data.len());
    let buf_len = relay_msg.data[0] as usize * 256 + relay_msg.data[1] as usize;
    info!("[WhiteNoise] relay data len:{},real buf len:{}", relay_msg.data.len(), buf_len);
    relay_msg.data[2..(2 + buf_len)].to_vec()
}

pub async fn read_and_decrypt_payload_arc(stream: WrappedStream, noise: &mut TransportState, buf: &mut [u8]) -> usize {
    let payload = read_payload_arc(stream.clone()).await;
    noise.read_message(&payload, buf).unwrap()
}

pub async fn read_and_decrypt_payload(stream: &mut NegotiatedSubstream, noise: &mut TransportState, buf: &mut [u8]) -> usize {
    let payload = read_payload(stream).await;
    noise.read_message(&payload, buf).unwrap()
}

pub async fn read_handshake_payload_arc(stream: WrappedStream, noise: &mut HandshakeState, buf: &mut [u8]) -> usize {
    let payload = read_payload_arc(stream.clone()).await;
    noise.read_message(&payload, buf).unwrap()
}

pub async fn read_handshake_payload(stream: &mut NegotiatedSubstream, noise: &mut HandshakeState, buf: &mut [u8]) -> usize {
    let payload = read_payload(stream).await;
    noise.read_message(&payload, buf).unwrap()
}


pub async fn read_from_negotiated_arc(mut stream: WrappedStream) -> Result<relay_proto::Relay, ReadOneError> {
    let msg = stream.read().await?;
    let relay = relay_proto::Relay::decode(msg.as_slice()).unwrap();
    Ok(relay)
}


pub async fn read_from_negotiated(stream: &mut NegotiatedSubstream) -> Result<relay_proto::Relay, io::Error> {
    let msg = upgrade::read_one(stream, 4096)
        .map_err(|e| {
            info!("[WhiteNoise] receive relay error:{:?}", e);
            io::Error::new(io::ErrorKind::InvalidData, e)
        }).await?;
    let relay = relay_proto::Relay::decode(msg.as_slice()).unwrap();
    Ok(relay)
}

pub fn new_relay_circuit_success(session_id: &str) -> relay_proto::Relay {
    let circuit_success = relay_proto::CircuitSuccess { session_id: session_id.to_string() };
    let mut data = Vec::new();
    circuit_success.encode(&mut data).unwrap();
    relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::Success as i32,
        data,
        confusion: Vec::new(),
    }
}

pub fn new_relay_probe(session_id: &str) -> relay_proto::Relay {
    let hash_algorithm = Code::Sha2_256;
    let hash = hash_algorithm.digest(session_id.as_bytes());
    let hash_bytes = hash.to_bytes()[2..].to_vec();
    let probe_signal = relay_proto::ProbeSignal { session_id: String::from(session_id), data: hash_bytes };
    let mut probe_signal_data = Vec::new();
    probe_signal.encode(&mut probe_signal_data).unwrap();
    relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::Probe as i32,
        data: probe_signal_data,
        confusion: Vec::new(),
    }
}

pub async fn forward_relay(session: &Session, cur_stream_id: &str, relay: relay_proto::Relay) {
    if session.pair_stream.early_stream.clone().unwrap().stream_id == cur_stream_id {
        write_relay_arc(session.pair_stream.later_stream.clone().unwrap(), relay).await;
    } else if session.pair_stream.later_stream.clone().unwrap().stream_id == cur_stream_id {
        write_relay_arc(session.pair_stream.early_stream.clone().unwrap(), relay).await;
    }
}

pub async fn send_relay_twoway(session: &Session, relay: relay_proto::Relay) {
    if session.pair_stream.early_stream.is_some() {
        async_std::task::spawn(crate::network::utils::write_relay_arc(session.pair_stream.early_stream.clone().unwrap(), relay.clone()));
    }
    if session.pair_stream.later_stream.is_some() {
        async_std::task::spawn(crate::network::utils::write_relay_arc(session.pair_stream.later_stream.clone().unwrap(), relay));
    }
}

fn confusion_relay(relay: relay_proto::Relay) -> relay_proto::Relay {
    let mut rng = rand::thread_rng();
    let confusion_len = rng.gen_range(0, relay.data.as_slice().len() * CONFUSION_MAX_PROPORTION / 100 + CONFUSION_PAD_LEN);
    let mut confusion = vec![0u8; confusion_len];
    rng.fill_bytes(confusion.as_mut_slice());
    relay_proto::Relay {
        id: relay.id,
        r#type: relay.r#type,
        data: relay.data,
        confusion,
    }
}

fn random_jitter_duration() -> Duration {
    let mut rng = rand::thread_rng();
    Duration::from_micros(rng.gen_range(0, JITTER_MAX_MICROS).try_into().unwrap())
}

async fn jitter() {
    let dur = random_jitter_duration();
    async_std::task::sleep(dur).await;
}

#[test]
fn test_confusion() {
    let data_len = 1024;
    let data = vec![0u8; data_len];
    let max_confusion_len = data_len * CONFUSION_MAX_PROPORTION / 100;
    let confusion = vec![0u8; 5];
    let relay = relay_proto::Relay {
        id: String::from(""),
        r#type: relay_proto::Relaytype::Data as i32,
        data,
        confusion,
    };
    let mut relay_encoded = Vec::new();
    relay.encode(&mut relay_encoded).unwrap();
    let next_relay = confusion_relay(relay);
    let mut next_relay_encoded = Vec::new();
    next_relay.encode(&mut next_relay_encoded).unwrap();
    assert!(next_relay.confusion.as_slice().len() <= max_confusion_len)
}
