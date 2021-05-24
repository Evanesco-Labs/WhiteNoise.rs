pub mod network;
pub mod account;

pub mod command_proto {
    include!(concat!(env!("OUT_DIR"), "/command_proto.rs"));
}

pub mod request_proto {
    include!(concat!(env!("OUT_DIR"), "/request_proto.rs"));
}

pub mod relay_proto {
    include!(concat!(env!("OUT_DIR"), "/relay_proto.rs"));
}