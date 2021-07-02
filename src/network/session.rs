
use crate::network::protocols::relay_behaviour::WrappedStream;


#[derive(Clone)]
pub enum SessionRole {
    CallerRole = 1,
    EntryRole,
    SinkRole,
    RelayRole,
    AccessRole,
    AnswerRole,
}

#[derive(Clone)]
pub struct PairStream {
    pub early_stream: std::option::Option<WrappedStream>,
    pub later_stream: std::option::Option<WrappedStream>,
}

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub pair_stream: PairStream,
    pub session_role: i32,
}

impl Session {
    pub fn ready(&self) -> bool {
        self.pair_stream.early_stream.is_some() && self.pair_stream.later_stream.is_some()
    }
}