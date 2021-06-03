pub mod network;
pub mod account;
pub mod models;

pub mod command_proto {
    include!(concat!(env!("OUT_DIR"), "/command_proto.rs"));
}

pub mod request_proto {
    include!(concat!(env!("OUT_DIR"), "/request_proto.rs"));
}

pub mod relay_proto {
    include!(concat!(env!("OUT_DIR"), "/relay_proto.rs"));
}

pub mod payload_proto {
    include!(concat!(env!("OUT_DIR"), "/payload_proto.rs"));
}

pub mod gossip_proto {
    include!(concat!(env!("OUT_DIR"), "/gossip_proto.rs"));
}

pub mod chat_proto {
    include!(concat!(env!("OUT_DIR"), "/chat_proto.rs"));
}