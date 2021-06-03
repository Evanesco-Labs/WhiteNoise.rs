fn main() {
    prost_build::compile_protos(&["src/command.proto"], &["src"]).unwrap();
    prost_build::compile_protos(&["src/request.proto"], &["src"]).unwrap();
    prost_build::compile_protos(&["src/relay.proto"], &["src"]).unwrap();
    prost_build::compile_protos(&["src/payload.proto"], &["src"]).unwrap();
    prost_build::compile_protos(&["src/gossip.proto"], &["src"]).unwrap();
    prost_build::compile_protos(&["src/chat.proto"], &["src"]).unwrap();
}