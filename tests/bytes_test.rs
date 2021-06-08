use multihash::{Code, MultihashDigest};
use bytes::BufMut;
#[test]
fn test_bytes(){
    let bytes1 = vec![1,2,3];
    let mut bytes2 = Vec::new();
    bytes2.push(1);
    bytes2.push(2);
    bytes2.push(3);
    assert_eq!(bytes1,bytes2);
    bytes2.push(4);
    assert_ne!(bytes1,bytes2);

    let hash_algorithm = Code::Sha2_256;
    let hash = hash_algorithm.digest("123xyznogwgw".as_bytes());

    let hash_bytes = hash.to_bytes()[2..].to_vec();
    assert_eq!(hash_bytes.get(0),Some(&(52 as u8)));
    assert_eq!(hash_bytes.get(1),Some(&(63 as u8)));
    assert_eq!(hash_bytes.len(),32 as usize);
    assert_eq!(hash_bytes.get(31),Some(&(227 as u8)));
}