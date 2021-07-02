use libp2p::{PeerId};
use crate::{command_proto, gossip_proto, network::{protocols::cmd_protocol::CmdRequest,
                                                   utils::{from_bytes_get_id},
                                                   whitenoise_behaviour::NodeCmdRequest}, request_proto};
use prost::Message;
use log::{info};
use super::{whitenoise_behaviour::{GetMainNets}};
use super::protocols::proxy_protocol::{ProxyRequest};
use super::protocols::ack_protocol::{AckRequest};
use super::whitenoise_behaviour::{NodeRequest, NodeProxyRequest};
use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use super::utils::{from_whitenoise_to_hash};
use super::node::{Node};
use super::session::SessionRole;
use super::utils::{from_request_get_id, send_relay_twoway, new_relay_circuit_success, new_relay_probe};


use libp2p::gossipsub::{
    GossipsubMessage
};

pub async fn process_publish_request(mut publish_receiver: UnboundedReceiver<GossipsubMessage>, mut node: Node) {
    loop {
        let gossipsub_message_opt = publish_receiver.next().await;
        if gossipsub_message_opt.is_none() {
            break;
        }
        let gossipsub_message = gossipsub_message_opt.unwrap();
        let encrypted_neg = gossip_proto::EncryptedNeg::decode(gossipsub_message.data.as_slice()).unwrap();
        if node.client_wn_map.read().unwrap().contains_key(&encrypted_neg.des) {
            let client_info = node.client_wn_map.read().unwrap().get(&encrypted_neg.des).cloned().unwrap();
            let cypher = encrypted_neg.cypher;
            let decrypt = request_proto::Decrypt { cypher, destination: encrypted_neg.des.clone() };
            let mut decrypt_data = Vec::new();
            decrypt.encode(&mut decrypt_data).unwrap();
            let mut request = request_proto::Request {
                req_id: String::from(""),
                from: node.get_id(),
                reqtype: request_proto::Reqtype::DecryptGossip as i32,
                data: decrypt_data,
            };
            let key = from_request_get_id(&request);
            request.req_id = key.clone();
            let node_request = NodeRequest::ProxyRequest(NodeProxyRequest { remote_peer_id: client_info.peer_id, proxy_request: Some(ProxyRequest(request)), peer_operation: None });
            let ack_response = node.external_send_node_request_and_wait(key, node_request).await;
            let AckRequest(ack) = ack_response;
            if !ack.result {
                info!("[WhiteNoise] client decrypt error");
                continue;
            }
            let negotiate = gossip_proto::Negotiate::decode(ack.data.as_slice()).unwrap();
            info!("[WhiteNoise] I have the answer client, ack as access node, session id:{}", negotiate.session_id);
            let wraped_stream_opt = node.new_session_to_peer(&client_info.peer_id, negotiate.session_id.clone(), SessionRole::AccessRole as i32, SessionRole::AnswerRole as i32).await;
            if wraped_stream_opt.is_none() {
                info!("[WhiteNoise] new session to answer client error");
                node.handle_close_session(&negotiate.session_id).await;
                continue;
            }
            if negotiate.join == node.get_id() {
                info!("[WhiteNoise] act both sink and access,session id:{}", negotiate.session_id);

                let circuit_success_relay = new_relay_circuit_success(&negotiate.session_id);
                let new_session_opt = {
                    let guard = node.session_map.write().unwrap();
                    (*guard).get(&negotiate.session_id).cloned()
                };
                if new_session_opt.is_none() {
                    info!("[WhiteNoise] sink and access node have no session");
                    continue;
                }
                if new_session_opt.is_some() {
                    let new_session = new_session_opt.unwrap();
                    send_relay_twoway(&new_session, circuit_success_relay).await;
                }
                continue;
            }

            let (sender, receiver) = futures::channel::oneshot::channel();
            let get_main_nets = GetMainNets { command_id: String::from("123"), remote_peer_id: PeerId::random(), num: 100, sender };
            node.node_request_sender.unbounded_send(NodeRequest::GetMainNetsRequest(get_main_nets)).unwrap();
            let nodeinfos_res = receiver.await;
            if nodeinfos_res.is_err() {
                info!("[WhiteNoise] get other nets error");
                continue;
            }
            let nodeinfos = nodeinfos_res.unwrap();
            let mut invalid: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
            let mut try_join = false;
            let mut relay = PeerId::random();

            for i in 0..3 {
                info!("[WhiteNoise] try {} for connecto to other peer for relay role", i);
                let mut index = rand::random::<usize>();

                for _j in 0..(nodeinfos.len()) {
                    index += 1;
                    index %= nodeinfos.len();
                    let id = nodeinfos.get(index).unwrap().id.clone();
                    let bootstrap_peer_id = node.boot_peer_id.unwrap_or_else(|| { PeerId::random() });
                    if !invalid.contains_key(&id) && id != node.get_id() && id != negotiate.join && id != bootstrap_peer_id.to_base58() {
                        let remote_client_white_noise_id_hash = node.client_peer_map.read().unwrap().get(&id).cloned();
                        if remote_client_white_noise_id_hash.is_none() || from_whitenoise_to_hash(remote_client_white_noise_id_hash.unwrap().as_str()) != encrypted_neg.des {
                            relay = PeerId::from_bytes(bs58::decode(id).into_vec().unwrap().as_slice()).unwrap();
                            break;
                        }
                    }
                }
                let wraped_stream_opt = node.new_session_to_peer(&relay, negotiate.session_id.clone(), SessionRole::AccessRole as i32, SessionRole::RelayRole as i32).await;
                if wraped_stream_opt.is_none() {
                    invalid.insert(relay.to_base58(), true);
                } else {
                    try_join = true;
                    break;
                }
            }

            if !try_join {
                info!("[WhiteNoise] try three times find relay but failed");
                node.handle_close_session(&negotiate.session_id).await;
                continue;
            }
            let session_expand = command_proto::SessionExpend { session_id: negotiate.session_id.clone(), peer_id: negotiate.join };
            let mut session_expand_data = Vec::new();
            session_expand.encode(&mut session_expand_data).unwrap();

            let mut command = command_proto::Command {
                command_id: String::from(""),
                r#type: command_proto::Cmdtype::SessionExPend as i32,
                from: node.get_id(),
                data: session_expand_data,
            };
            let mut command_data = Vec::new();
            command.encode(&mut command_data).unwrap();
            let key = from_bytes_get_id(&command_data);
            command.command_id = key.clone();
            let node_cmd_request = NodeCmdRequest {
                remote_peer_id: relay,
                cmd_request: Some(CmdRequest(command)),
            };

            let ack_response = node.external_send_node_request_and_wait(key, NodeRequest::CmdRequest(node_cmd_request)).await;
            let AckRequest(ack) = ack_response;
            if !ack.result {
                info!("[WhiteNoise] session expand failed");
                node.handle_close_session(&negotiate.session_id).await;
                continue;
            }
            //probe
            info!("[WhiteNoise] prepare to send probe");

            let probe_relay = new_relay_probe(negotiate.session_id.as_str());

            let session_opt = node.session_map.read().unwrap().get(&negotiate.session_id).cloned();
            if session_opt.is_none() {
                info!("[WhiteNoise] publish want to send probe,but session is null");
                continue;
            }
            let session = session_opt.unwrap();
            send_relay_twoway(&session, probe_relay).await;
        }
    }
}