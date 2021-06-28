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


use std::error::Error;

use log::{info};

use clap::{Arg, App, SubCommand};

use env_logger::{Builder};

use whitenoisers::sdk::host::start_server;
use futures::channel::oneshot;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = App::new("whitenoise")
        .version("1.0")
        .author("EVA-Labs")
        .about("whitenoise")
        .arg(Arg::with_name("log")
            .long("log")
            .help("log level error, warn, info or debug")
            .takes_value(true))
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
                .takes_value(true))
            .arg(Arg::with_name("ktype")
                .long("keytype")
                .help("key type")
                .takes_value(true)))
        .get_matches();

    let log_level = args.value_of("log").unwrap_or("info");
    let env = env_logger::Env::new().filter_or("MY_LOG", log_level);
    let mut builder = Builder::new();
    builder.parse_env(env);
    builder.format_timestamp_millis();
    builder.init();

    #[cfg(feature = "prod")]
    if let Some(x) = args.subcommand_matches("start") {
        let bootstrap_addr_str_option = x.value_of("bootstrap");
        info!("[WhiteNoise] bootstrap_addr:{:?}", bootstrap_addr_str_option);
        let port_str_option = x.value_of("port");
        info!("[WhiteNoise] port str:{:?}", port_str_option);
        let bootstrap_addr_option = bootstrap_addr_str_option.map(String::from);
        let port_option = port_str_option.map(String::from);
        let key_type = String::from(x.value_of("ktype").unwrap_or("ed25519"));

        let (_sender, receiver) = oneshot::channel::<()>();
        let _node = start_server(bootstrap_addr_option, port_option, key_type, None).await;
        receiver.await.unwrap();
    }
    Ok(())
}