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

Learn more [specifics about WhiteNoise](docs/whitenoise_spec.md) and WhiteNoise multi-hop connection [Workflows](./docs/workflows.md).

## WhiteNoise Network

### Build

Building WhiteNoise requires Rust toolchain. See more for how to install
Rust [here](https://www.rust-lang.org/tools/install).

Use the following command to build the WhiteNoise node:

```shell
cargo build --release
```

### Embedded Docs

Once the project has been built, the following command can be used to explore all parameters and subcommands:

```shell
./target/release/whitenoisers -h
```

### Start WhiteNoise Network

#### Start Local WhiteNoise Network
At first, we suggest user build 5 directories at least, contains boot and node1/2/3/4
```shell
mkdir boot node1 node2 node3 node4    
```

By then, we can build soft link from whitenoisers binary to above directories:
```shell
cd directory/of/boot
ln -s binary/path/of/whitenoisers directory/of/boot/whitenoisers

cd directory/of/node1
ln -s binary/path/of/whitenoisers directory/of/node1/whitenoisers

cd directory/of/node2
ln -s binary/path/of/whitenoisers directory/of/node2/whitenoisers

cd directory/of/node3
ln -s binary/path/of/whitenoisers directory/of/node3/whitenoisers

cd directory/of/node4
ln -s binary/path/of/whitenoisers directory/of/node4/whitenoisers
```

##### 1. Start Bootstrap Node

This command will start a WhiteNoise node as a Bootstrap, listening to port "3331":

```shell
cd boot
 ./whitenoisers start --port 3331
```

After running this command, the local **MultiAddress** of Bootstrap is shown in log like the following:
![img.png](docs/pics/img0.png)
Notice: Please remember the value of Multiaddress(*/ip4/127.0.0.1/tcp/3331/p2p/12D3KooWL4obTZmoVKWxxxSymX6P7iJHu5HuDqL5c3sNAqBP2NwE*), we will use it for bootstraping node1/node2/node3/node4 in future.

##### 2. Start WhiteNoise Node1

This command will start a WhiteNoise node as normal routing node, listening to port "3332". Make sure the port is
available and fill in the Bootstrap **MultiAddress** in the `--bootstrap` flag:

```shell
cd node1
./whitenoisers start --port 3332 --bootstrap /ip4/127.0.0.1/tcp/3331/p2p/12D3KooWL4obTZmoVKWxxxSymX6P7iJHu5HuDqL5c3sNAqBP2NwE
```
At the same time, we will see some **JOINING** logs printed in boot node as following:
![img.png](./docs/pics/img1.png)

##### 3. Start WhiteNoise Node2
We will do as step 2 to start node2 use the same MultiAddress value and use another --port value 3333:
```shell
cd node2
./whitenoisers start --port 3333 --bootstrap /ip4/127.0.0.1/tcp/3331/p2p/12D3KooWL4obTZmoVKWxxxSymX6P7iJHu5HuDqL5c3sNAqBP2NwE
```
Of course, we will see some **JOINING** logs about node2 printed in boot node as following:
![img.png](docs/pics/img2.png)

Change the port and start node3 and node4, finally we will build a network with four routing nodes and one bootstrap node.

#### Join WhiteNoise Network

You are able to join remote WhiteNoise network as a routing node, if you know the **MultiAddress** of it's Bootstrap. Just
simply fill in the bootstrap flag.

## WhiteNoise Client
WhiteNoise Clients are able to access WhiteNoise network and build P2P privacy connection to another client.
It is a multi-hop connection and several nodes in the WhiteNoise network ack as relays to transfer connection data.

We implement the WhiteNoise Client SDK and a P2P chat example.

### Overview

Every client starts with an account associate with a crypto keypair. An unique WhiteNoiseID is derived from this keypair and
it identifies the client in WhiteNoise network. With privacy protection, dialing another client doesn't needs her IP
address, but only her WhiteNoiseID. So, clients who want to start connections share their WhiteNoiseIDs.

After Circuit Connection successfully built, both clients are able to read and write on this connection with network
privacy and security protection.

### Init Client
A client can be inited with the following method of `whitenoisers::sdk::client::WhiteNoiseClient`:
```rust
pub fn init(bootstrap_addr_str: String, key_type: crate::account::key_types::KeyType, keypair: Option<libp2p::identity::Keypair>) -> Self
```
Flag `bootstrap_addr_str` determines the Bootstrap node of the WhiteNoise network that the client is connected to. A
WhiteNoise network may have multiple Bootstraps, and only clients in the same WhiteNoise Network are able to connect to
each other.

Two kinds of key types Ed25519 and Secp256k1 are supported. If `keypair` is `None`, it will generate and store a new
keypair of the selected key type.

### Client APIs
Trait `whitenoisers::sdk::client::Client` defines the APIs in the following:

- Get nodes of the WhiteNoise network. It return `PeerID` of no more than `cnt` nodes. PeerID is the identity of a
  WhiteNoise node.

```rust
async fn get_main_net_peers(&mut self, cnt: i32) -> Vec<PeerId>
```


- Register to a node with `peer_id` as proxy to access the WhiteNoise network. Clients are able to chose proxy randomly or
  set their own strategy.

```rust
async fn register(&mut self, peer_id: PeerId) -> bool
```


- Dial another client, and returns a SessionID if dialing success. SessionID is the unique identity of a circuit
  connection, which shares by both sides of the connection. We supports multiplexing connection, so a client is able to
  maintain multiple circuit connections. These connections are identified by SessionID.

```rust
async fn dial(&mut self, remote_id: String) -> String;
```


- Get circuit connection of certain SessionId.

```rust
fn get_circuit(&self, session_id: &str) -> Option<CircuitConn>
```


- Send message in a circuit connection of SessionID.

```rust
async fn send_message(&self, session_id: &str, data: &[u8])
```


- Close the circuit connection with session_id. This function will close the sub-streams of all relay nodes and the
  clients of this circuit connection.

```rust
 async fn disconnect_circuit(&mut self, session_id: String)
```


7. Get client's unique WhiteNoiseId.

```rust
fn get_whitenoise_id( & self ) -> String
```


8. Pop latest inbound or outbound circuit connection's SessionID.

```rust
async fn notify_next_session(&mut self) -> Option<String>
```

### Chat Client Example

We implement a P2P chat application on WhiteNoise network as an client example.

#### Build Chat Client

Use the following command to build the WhiteNoise chat client:

```shell
cargo build -p whitenoise-client --release
```
Once the project has been built, the following command can be used to explore all parameters and subcommands:

```shell
./target/release/whitenoise-client -h
```

#### Run Chat Client
Copy `./target/release/whitenoise-client` into two different directories one as caller and another as answer.
Then follow these steps:

##### 1. Start WhiteNoise Network 
Follow instructions above to start local WhiteNoise Network or get the Bootstrap **MultiAddress** of a remote WhiteNoise network.

##### 2. Start Answer Client
Start an **Answer** client waiting for others to dial with this command.

Set your nick name in the `--nick` flag.
Fill in the `--bootstrap` flag with the Bootstrap **MultiAddress** of the WhiteNoise Network you are trying to connect to:

```shell
./whitenoise-client chat --bootstrap /ip4/127.0.0.1/tcp/3331/p2p/12D3KooWMNFaCGrnfMomi4TTMvQsKMGVwoxQzHo6P49ue6Fwq6zU --nick Alice
```

Your unique **WhiteNoiseID** is shown in log. The **WhiteNoiseID** keeps the same, if you start chat client in the same directory and using the same key type.

The following shows the WhiteNoiseID in log:
```verilog
[2021-06-07T07:59:21.443Z INFO  whitenoisers::network::node] local whitenoise id:0HejBsyG9SPV5YB91Xf2zXiNGJQagRL3yAq7qtCVum4Pw
```

#### 3. Start Caller Client
Start a **Caller** client and dial the **Answer** with this command, fill in the `--id` flag with *Answer*'s *WhiteNoiseID*,
fill in the `--bootstrap` flag with the Bootstrap MultiAddress same as **Answer** client:

```shell
./whitenoise-client chat --bootstrap /ip4/127.0.0.1/tcp/3331/p2p/12D3KooWMNFaCGrnfMomi4TTMvQsKMGVwoxQzHo6P49ue6Fwq6zU --nick Bob -n 0HejBsyG9SPV5YB91Xf2zXiNGJQagRL3yAq7qtCVum4Pw
```

After seeing "Build circuit success!" in log, both chat clients are able to type and chat on the command line!

## Test 
This command run tests:
```shell
cargo test
```

## Contributing

Thank you for considering contributing to Evanesco. We welcome any individuals and organizations on the Internet to
participate in this open source project.