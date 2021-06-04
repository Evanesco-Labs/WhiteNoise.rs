use serde::{Serialize, Deserialize};
#[derive(Serialize, Deserialize,Debug,Clone)]
pub struct ChatMessage{
    pub peer_id: String,
    pub data: Vec<u8>
}
