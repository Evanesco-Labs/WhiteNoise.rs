use libp2p::{
    Multiaddr,
    PeerId,
    core::connection::ConnectionId,
    swarm::{
        NetworkBehaviour,
        NetworkBehaviourAction,
        NotifyHandler,
        PollParameters,
        NegotiatedSubstream,
    },
    core::upgrade::{self, ReadOneError},
};
use log::{info, debug};

use std::{collections::{VecDeque}, task::{Context, Poll}};
use super::relay_protocol::{RelayHandler, RelayHandlerInEvent, RelayHandlerOutEvent};
use smallvec::SmallVec;
use uuid::Uuid;

// use futures::AsyncWriteExt;
use futures::{AsyncWriteExt, AsyncReadExt, StreamExt};
use futures::future::FutureExt;


#[derive(Clone)]
pub struct WrappedStream {
    pub stream: std::sync::Arc<futures::lock::Mutex<NegotiatedSubstream>>,
    pub remote_peer_id: PeerId,
    pub stream_id: String,
    pub out_receiver: std::sync::Arc<futures::lock::Mutex<futures::channel::mpsc::UnboundedReceiver<WrapedData>>>,
    pub out_sender: futures::channel::mpsc::UnboundedSender<WrapedData>,
    pub in_receiver: std::sync::Arc<futures::lock::Mutex<futures::channel::mpsc::UnboundedReceiver<Result<Vec<u8>, ReadOneError>>>>,
    pub in_sender: futures::channel::mpsc::UnboundedSender<Result<Vec<u8>, ReadOneError>>,

}

pub enum WrapedData {
    Close,
    Data(Vec<u8>),
}

impl WrappedStream {
    pub async fn write(&mut self, data: Vec<u8>) {
        self.out_sender.unbounded_send(WrapedData::Data(data));
    }
    pub async fn read(&mut self) -> Result<Vec<u8>, ReadOneError> {
        let read_res = self.in_receiver.lock().await.next().await;
        if read_res.is_none() {
            return Err(ReadOneError::Io(std::io::Error::new(std::io::ErrorKind::Other, "stream reader sender all destroyed")));
        }
        return read_res.unwrap();
    }
    pub async fn close(&mut self) {
        self.out_sender.unbounded_send(WrapedData::Close);
    }
}

pub async fn start_poll(wraped_stream: WrappedStream) {
    let mut stream = wraped_stream.stream.lock().await;
    let mut receiver = wraped_stream.out_receiver.lock().await;
    loop {
        futures::select! {
            data = (*receiver).next().fuse() =>{
                if data.is_none(){
                    info!("[WhiteNoise] stream out sender is null,close");
                    wraped_stream.in_sender.unbounded_send(Err(ReadOneError::Io(std::io::Error::new(std::io::ErrorKind::Other, "stream out sender is null,close"))));
                    (*stream).close().await;
                    break;
                }
                let wraped_data: WrapedData = data.unwrap();
                match wraped_data{
                    WrapedData::Close =>{
                        info!("[WhiteNoise] prepare to close stream for active close");
                        (*stream).close().await;
                        wraped_stream.in_sender.unbounded_send(Err(ReadOneError::Io(std::io::Error::new(std::io::ErrorKind::Other, "active close stream"))));
                        break;
                    }
                    WrapedData::Data(x) =>{
                        upgrade::write_with_len_prefix(&mut *stream, &x).await.unwrap();
                    }
                }
            }

            read_res = upgrade::read_one(&mut *stream, 4096).fuse() =>{
                if read_res.is_err(){
                    info!("[WhiteNoise] stream read error,so we close,{:?}",read_res.as_ref().err());
                    wraped_stream.in_sender.unbounded_send(read_res);
                    break;
                }
                if read_res.is_ok() && read_res.as_ref().unwrap().clone().len()<=0{
                    info!("[WhiteNoise] stream read len is zero");
                    wraped_stream.in_sender.unbounded_send(Err(ReadOneError::Io(std::io::Error::new(std::io::ErrorKind::Other, "stream read len zero"))));
                    break;
                }
                let in_sender_cp = wraped_stream.in_sender.clone();
                async_std::task::spawn(async move{
                    in_sender_cp.unbounded_send(read_res);
                });
            }
        }
    }
}

