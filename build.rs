fn main() {
    prost_build::compile_protos(&["src/command.proto"], &["src"]).unwrap();
    prost_build::compile_protos(&["src/request.proto"], &["src"]).unwrap();
}