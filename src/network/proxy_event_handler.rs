use libp2p::{PeerId, identity};
use crate::{command_proto, gossip_proto, network::utils::{from_bytes_get_id, write_disconnect_arc}, relay_proto, request_proto};
use prost::Message;
use log::{info, error, warn, debug};
use super::{whitenoise_behaviour::{GetMainNets}};
use super::protocols::proxy_protocol::{ProxyRequest};
use super::protocols::ack_protocol::{AckRequest};
use eth_ecies::{Secret, Public};
use eth_ecies::crypto::ecies;
use crate::models::client_info::ClientInfo;
use super::whitenoise_behaviour::{self, NodeRequest, NodeProxyRequest, NodeAckRequest, PublishDataRequest};
use futures::{StreamExt,channel::mpsc::UnboundedReceiver};
use crate::account::account::Account;
use super::utils::{from_whitenoise_to_hash, forward_relay, send_relay_twoway, new_relay_circuit_success, new_relay_probe};
use super::node::{self, Node};
use super::session::SessionRole;
use super::protocols::relay_behaviour::WrappedStream;
use super::utils::from_request_get_id;
use multihash::{Code, MultihashDigest};

pub async fn process_proxy_request(mut proxy_request_receiver: UnboundedReceiver<NodeProxyRequest>, mut node: Node) {
    loop {
        let proxy_request_option = proxy_request_receiver.next().await;
        if proxy_request_option.is_some() {
            let node_proxy_request = proxy_request_option.unwrap();
            if node_proxy_request.peer_operation.is_some() {
                let operation = node_proxy_request.peer_operation.unwrap();
                match operation {
                    whitenoise_behaviour::PeerOperation::Disconnect => {
                        debug!("prepare to remove register proxy information in map:{}", node_proxy_request.remote_peer_id.to_base58());
                        let hash_option = {
                            let mut guard = node.client_peer_map.write().unwrap();
                            (*guard).remove(&node_proxy_request.remote_peer_id.to_base58())
                        };
                        if hash_option.is_some() {
                            let hash = hash_option.unwrap();
                            debug!("prepare to remove register proxy information in wn map:{}", hash);
                            let mut guard = node.client_wn_map.write().unwrap();
                            let zz = (*guard).remove(&hash);
                            debug!("prepare to remove register proxy information in wn map:{:?}", zz);
                        }
                    }
                }
                continue;
            }
            let ProxyRequest(request) = node_proxy_request.proxy_request.clone().unwrap();
            if request.reqtype == (request_proto::Reqtype::DecryptGossip as i32) {
                let mut ack_response = command_proto::Ack { command_id: request.req_id, result: false, data: Vec::new() };

                let decrypt = request_proto::Decrypt::decode(request.data.as_slice()).unwrap();

                let decrypt_res = match node.keypair {
                    identity::Keypair::Ed25519(ref x) => {
                        crate::account::account::Account::ecies_ed25519_decrypt(&node.keypair, &decrypt.cypher).unwrap()
                    }
                    identity::Keypair::Secp256k1(ref x) => {
                        crate::account::account::Account::ecies_decrypt(&node.keypair, &decrypt.cypher).unwrap()
                    }
                    _ => {
                        panic!("keypair not ed25519 or secp256k1");
                    }
                };
                {
                    let plain_text = decrypt_res;
                    let neg = gossip_proto::Negotiate::decode(plain_text.as_slice()).unwrap();
                    ack_response.result = true;
                    ack_response.data = plain_text;
                    let ack_request = AckRequest(ack_response);
                    let node_ack_request = NodeAckRequest {
                        remote_peer_id: node_proxy_request.remote_peer_id,
                        ack_request: Some(ack_request),
                    };
                    node.node_request_sender.unbounded_send(NodeRequest::AckRequest(node_ack_request));
                }
            } else if request.reqtype == (request_proto::Reqtype::NegPlainText as i32) {
                debug!("receive encrypt request");
                let mut ack_response = command_proto::Ack { command_id: request.req_id, result: false, data: Vec::new() };

                let neg_plaintext = request_proto::NegPlaintext::decode(request.data.as_slice()).unwrap();
                let guard = node.circuit_task.write().unwrap();
                let pub_bytes = (*guard).get(&neg_plaintext.session_id).unwrap();

                let crypted_text = if pub_bytes[0] == ('0' as u8){
                    crate::account::account::Account::ecies_ed25519_encrypt(&pub_bytes[1..], &neg_plaintext.neg)
                } else {
                    crate::account::account::Account::ecies_encrypt(&pub_bytes[1..], &neg_plaintext.neg)
                };

                ack_response.result = true;
                ack_response.data = crypted_text;
                let ack_request = AckRequest(ack_response);
                let node_ack_request = NodeAckRequest {
                    remote_peer_id: node_proxy_request.remote_peer_id,
                    ack_request: Some(ack_request),
                };
                node.node_request_sender.unbounded_send(NodeRequest::AckRequest(node_ack_request));
                debug!("send encrypt neg");
            } else if request.reqtype == (request_proto::Reqtype::MainNetPeers as i32) {
                //info!("receive get main_net_peers");
                let get_mainnets_request = request_proto::MainNetPeers::decode(request.data.as_slice()).unwrap();
                let (sender, receiver) = futures::channel::oneshot::channel();
                let get_main_nets = GetMainNets { command_id: request.req_id.clone(), remote_peer_id: node_proxy_request.remote_peer_id.clone(), num: get_mainnets_request.max, sender: sender };
                node.node_request_sender.unbounded_send(NodeRequest::GetMainNetsRequest(get_main_nets));
                let nodeinfos_res = receiver.await;
                if nodeinfos_res.is_ok() {
                    let peers_list = request_proto::PeersList { peers: nodeinfos_res.unwrap() };
                    let mut data = Vec::new();
                    peers_list.encode(&mut data).unwrap();
                    let ack = AckRequest(command_proto::Ack { command_id: request.req_id, result: true, data: data });
                    node.send_ack(NodeRequest::AckRequest(NodeAckRequest { remote_peer_id: node_proxy_request.remote_peer_id, ack_request: Some(ack) })).await;
                } else {
                    let ack = AckRequest(command_proto::Ack { command_id: request.req_id, result: false, data: Vec::new() });
                    node.send_ack(NodeRequest::AckRequest(NodeAckRequest { remote_peer_id: node_proxy_request.remote_peer_id, ack_request: Some(ack) })).await;
                }
            } else if request.reqtype == (request_proto::Reqtype::NewProxy as i32) {
                let new_proxy = request_proto::NewProxy::decode(request.data.as_slice()).unwrap();
                let hash = from_whitenoise_to_hash(new_proxy.white_noise_id.as_str());
                let find = {
                    let guard = node.client_wn_map.write().unwrap();
                    (*guard).get(&hash).is_some()
                };
                if true == find {
                    //info!("prepare to  register proxy information in map:{},and wn map exists",node_proxy_request.remote_peer_id.to_base58());
                    let data = "Proxy already".as_bytes().to_vec();
                    let mut ack_response = command_proto::Ack { command_id: request.req_id, result: false, data: data };
                    let ack_request = AckRequest(ack_response);
                    let node_ack_request = NodeAckRequest {
                        remote_peer_id: node_proxy_request.remote_peer_id,
                        ack_request: Some(ack_request),
                    };
                    node.node_request_sender.unbounded_send(NodeRequest::AckRequest(node_ack_request));
                } else {
                    {
                        //info!("prepare to  register proxy information in map:{}",node_proxy_request.remote_peer_id.to_base58());
                        let mut guard = node.client_peer_map.write().unwrap();
                        (*guard).insert(node_proxy_request.remote_peer_id.to_base58(), hash.clone());
                    }
                    {
                        let client_info = ClientInfo { whitenoise_id: new_proxy.white_noise_id, peer_id: node_proxy_request.remote_peer_id, state: 1, time: std::time::Duration::from_secs(3600) };
                        let mut guard = node.client_wn_map.write().unwrap();
                        //info!("prepare to  register proxy information in wn map:{}",hash);
                        (*guard).insert(hash, client_info);
                    }
                    let mut ack_response = command_proto::Ack { command_id: request.req_id, result: true, data: Vec::new() };
                    let ack_request = AckRequest(ack_response);
                    let node_ack_request = NodeAckRequest {
                        remote_peer_id: node_proxy_request.remote_peer_id,
                        ack_request: Some(ack_request),
                    };
                    node.node_request_sender.unbounded_send(NodeRequest::AckRequest(node_ack_request));
                }
            } else if request.reqtype == (request_proto::Reqtype::UnRegisterType as i32) {
                info!("receive unregister and going to remove register proxy information in map");
                let hash_option = {
                    let mut guard = node.client_peer_map.write().unwrap();
                    (*guard).remove(&node_proxy_request.remote_peer_id.to_base58())
                };
                if hash_option.is_some() {
                    let hash = hash_option.unwrap();
                    let mut guard = node.client_wn_map.write().unwrap();
                    (*guard).remove(&hash);
                }
            } else if request.reqtype == (request_proto::Reqtype::NewCircuit as i32) {
                let res = handle_new_circuit(node.clone(), request.clone(), node_proxy_request.remote_peer_id).await;
                let mut ack_response = command_proto::Ack { command_id: request.req_id, result: res, data: Vec::new() };
                let ack_request = AckRequest(ack_response);
                let node_ack_request = NodeAckRequest {
                    remote_peer_id: node_proxy_request.remote_peer_id,
                    ack_request: Some(ack_request),
                };
                node.node_request_sender.unbounded_send(NodeRequest::AckRequest(node_ack_request));
            }
        } else {
            info!("proxy sender all stop");
            break;
        }
    }
}

