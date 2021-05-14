pub mod network;
pub mod account;

pub mod command_proto {
    include!(concat!(env!("OUT_DIR"), "/command_proto.rs"));
}
