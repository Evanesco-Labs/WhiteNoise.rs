use libp2p::PeerId;

#[derive(Clone,Debug)]
pub struct ClientInfo{
    pub whitenoise_id: String,
    pub peer_id: PeerId,
    pub state: i32,
    pub time: std::time::Duration
}