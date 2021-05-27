use tokio::sync::mpsc;
use crate::network::protocols::relay_behaviour::WrappedStream;
use crate::relay_proto;

#[derive(Clone)]
pub enum SessionRole {
    CallerRole = 1,
    EntryRole,
    JointRole,
    RelayRole,
    ExitRole,
    AnswerRole,
}

#[derive(Clone)]
pub struct PairStream {
    pub early_stream: std::option::Option<WrappedStream>,
    pub later_stream: std::option::Option<WrappedStream>,
}

pub struct LaterChannel {
    pub inbound_receiver: std::sync::Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<relay_proto::Relay>>>,
    pub outbound_sender: mpsc::UnboundedSender<relay_proto::Relay>,
}

#[derive(Clone)]
pub struct PairSender {
    pub early_outbound_sender: mpsc::UnboundedSender<relay_proto::Relay>,
    pub later_outbound_sender: mpsc::UnboundedSender<relay_proto::Relay>,
}

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub pair_stream: PairStream,
    pub session_role: i32,
}

impl Session {
    pub fn ready(&self) -> bool {
        return self.pair_stream.early_stream.is_some() && self.pair_stream.later_stream.is_some();
    }
}