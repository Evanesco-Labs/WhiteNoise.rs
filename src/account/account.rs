use libp2p::{
    identity,
    identity::Keypair,
};
use rusty_leveldb::{DB};
use log::{info, debug};
use eth_ecies::{Secret, Public, crypto::ecies, crypto::Error};
use sha2::{Digest, Sha512};

pub struct Account {}

impl Account {
    pub fn get_default_account_keypair(path: &str, key_type: super::key_types::KeyType) -> Keypair {
        let mut opt = rusty_leveldb::Options::default();
        let mut db = DB::open(path, opt).unwrap();
        let mut key = String::from("default");
        let key_type_index = key_type.to_i32().to_string();
        key.push_str(key_type_index.as_str());
        let default_account_bytes_option = db.get(key.as_bytes());
        match default_account_bytes_option {
            None => {
                let keypair = match key_type {
                    super::key_types::KeyType::ED25519 => identity::Keypair::generate_ed25519(),
                    super::key_types::KeyType::SECP256K1 => identity::Keypair::generate_secp256k1()
                };

                match keypair.clone() {
                    identity::Keypair::Ed25519(k) => {
                        db.put(key.as_bytes(), &(k.encode()));
                    }
                    identity::Keypair::Secp256k1(k) => {
                        db.put(key.as_bytes(), &k.secret().to_bytes());
                    }
                    _ => {
                        panic!("we only support ed25519 or secp256k1");
                    }
                }
                keypair
            }
            Some(mut default_account_bytes) => {
                info!("[WhiteNoise] use existing keypair");
                match key_type {
                    super::key_types::KeyType::ED25519 => identity::Keypair::Ed25519(identity::ed25519::Keypair::decode(&mut default_account_bytes).unwrap()),
                    super::key_types::KeyType::SECP256K1 => identity::Keypair::Secp256k1(identity::secp256k1::Keypair::from(identity::secp256k1::SecretKey::from_bytes(default_account_bytes).unwrap()))
                }
            }
        }
    }

    pub fn from_keypair_to_whitenoise_id(keypair: &identity::Keypair) -> String {
        match keypair {
            identity::Keypair::Ed25519(k) => {
                let mut encoded = String::from("0");
                encoded.push_str(bs58::encode(k.public().encode()).into_string().as_str());
                return encoded;
            }
            identity::Keypair::Secp256k1(k) => {
                let mut encoded = String::from("1");
                encoded.push_str(bs58::encode(k.public().encode()).into_string().as_str());
                return encoded;
            }
            _ => {
                panic!("keypair format not ed15519 or secp256k1");
            }
        };
    }

    pub fn from_keypair_to_secretkey_bytes(keypair: &identity::Keypair) -> Vec<u8> {
        let secret_key = match keypair {
            identity::Keypair::Secp256k1(k) => {
                k.secret().to_bytes().to_vec()
            }
            _ => {
                panic!("keypair format not ed25519");
            }
        };
        return secret_key;
    }

    pub fn ecies_encrypt(pub_bytes: &[u8], plain: &[u8]) -> Vec<u8> {
        let secp_pub_key = identity::secp256k1::PublicKey::decode(pub_bytes).unwrap();
        let serialized = secp_pub_key.encode_uncompressed();

        let mut public_key = Public::default();
        public_key.copy_from_slice(&serialized[1..65]);
        return ecies::encrypt(&public_key, b"", plain).unwrap();
    }

    pub fn ecies_ed25519_encrypt(pub_bytes: &[u8], plain: &[u8]) -> Vec<u8> {
        debug!("prepare to encrypt:{},data:{}", bs58::encode(pub_bytes).into_string(), bs58::encode(plain).into_string());
        let public = ecies_ed25519::PublicKey::from_bytes(pub_bytes).unwrap();
        let mut csprng = rand::thread_rng();
        let encrypted = ecies_ed25519::encrypt(&public, plain, &mut csprng).unwrap();
        debug!("encrypt result:{}", bs58::encode(&encrypted).into_string());
        return encrypted;
    }

    pub fn ecies_decrypt(keypair: &identity::Keypair, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        let secret_key = Self::from_keypair_to_secretkey_bytes(keypair);
        let secret = Secret::from_slice(&secret_key).unwrap();
        return ecies::decrypt(&secret, b"", encrypted);
    }

    pub fn ecies_ed25519_decrypt(keypair: &identity::Keypair, encrypted: &[u8]) -> Result<Vec<u8>, ecies_ed25519::Error> {
        let ed25519_keypair =
            match keypair {
                identity::Keypair::Ed25519(k) => {
                    k
                }
                _ => {
                    panic!("wrong format");
                }
            };
        debug!("prepare to decrypt:{},data:{}", bs58::encode(ed25519_keypair.encode()).into_string(), bs58::encode(encrypted).into_string());
        let mut h: Sha512 = Sha512::default();
        let mut hash: [u8; 64] = [0u8; 64];
        let mut lower: [u8; 32] = [0u8; 32];
        h.update(ed25519_keypair.secret().as_ref());
        hash.copy_from_slice(h.finalize().as_slice());
        lower.copy_from_slice(&hash[00..32]);

        lower[0] &= 248;
        lower[31] &= 63;
        lower[31] |= 64;
        lower[0] &= 248;
        lower[31] &= 127;
        lower[31] |= 64;

        let secret = ecies_ed25519::SecretKey::from_bytes(&lower).unwrap();
        return ecies_ed25519::decrypt(&secret, &encrypted);
    }
}