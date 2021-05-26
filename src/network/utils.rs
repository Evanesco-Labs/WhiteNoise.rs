use multihash::{Code, MultihashDigest};
use crate::{request_proto};
use prost::Message;

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
