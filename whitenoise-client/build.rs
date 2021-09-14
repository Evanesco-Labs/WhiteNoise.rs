fn main() {
    prost_build::compile_protos(&["src/chat.proto"], &["src"]).unwrap();
}