pub async fn handle_new_circuit(mut node: Node, request: request_proto::Request, remote_peer_id: PeerId) -> bool {
    let new_circuit_rst = request_proto::NewCircuit::decode(request.data.as_slice());
    if new_circuit_rst.is_err() {
        info!("parse new circuit error");
        return false;
    }
    let new_circuit = new_circuit_rst.unwrap();
    if node.client_wn_map.read().unwrap().get(&new_circuit.from).is_none() {
        info!("have no client for from:{}", new_circuit.from);
        return false;
    }
    let empty_or_full_check = node.session_map.read().unwrap().get(&new_circuit.session_id).and_then(|session| {
        if session.pair_stream.early_stream.is_some() && session.pair_stream.later_stream.is_some() {
            info!("session is full for :{}", new_circuit.session_id);
            return Some(false);
        } else {
            return Some(true);
        }
    });
    if empty_or_full_check.is_none() || empty_or_full_check.unwrap() == false {
        return false;
    }
    let to_client_info_opt = {
        let guard = node.client_wn_map.read().unwrap();
        let client_opt = (*guard).get(&new_circuit.to);
        match client_opt {
            None => None,
            Some(x) => Some(x.clone())
        }
    };
    if to_client_info_opt.is_some() {
        info!("entry node is also exit node,session id:{}", new_circuit.session_id);
        let to_client_info = to_client_info_opt.unwrap();

        let wraped_stream_opt = node.new_session_to_peer(&to_client_info.peer_id, new_circuit.session_id.clone(), SessionRole::ExitRole as i32, SessionRole::AnswerRole as i32).await;
        if wraped_stream_opt.is_none() {
            info!("same entry and exit,new session to peer failed");
            return false;
        }
        let wraped_stream = wraped_stream_opt.unwrap();

        let circuit_success_relay = new_relay_circuit_success(&new_circuit.session_id);
        let new_session_opt = {
            let mut guard = node.session_map.write().unwrap();
            let session_opt = (*guard).get(&new_circuit.session_id);
            match session_opt {
                None => None,
                Some(x) => Some(x.clone())
            }
        };
        if new_session_opt.is_some() {
            let new_session = new_session_opt.unwrap();
            send_relay_twoway(&new_session, circuit_success_relay).await;
        }
        return true;
    }
    info!("i am entry node,session id:{}", new_circuit.session_id);

    let (sender, receiver) = futures::channel::oneshot::channel();
    let get_main_nets = GetMainNets { command_id: request.req_id.clone(), remote_peer_id: remote_peer_id, num: 100, sender: sender };
    node.node_request_sender.unbounded_send(NodeRequest::GetMainNetsRequest(get_main_nets));
    let nodeinfos_res = receiver.await;
    if nodeinfos_res.is_err() {
        info!("get other nets error");
        return false;
    }
    let nodeinfos = nodeinfos_res.unwrap();
    let mut invalid: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut try_join = false;
    let mut join = PeerId::random();

    for i in 0..3 {
        info!("try {} for connecto to other peer for joint role", i);
        let mut index = rand::random::<usize>();

        for j in 0..(nodeinfos.len()) {
            index = index + 1;
            index = index % nodeinfos.len();
            let id = nodeinfos.get(index).unwrap().id.clone();
            if !invalid.contains_key(&id) && id != node.get_id() && id != remote_peer_id.to_base58() {
                let remote_client_white_noise_id_hash = node.client_peer_map.read().unwrap().get(&id).and_then(|x| Some(x.clone()));
                if remote_client_white_noise_id_hash.is_none() || from_whitenoise_to_hash(remote_client_white_noise_id_hash.unwrap().as_str()) != new_circuit.to {
                    join = PeerId::from_bytes(bs58::decode(id).into_vec().unwrap().as_slice()).unwrap();
                    break;
                }
            }
        }
        let wraped_stream_opt = node.new_session_to_peer(&join, new_circuit.session_id.clone(), SessionRole::EntryRole as i32, SessionRole::JointRole as i32).await;
        if wraped_stream_opt.is_none() {
            invalid.insert(join.to_base58(), true);
        } else {
            try_join = true;
            break;
        }
    }

    if !try_join {
        info!("try three times to find joint but failed");
        node.handle_close_session(&new_circuit.session_id).await;
        return false;
    }

    info!("prepare to send neg infomation");
    let neg = gossip_proto::Negotiate { join: join.to_base58(), session_id: new_circuit.session_id.clone(), destination: new_circuit.to.clone(), sig: Vec::new() };
    let mut neg_data = Vec::new();
    neg.encode(&mut neg_data).unwrap();
    let neg_plain = request_proto::NegPlaintext {
        session_id: new_circuit.session_id.clone(),
        neg: neg_data,
    };
    let mut neg_plain_data = Vec::new();
    neg_plain.encode(&mut neg_plain_data).unwrap();
    let mut neg_request = request_proto::Request {
        req_id: String::from(""),
        from: node.get_id(),
        reqtype: request_proto::Reqtype::NegPlainText as i32,
        data: neg_plain_data,
    };
    let key = from_request_get_id(&neg_request);
    neg_request.req_id = key.clone();
    let node_request = NodeRequest::ProxyRequest(NodeProxyRequest { remote_peer_id: remote_peer_id, proxy_request: Some(ProxyRequest(neg_request)), peer_operation: None });
    let ack_request = node.external_send_node_request_and_wait(key, node_request).await;
    info!("receive neg information");
    let AckRequest(ack) = ack_request;
    let neg_cypher = ack.data;
    //publish
    info!("prepare to publish");
    let encrypted_neg = gossip_proto::EncryptedNeg { des: new_circuit.to, cypher: neg_cypher };
    let mut encrypted_neg_data = Vec::new();
    encrypted_neg.encode(&mut encrypted_neg_data).unwrap();
    let (sender, receiver) = futures::channel::oneshot::channel();
    let pr = PublishDataRequest { data: encrypted_neg_data, sender };
    let node_request = NodeRequest::PublishData(pr);
    node.node_request_sender.unbounded_send(node_request);
    let publish_res = receiver.await.unwrap();

    //probe
    info!("prepare to send probe");
    let probe_relay = new_relay_probe(new_circuit.session_id.as_str());

    let session_opt = node.session_map.read().unwrap().get(&new_circuit.session_id).and_then(|session| {
        return Some(session.clone());
    });
    if session_opt.is_none() {
        return false;
    }
    let session = session_opt.unwrap();
    send_relay_twoway(&session, probe_relay).await;

    return true;
}


