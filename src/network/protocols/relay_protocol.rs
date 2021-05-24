use libp2p::PeerId;

use futures::{prelude::*};

use std::{collections::VecDeque, iter, task::{Context, Poll}};

use libp2p::swarm::{
    NegotiatedSubstream,
    KeepAlive,
    SubstreamProtocol,
    ProtocolsHandler,
    ProtocolsHandlerEvent,
    ProtocolsHandlerUpgrErr,
};
use libp2p::core::{
    InboundUpgrade,
    OutboundUpgrade,
    UpgradeInfo,
    upgrade::ReadOneError,
};

use void::Void;

pub struct RelayHandler {
    inbound: VecDeque<NegotiatedSubstream>,
    outbound: VecDeque<NegotiatedSubstream>,
    events: VecDeque<RelayHandlerInEvent>,
}

#[derive(Debug)]
pub enum RelayHandlerOutEvent {
    RelayInbound(NegotiatedSubstream),
    RelayOutbound(NegotiatedSubstream),
}

pub enum RelayHandlerInEvent {
    Dial(PeerId)
}

impl RelayHandler {
    pub fn new() -> Self {
        RelayHandler {
            inbound: VecDeque::new(),
            outbound: VecDeque::new(),
            events: VecDeque::new(),
        }
    }
}

impl ProtocolsHandler for RelayHandler {
    type InEvent = RelayHandlerInEvent;
    type OutEvent = RelayHandlerOutEvent;
    type Error = ReadOneError;
    type InboundProtocol = RelayProtocol;
    type OutboundProtocol = RelayProtocol;
    type OutboundOpenInfo = ();
    type InboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<RelayProtocol, ()> {
        SubstreamProtocol::new(RelayProtocol, ())
    }

    fn inject_fully_negotiated_inbound(&mut self, stream: NegotiatedSubstream, (): ()) {
        log::debug!("ProtocolsHandler::inject_fully_negotiated_inbound");
        self.inbound.push_back(stream);
    }

    fn inject_fully_negotiated_outbound(&mut self, stream: NegotiatedSubstream, (): ()) {
        log::debug!("ProtocolsHandler::inject_fully_negotiated_outbound");
        self.outbound.push_back(stream);
    }

    fn inject_event(&mut self, x: RelayHandlerInEvent) {
        self.events.push_back(x);
    }

    fn inject_dial_upgrade_error(&mut self, _info: (), error: ProtocolsHandlerUpgrErr<Void>) {
        log::debug!("ProtocolsHandler::inject_dial_upgrade_error: {:?}", error);
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        KeepAlive::Yes
    }

    fn poll(&mut self, _cx: &mut Context<'_>) -> Poll<
        ProtocolsHandlerEvent<
            RelayProtocol,
            (),
            RelayHandlerOutEvent,
            Self::Error
        >
    > {
        log::debug!("ProtocolsHandler::poll begins...");
        if let Some(stream_s) = self.inbound.pop_front() {
            return Poll::Ready(ProtocolsHandlerEvent::Custom(RelayHandlerOutEvent::RelayInbound(stream_s)));
        }
        if let Some(stream_s) = self.outbound.pop_front() {
            return Poll::Ready(ProtocolsHandlerEvent::Custom(RelayHandlerOutEvent::RelayOutbound(stream_s)));
        }
        if self.events.pop_front().is_some() {
            return Poll::Ready(ProtocolsHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(RelayProtocol, ())
            });
        }
        Poll::Pending
    }
}


#[derive(Default, Debug, Copy, Clone)]
pub struct RelayProtocol;

impl InboundUpgrade<NegotiatedSubstream> for RelayProtocol {
    type Output = NegotiatedSubstream;
    type Error = Void;
    type Future = future::Ready<Result<NegotiatedSubstream, Void>>;


    fn upgrade_inbound(self, stream: NegotiatedSubstream, _: Self::Info) -> Self::Future {
        log::debug!("InboundUpgrade::upgrade_inbound");
        future::ok(stream)
    }
}

impl OutboundUpgrade<NegotiatedSubstream> for RelayProtocol {
    type Output = NegotiatedSubstream;
    type Error = Void;
    type Future = future::Ready<Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, stream: NegotiatedSubstream, _: Self::Info) -> Self::Future {
        log::debug!("OutboundUpgrade::upgrade_outbound");
        future::ok(stream)
    }
}

impl UpgradeInfo for RelayProtocol {
    type Info = &'static [u8];
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(b"/relay")
    }
}

