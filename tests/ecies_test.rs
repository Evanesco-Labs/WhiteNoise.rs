use whitenoisers::{sdk::{host, host::RunMode}, account, network::{self, node::Node}};
use log::{info, debug, warn, error};
use env_logger::Builder;
use account::account::Account;
use whitenoisers::sdk::host::start_server;
use whitenoisers::sdk::client::{WhiteNoiseClient, Client};
use libp2p::PeerId;
use whitenoisers::gossip_proto;
use prost::Message;
use futures::StreamExt;
use libp2p::core::identity;


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