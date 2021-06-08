# WhiteNoise Client Instruction

WhiteNoise is an overlay privacy network protocol, which supports p2p multi-hop connections between clients. We call it
Circuit Connection.

Every client have an account associate with a crypto keypair. An unique WhiteNoiseID is derived from this keypair and
it's the client's identity in WhiteNoise network. With privacy protection, dialing another client doesn't needs her IP
address, but only her WhiteNoiseID. So, clients who want to start connections share their WhiteNoiseIDs.

After Circuit Connection successfully built, both clients are able to read and write on this connection with network
privacy and security protection.

## Init Client

`bootstrap_addr_str` determines the Bootstrap node of the WhiteNoise network that the client is connected to. A
WhiteNoise network may have multiple Bootstraps, and only clients in the same WhiteNoise Network are able to connect to
each other.

We support two kinds of key types Ed25519 and Secp256k1. If `keypair` is `None`, it will generate and store a new
keypair of the selected key type.

```rust
pub fn init(bootstrap_addr_str: String, key_type: KeyType, keypair: Option<libp2p::identity::Keypair>) -> WhiteNoiseClient
```

## Client APIs

Get nodes of the WhiteNoise network. It return `PeerID` of no more than `cnt` nodes. PeerID is the identity of a
WhiteNoise node.

```rust
async fn get_main_net_peers(&mut self, cnt: i32) -> Vec<PeerId>
```


Register to a node with `peer_id` as proxy to access the WhiteNoise network. Clients are able to chose proxy randomly or
set their own strategy.

```rust
async fn register(&mut self, peer_id: PeerId) -> bool
```


Dial another client, and returns a SessionID if dialing success. SessionID is the unique identity of a circuit
connection, which shares by both sides of the connection. We supports multiplexing connection, so a client is able to
maintain multiple circuit connections. These connections are identified by SessionID.

```rust
async fn dial(&mut self, remote_id: String) -> String;
```


Get circuit connection of certain SessionId.

```rust
fn get_circuit(&self, session_id: &str) -> Option<CircuitConn>
```


Send message in a circuit connection of SessionID.

```rust
async fn send_message(&self, session_id: &str, data: &[u8])
```


Close the circuit connection with session_id. This function will close the sub-streams of all relay nodes and the
clients of this circuit connection.

```rust
 async fn disconnect_circuit(&mut self, session_id: String)
```


Get client's unique WhiteNoiseId.

```rust
get_whitenoise_id( & self ) -> String
```


Pop latest inbound or outbound circuit connection's SessionID.

```rust
async fn notify_next_session(&mut self) -> Option<String>
```
