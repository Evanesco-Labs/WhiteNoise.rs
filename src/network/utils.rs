use multihash::{Code, MultihashDigest};
use crate::{request_proto,relay_proto};
use prost::Message;
use bytes::BufMut;
use crate::network::protocols::relay_behaviour::WrappedStream;

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
    let whitenoise_bytes = bs58::decode(whitenoise_id).into_vec().unwrap();
    let hash_algorithm = Code::Sha2_256;
    let hash = hash_algorithm.digest(&whitenoise_bytes);
    let zz = hash.to_bytes()[2..].to_vec();
    bs58::encode(zz.as_slice()).into_string()
}

pub async fn write_relay_arc(mut stream: WrappedStream,mut relay: relay_proto::Relay) -> String{
    let mut relay_data = Vec::new();
    relay.encode(&mut relay_data);
    let key = from_bytes_get_id(&relay_data);
    relay.id = key.clone();
    let mut relay_data = Vec::new();
    relay.encode(&mut relay_data);
    stream.write(relay_data).await;
    return key;
}

pub async fn write_payload_arc(stream: WrappedStream,buf: &[u8],len: usize,session_id: &str){
    let mut new_buf = Vec::with_capacity(2+len);
    new_buf.put_u16(len as u16);
    new_buf.chunk_mut().copy_from_slice(buf);
    unsafe{
        new_buf.advance_mut(len);
    }
    let relay_msg = relay_proto::RelayMsg{
        session_id: String::from(session_id),
        data: new_buf
    };
    let mut relay_msg_data = Vec::new();
    relay_msg.encode(&mut relay_msg_data);
    let mut relay = relay_proto::Relay{
        id: String::from(""),
        r#type: relay_proto::Relaytype::Data as i32,
        data: relay_msg_data
    };
    write_relay_arc(stream.clone(), relay).await;
}