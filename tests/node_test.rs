use whitenoisers::{account, network::{self}};


use whitenoisers::sdk::host::start_server;

#[async_std::test]
async fn node_test() {
    //start boot strap
    let port = Some(String::from("3331"));
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let key_type = String::from("ed25519");

    let boot = start_server(None, port, key_type.clone(), Some(keypair)).await;
    let boot_id = boot.get_id();
    let mut bootstrap_addr = String::from("/ip4/127.0.0.1/tcp/3331/p2p/");
    bootstrap_addr.push_str(boot_id.as_str());

    //start nodes
    let cnt = 3;
    let mut node_vec = Vec::new();
    let mut port_int = 6661;
    for _i in 0..cnt {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let node = start_server(Some(bootstrap_addr.clone()), Some(port_int.to_string()), key_type.clone(), Some(keypair)).await;
        port_int += 1;
        node_vec.push(node);
    }
}

#[test]
fn whitenoiseid_hash_test() {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    if let libp2p::identity::Keypair::Ed25519(k) = &keypair {
        println!("pk encode {:?}", k.public().encode())
    }
    let id = account::account_service::Account::from_keypair_to_whitenoise_id(&keypair);
    println!("id {:?}", id.as_str());
    let hash = network::utils::from_whitenoise_to_hash(id.as_str());
    println!("hash {:?}", hash);
}