pub enum RelayEvent {
    RelayInbound(WrappedStream),
    RelayOutbound(WrappedStream),
    Disconnect(PeerId),
}

pub struct Relay {
    pub out_events: VecDeque<RelayEvent>,
    pub dive_events: VecDeque<RelayHandlerInEvent>,
    pub addresses: std::collections::HashMap<PeerId, SmallVec<[Multiaddr; 6]>>,
}

impl Relay {
    pub fn new_stream(&mut self, peer_id: &PeerId) {
        self.dive_events.push_back(RelayHandlerInEvent::Dial(peer_id.clone()))
    }
}

pub fn from_neg_to_wraped(stream: NegotiatedSubstream, peer: PeerId) -> WrappedStream {
    let stream_id = Uuid::new_v4().to_string();
    let (out_bound_sender, out_bound_receiver) = futures::channel::mpsc::unbounded();
    let (in_bound_sender, in_bound_receiver) = futures::channel::mpsc::unbounded();
    let wraped_stream = WrappedStream {
        remote_peer_id: peer,
        stream: std::sync::Arc::new(futures::lock::Mutex::new(stream)),
        stream_id: stream_id,
        in_receiver: std::sync::Arc::new(futures::lock::Mutex::new(in_bound_receiver)),
        in_sender: in_bound_sender,
        out_receiver: std::sync::Arc::new(futures::lock::Mutex::new(out_bound_receiver)),
        out_sender: out_bound_sender,
    };
    let wraped_stream_cp = wraped_stream.clone();
    async_std::task::spawn(start_poll(wraped_stream_cp));
    return wraped_stream;
}

impl NetworkBehaviour for Relay {
    type ProtocolsHandler = RelayHandler;
    type OutEvent = RelayEvent;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        RelayHandler::new()
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        let mut addresses = Vec::new();

        let exist_addresses_option = self.addresses.get(_peer_id);
        if exist_addresses_option.is_some() {
            let exist_addresses = exist_addresses_option.unwrap();
            exist_addresses.iter().for_each(|x| addresses.push(x.clone()))
        }
        addresses
    }

    fn inject_connected(&mut self, _: &PeerId) {
        debug!("relay connected");
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId) {
        debug!("relay behaviour connection disconnect for peer_id:{}", peer_id);
        self.out_events.push_front(RelayEvent::Disconnect(peer_id.clone()));
    }

    fn inject_event(&mut self, peer: PeerId, _: ConnectionId, result: RelayHandlerOutEvent) {
        match result {
            RelayHandlerOutEvent::RelayInbound(x) => {
                let wraped_stream = from_neg_to_wraped(x, peer.clone());
                self.out_events.push_front(RelayEvent::RelayInbound(wraped_stream));
            }
            RelayHandlerOutEvent::RelayOutbound(x) => {
                let wraped_stream = from_neg_to_wraped(x, peer.clone());
                self.out_events.push_front(RelayEvent::RelayOutbound(wraped_stream));
            }
        }
    }

    fn poll(&mut self, _: &mut Context<'_>, _: &mut impl PollParameters)
            -> Poll<NetworkBehaviourAction<RelayHandlerInEvent, RelayEvent>>
    {
        if let Some(e) = self.out_events.pop_back() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(e));
        }
        if let Some(e) = self.dive_events.pop_front() {
            let RelayHandlerInEvent::Dial(x) = e;
            debug!("prepare to new relay:{:?}", x);
            return Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                peer_id: x.clone(),
                handler: NotifyHandler::Any,
                event: RelayHandlerInEvent::Dial(x.clone()),
            });
        }
        Poll::Pending
    }
}