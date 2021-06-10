# WhiteNoise.rs

The implementation of privacy p2p network protocol in Rust

## WhiteNoise Protocol

WhiteNoise is an overlay privacy network protocol. It is designed to provide comprehensive network privacy protection,
including link privacy, node privacy, data privacy and traffic privacy. It is also a decentralized and open network.
Anyone can act as a Node participate in the network to relay data transmissions, or a Terminal to use private
connections.

WhiteNoise Protocol has superior robustness, ease of use, and cross-platform. It can provide safe and reliable
transmission capabilities in a very friendly manner, allowing upper-layer applications to easily and confidently focus
on their own business innovations. The privacy of the data is fully guaranteed by WhiteNoise.

Learn more [specifics about WhiteNoise](./whitenoise.md).

## Build

Building WhiteNoise requires Rust toolchain. See more for how to install
Rust [here](https://www.rust-lang.org/tools/install).

Use the following command to build the WhiteNoise node:

```shell
cargo build --release
```

## Embedded Docs

Once the project has been built, the following command can be used to explore all parameters and subcommands:

```shell
./target/release/whitenoisers -h
```

## Run

### Start Local WhiteNoise Network

#### 1. Start Bootstrap Node

This command will start a WhiteNoise node as a Bootstrap, listening to port "3331":

```shell
./whitenoisers start --port 3331
```

After running this command, the local **MultiAddress** of Bootstrap is shown in log like the following:

```verilog
[2021-06-07T08:42:53.183Z INFO  whitenoisers::sdk::host] local Multiaddress: /ip4/127.0.0.1/tcp/3331/p2p/12D3KooWMNFaCGrnfMomi4TTMvQsKMGVwoxQzHo6P49ue6Fwq6zU
```

#### 2. Start WhiteNoise Nodes

This command will start a WhiteNoise node as normal relay node, listening to port "3332". Make sure the port is
available and fill in the Bootstrap **MultiAddress** in the `--bootstrap` flag:

```shell
./whitenoisers start --port 3332 --bootstrap /ip4/127.0.0.1/tcp/3331/p2p/12D3KooWMNFaCGrnfMomi4TTMvQsKMGVwoxQzHo6P49ue6Fwq6zU
```

Change the port and start more nodes.

### Join WhiteNoise Network

You are able to join remote WhiteNoise network as a relay node, if you know the **MultiAddress** of it's Bootstrap. Just
simply fill in the bootstrap flag.

## Test 
This command run tests:
```shell
cargo test
```

## Contributing

Thank you for considering contributing to Evanesco. We welcome any individuals and organizations on the Internet to
participate in this open source project.