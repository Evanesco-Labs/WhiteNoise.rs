use libp2p::{PeerId};
use crate::{command_proto, relay_proto};
use prost::Message;
use log::{info};
use super::{protocols::cmd_protocol::CmdRequest};
use super::protocols::ack_protocol::{AckRequest};
use eth_ecies::{Secret};
use super::whitenoise_behaviour::{NodeCmdRequest, NodeRequest, NodeAckRequest};
use tokio::sync::mpsc::{UnboundedReceiver};
use crate::account::account::Account;
use super::node::{Node};
use super::session::SessionRole;
use super::utils::{new_relay_circuit_success, new_relay_probe, forward_relay, send_relay_twoway};


pub async fn process_cmd_request(mut cmd_request_receiver: UnboundedReceiver<NodeCmdRequest>, mut node: Node) {
    loop {
        let cmd_request_option = cmd_request_receiver.recv().await;
        if cmd_request_option.is_some() {
            let node_cmd_request = cmd_request_option.unwrap();

            let CmdRequest(request) = node_cmd_request.cmd_request.clone().unwrap();

            if request.r#type == (command_proto::Cmdtype::SessionExPend as i32) {
                let session_expend = command_proto::SessionExpend::decode(request.data.as_slice()).unwrap();
                let mut ack = command_proto::Ack { command_id: request.command_id, result: false, data: Vec::new() };
                let session = node.session_map.read().unwrap().get(&session_expend.session_id).and_then(|x| {
                    Some(x.clone())
                });
                info!("prepare to process cmd request,i am relay node,session id:{}", session_expend.session_id);
                if session.is_none() {
                    node.handle_close_session(&session_expend.session_id).await;
                    ack.data = "No such session".as_bytes().to_vec();
                    let ack_request = NodeRequest::AckRequest(NodeAckRequest { remote_peer_id: node_cmd_request.remote_peer_id.clone(), ack_request: Some(AckRequest(ack)) });
                    node.send_ack(ack_request).await;
                    continue;
                }
                if session.as_ref().unwrap().ready() {
                    info!("session is ready,both relay and entry,session id:{}", session_expend.session_id);
                    ack.result = true;
                    let ack_request = NodeRequest::AckRequest(NodeAckRequest { remote_peer_id: node_cmd_request.remote_peer_id.clone(), ack_request: Some(AckRequest(ack)) });
                    node.send_ack(ack_request).await;

                    let circuit_success_relay = new_relay_circuit_success(&session_expend.session_id);
                    let new_session = session.unwrap();
                    send_relay_twoway(&new_session, circuit_success_relay).await;
                    continue;
                }
                let joint_peer_id = PeerId::from_bytes(bs58::decode(session_expend.peer_id).into_vec().unwrap().as_slice()).unwrap();
                let wraped_stream = node.new_session_to_peer(&joint_peer_id, session_expend.session_id.clone(), SessionRole::RelayRole as i32, SessionRole::JointRole as i32).await;
                if wraped_stream.is_none() {
                    node.handle_close_session(&session_expend.session_id).await;
                    ack.data = "new session error".as_bytes().to_vec();
                    let ack_request = NodeRequest::AckRequest(NodeAckRequest { remote_peer_id: node_cmd_request.remote_peer_id.clone(), ack_request: Some(AckRequest(ack)) });
                    node.send_ack(ack_request).await;
                    continue;
                } else {
                    ack.result = true;
                    let ack_request = NodeRequest::AckRequest(NodeAckRequest { remote_peer_id: node_cmd_request.remote_peer_id.clone(), ack_request: Some(AckRequest(ack)) });
                    node.send_ack(ack_request).await;
                    continue;
                }
            }
        } else {
            info!("cmd sender all stop");
            break;
        }
    }
}


