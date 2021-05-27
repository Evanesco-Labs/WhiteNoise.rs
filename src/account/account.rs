use libp2p::{
    identity,
    identity::Keypair,
};
use rusty_leveldb::{DB};
use log::info;

pub struct Account {}

impl Account {
    pub fn get_default_account_keypair(path: &str) -> Keypair {
        let mut opt = rusty_leveldb::Options::default();
        let mut db = DB::open(path, opt).unwrap();
        let default_account_bytes_option = db.get(b"default");
        match default_account_bytes_option {
            None => {
                let keypair = identity::Keypair::generate_secp256k1();
                match keypair.clone() {
                    identity::Keypair::Secp256k1(k) => {
                        db.put(b"default", &(k.secret().to_bytes()));
                    }
                    _ => {}
                }
                keypair
            }
            Some(default_account_bytes) => {
                info!("keypair exists");
                let secretkey = identity::secp256k1::SecretKey::from_bytes(default_account_bytes).unwrap();
                identity::Keypair::Secp256k1(identity::secp256k1::Keypair::from(secretkey))
            }
        }
    }
    pub fn from_keypair_to_whitenoise_id(keypair: &identity::Keypair) -> String {
        let public_key_secp256k1 = match keypair.public() {
            identity::PublicKey::Secp256k1(k) => k,
            _ => {
                panic!("keypair format not secp256k1");
            }
        };
        bs58::encode(public_key_secp256k1.encode()).into_string()
    }
    pub fn from_keypair_to_secretkey_bytes(keypair: &identity::Keypair) -> Vec<u8> {
        let secret_key = match keypair {
            identity::Keypair::Secp256k1(k) => {
                k.secret().to_bytes().to_vec()
            }
            _ => {
                panic!("keypair format not secp256k1");
            }
        };
        return secret_key;
    }
}