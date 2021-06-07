use libp2p::{noise};
use crate::{command_proto, relay_proto};
use prost::Message;
use tokio::sync::mpsc;
use log::{info, debug, error};
use super::{protocols::relay_behaviour::WrappedStream};
use super::protocols::ack_protocol::{AckRequest};
use snow::{params::NoiseParams, Builder, HandshakeState};
use super::whitenoise_behaviour::{NodeRequest, NodeAckRequest};
use super::node::{self, Node};
use super::utils::{read_from_negotiated_arc, write_relay_arc,
                   write_handshake_payload_arc, generate_handshake_payload,
                   handle_remote_handshake_payload, write_disconnect_arc,
                   forward_relay, send_relay_twoway, new_relay_circuit_success};
use super::session::{Session, SessionRole, PairStream};
use super::connection::CircuitConn;

pub async fn relay_event_handler(mut stream: WrappedStream, mut node: Node, mut session_id: std::option::Option<String>) {
    loop {
        let read_relay_option = read_from_negotiated_arc(stream.clone()).await;
        if read_relay_option.is_err() {
            error!("relay stream error");
            if session_id.is_some() {
                let cur_stream_id = stream.stream_id.clone();
                let find_stream = {
                    let mut find_stream = false;
                    let guard = node.session_map.read().unwrap();
                    let session_opt = (*guard).get(&session_id.clone().unwrap());
                    if session_opt.is_some() {
                        let session = session_opt.unwrap();
                        if session.pair_stream.early_stream.is_some() {
                            if session.pair_stream.early_stream.as_ref().unwrap().stream_id == cur_stream_id {
                                find_stream = true;
                            }
                        }
                        if session.pair_stream.later_stream.is_some() {
                            if session.pair_stream.later_stream.as_ref().unwrap().stream_id == cur_stream_id {
                                find_stream = true;
                            }
                        }
                    }
                    find_stream
                };
                if find_stream {
                    info!("relay stream error and find stream in session so we close all session");
                    node.handle_close_session(session_id.as_ref().unwrap()).await;
                } else {
                    info!("session'stream not include this stream,which is replaced,and relay stram read is already closed,need no more work");
                }
            }
            break;
        }
        let read_relay = read_relay_option.unwrap();
        if read_relay.r#type == (relay_proto::Relaytype::Wake as i32) {
            debug!("receive wake msg");
        } else if read_relay.r#type == (relay_proto::Relaytype::Ack as i32) {
            debug!("receive ack msg");
        } else if read_relay.r#type == (relay_proto::Relaytype::Probe as i32) {
            handle_relay_probe(node.clone(), session_id.clone().unwrap(), stream.clone(), read_relay).await;
        } else if read_relay.r#type == (relay_proto::Relaytype::Disconnect as i32) {
            debug!("receive relay disconnect");
            let disconnect_result = relay_proto::Disconnect::decode(read_relay.data.as_slice());
            if disconnect_result.is_ok() {
                let disconnect = disconnect_result.unwrap();
                node.handle_close_session(&disconnect.session_id).await;
            }
            break;
        } else if read_relay.r#type == (relay_proto::Relaytype::SetSessionId as i32) {
            debug!("receive set session id");
            let relay_set_session = relay_proto::SetSessionIdMsg::decode(read_relay.data.as_slice()).unwrap();
            if session_id.is_none() {
                session_id = Some(relay_set_session.session_id.clone());
            }
            if relay_set_session.role == (SessionRole::AnswerRole as i32) {
                add_circuit_conn(node.clone(), session_id.clone().unwrap(), stream.clone()).await;
            }
            handle_set_session(node.clone(), relay_set_session, stream.clone(), read_relay.id).await;
        } else if read_relay.r#type == (relay_proto::Relaytype::Success as i32) {
            info!("relay event handler receive set success");
            handle_success(node.clone(), read_relay, stream.clone()).await;
        } else if read_relay.r#type == (relay_proto::Relaytype::Data as i32) {
            handle_relay_msg(node.clone(), read_relay, stream.clone()).await;
        }
    }
}

pub async fn handle_relay_probe(mut node: Node, session_id: String, stream: WrappedStream, relay: relay_proto::Relay) {
    let probe = relay_proto::ProbeSignal::decode(relay.data.as_slice()).unwrap();
    let session = node.session_map.read().unwrap().get(&session_id).and_then(|session| {
        Some(session.clone())
    });
    if session.is_none() {
        return;
    }
    if session.is_some() && session.as_ref().unwrap().session_role == (SessionRole::JointRole as i32) {
        info!("i am joint node,session id:{}", session_id);
        if node.probe_map.read().unwrap().contains_key(&session_id) {
            info!("i contains probe");
            let exist_probe = node.probe_map.read().unwrap().get(&session_id).and_then(|x| {
                Some(x.clone())
            });
            if exist_probe.unwrap().rand == probe.data {
                info!("exist probe and probe equals");
                let circuit_success_relay = new_relay_circuit_success(&session_id);
                if session.is_some() {
                    let new_session = session.unwrap();
                    send_relay_twoway(&new_session, circuit_success_relay).await;
                }
            } else {
                info!("exist probe and probe not equals");
                node.handle_close_session(&session_id).await;
            }
        } else {
            info!("i donnot contains probe");
            let session_probe = crate::network::node::Probe { session_id: session_id.clone(), rand: probe.data.clone() };
            node.probe_map.write().unwrap().insert(session_id.clone(), session_probe);
        }
        return;
    }

    if session.as_ref().unwrap().ready() {
        forward_relay(session.as_ref().unwrap(), stream.stream_id.as_str(), relay).await;
    } else {
        debug!("relay stream not ready");
    }
}

pub async fn handle_success(mut node: Node, relay: relay_proto::Relay, stream: WrappedStream) {
    info!("relay event handler start handle success");
    let relay_success_rst = relay_proto::CircuitSuccess::decode(relay.data.as_slice());
    if relay_success_rst.is_err() {
        return;
    }
    let relay_success = relay_success_rst.unwrap();
    let session = {
        let guard = node.session_map.read().unwrap();
        let session_inner = (*guard).get(&relay_success.session_id);
        match session_inner {
            None => None,
            Some(x) => Some(x.clone())
        }
    };
    if session.is_none() {
        return;
    }
    if session.as_ref().unwrap().session_role == (SessionRole::CallerRole as i32) {
        info!("handle success for caller");
        let circuit_conn = {
            let guard = node.circuit_map.read().unwrap();
            let cc = (*guard).get(&relay_success.session_id).unwrap();
            cc.clone()
        };
        tokio::spawn(process_handshake(circuit_conn, true, relay_success.session_id.clone(), node.clone()));
        return;
    }
    if session.as_ref().unwrap().session_role == (SessionRole::AnswerRole as i32) {
        let circuit_conn = {
            let guard = node.circuit_map.read().unwrap();
            let cc = (*guard).get(&relay_success.session_id).unwrap();
            cc.clone()
        };
        tokio::spawn(process_handshake(circuit_conn, false, relay_success.session_id.clone(), node.clone()));
        return;
    } else {
        debug!("relay event handler start handle success,not caller,not not answer");
        if session.as_ref().unwrap().ready() {
            forward_relay(session.as_ref().unwrap(), stream.stream_id.as_str(), relay).await;
        } else {
            debug!("relay stream not ready");
        }
    }
}

pub async fn process_handshake(mut circuit_conn: CircuitConn, initiate: bool, session_id: String, node: Node) {
    info!("start handshake");
    //caller
    if initiate {
        info!("start handshake initiate");
        let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&node.keypair)
            .expect("Signing libp2p-noise static DH keypair failed.");

        let bb = noise_keys.public().as_ref();
        info!("noise pub key:{}", bs58::encode(bb).into_string());
        let msg = ["noise-libp2p-static-key:".as_bytes(), bb].concat();

        let params: NoiseParams = "Noise_XX_25519_ChaChaPoly_SHA256".parse().unwrap();

        let mut buf = vec![0u8; 65535];
        // Initialize our responder using a builder.
        let builder: Builder<'_> = Builder::new(params.clone());
        let mut noise =
            builder.local_private_key(&noise_keys.secret().as_ref()).build_initiator().unwrap();
        //write nil
        let len = noise.write_message(&[], &mut buf).unwrap();
        write_handshake_payload_arc(circuit_conn.out_stream.clone(), &buf[..len], len, &session_id).await;
        info!("write nil");
        // <- s, se
        let len = handle_read_shake(circuit_conn.in_channel_receiver.lock().await.recv().await.unwrap(), &mut noise, &mut buf);
        //     //verify 
        info!("read identity");
        let pubkey = noise.get_remote_static().unwrap();
        let verify_success = handle_remote_handshake_payload(&buf[..len], pubkey);
        info!("verify success:{}", verify_success);
        // -> e, ee, s, es
        let payload = generate_handshake_payload(noise_keys.into_identity()).await;
        let len = noise.write_message(&payload, &mut buf).unwrap();
        write_handshake_payload_arc(circuit_conn.out_stream.clone(), &buf[..len], len, &session_id).await;

        let mut noise = noise.into_transport_mode().unwrap();

        circuit_conn.transport_state = Some(std::sync::Arc::new(tokio::sync::Mutex::new(noise)));
        {
            let mut guard = node.circuit_map.write().unwrap();
            (*guard).insert(session_id.clone(), circuit_conn);
        }
        info!("Build circuit sucess!");
    } else {
        let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&node.keypair)
            .expect("Signing libp2p-noise static DH keypair failed.");

        let bb = noise_keys.public().as_ref();
        info!("noise pub key:{}", bs58::encode(bb).into_string());
        let msg = ["noise-libp2p-static-key:".as_bytes(), bb].concat();
        //
        let pp = noise_keys.clone().into_identity().public;
        let verify_succes = pp.verify(&msg, &noise_keys.clone().into_identity().signature.unwrap());
        info!("self verify success:{}", verify_succes);
        //let noise_config = noise::NoiseConfig::xx(noise_keys);


        let params: NoiseParams = "Noise_XX_25519_ChaChaPoly_SHA256".parse().unwrap();
        let mut buf = vec![0u8; 65535];
        // Initialize our responder using a builder.
        let builder: Builder<'_> = Builder::new(params.clone());
        //let static_key = builder.generate_keypair().unwrap().private;
        let mut noise =
            builder.local_private_key(&noise_keys.secret().as_ref()).build_responder().unwrap();
        // <- e
        //crate::network::utils::read_handshake_payload_arc(stream.clone(), &mut noise, &mut buf).await;
        let len = handle_read_shake(circuit_conn.in_channel_receiver.lock().await.recv().await.unwrap(), &mut noise, &mut buf);

        // -> e, ee, s, es
        let payload = generate_handshake_payload(noise_keys.into_identity()).await;
        let len = noise.write_message(&payload, &mut buf).unwrap();
        //crate::network::utils::write_handshake_payload_arc(stream.clone(), &buf[..len],len,&session_id).await;
        write_handshake_payload_arc(circuit_conn.out_stream.clone(), &buf[..len], len, &session_id).await;

        // <- s, se
        //let len = crate::network::utils::read_handshake_payload_arc(stream.clone(), &mut noise, &mut buf).await;
        let len = handle_read_shake(circuit_conn.in_channel_receiver.lock().await.recv().await.unwrap(), &mut noise, &mut buf);
        //verify 

        let pubkey = noise.get_remote_static().unwrap();
        let verify_success = crate::network::utils::handle_remote_handshake_payload(&buf[..len], pubkey);
        info!("verify success:{}", verify_success);

        let mut noise = noise.into_transport_mode().unwrap();
        circuit_conn.transport_state = Some(std::sync::Arc::new(tokio::sync::Mutex::new(noise)));
        {
            let mut guard = node.circuit_map.write().unwrap();
            (*guard).insert(session_id.clone(), circuit_conn);
        }
        info!("Build circuit sucess!");
    }
}

pub fn handle_read_shake(data: Vec<u8>, noise: &mut HandshakeState, buf: &mut Vec<u8>) -> usize {
    let buf_len = data[0] as usize * 256 + data[1] as usize;
    debug!("relay data len:{},real buf len:{}", data.len(), buf_len);
    let payload = data[2..(2 + buf_len)].to_vec();
    return noise.read_message(&payload, buf).unwrap();
}

pub async fn handle_relay_msg(mut node: Node, relay: relay_proto::Relay, stream: WrappedStream) {
    //info!("receive relay msg,peer_id:{:?}",stream.remote_peer_id);
    let relay_msg_rst = relay_proto::RelayMsg::decode(relay.data.as_slice());
    if relay_msg_rst.is_err() {
        return;
    }
    let relay_msg = relay_msg_rst.unwrap();

    let session = {
        let guard = node.session_map.read().unwrap();
        let session_inner = (*guard).get(&relay_msg.session_id);
        match session_inner {
            None => None,
            Some(x) => Some(x.clone())
        }
    };
    if session.is_none() {
        info!("receive relay msg,peer_id:{:?},session is null", stream.remote_peer_id);
        return;
    }
    if session.as_ref().unwrap().session_role == (SessionRole::AnswerRole as i32) || session.as_ref().unwrap().session_role == (SessionRole::CallerRole as i32) {
        let guard = node.circuit_map.read().unwrap();
        let circuit_conn_opt = (*guard).get(&relay_msg.session_id);
        if circuit_conn_opt.is_some() {
            let circuit_conn = circuit_conn_opt.unwrap();
            circuit_conn.in_channel_sender.send(relay_msg.data);
        }
    } else {
        if session.as_ref().unwrap().ready() {
            forward_relay(session.as_ref().unwrap(), stream.stream_id.as_str(), relay).await;
        } else {
            debug!("relay stream not ready");
        }
    }
}

pub async fn handle_set_session(mut node: Node, relay_set_session: relay_proto::SetSessionIdMsg, stream: WrappedStream, relay_id: String) {
    debug!("start to process set session id:{},stream remote:{}", relay_set_session.session_id, stream.remote_peer_id.to_base58());
    let mut session = {
        let guard = node.session_map.write().unwrap();
        let session_option = (*guard).get(&relay_set_session.session_id);
        match session_option {
            None => None,
            Some(x) => Some(x.clone())
        }
    };
    if session.is_some() && session.as_ref().unwrap().session_role == (SessionRole::EntryRole as i32) && relay_set_session.role == (SessionRole::RelayRole as i32) {
        info!("start to process set session id,entry and relay are the same");
        if session.as_ref().unwrap().ready() {
            info!("start to process set session id,entry and relay are the same,session is ready");
            info!("start to process set session id,entry and relay are the same,replace later stream");
            let mut new_session = session.clone().unwrap();
            new_session.pair_stream.later_stream = Some(stream.clone());
            node.session_map.write().unwrap().insert(relay_set_session.session_id.clone(), new_session);
            info!("start to process set session id,entry and relay are the same,close early later stream");
            let mut to_close_stream = session.unwrap().pair_stream.later_stream.unwrap();
            write_disconnect_arc(to_close_stream.clone(), relay_set_session.session_id.clone()).await;
            to_close_stream.close().await;
            debug!("start to process set session id,entry and relay are the same,send set session ack:{}", stream.remote_peer_id);
            let ack = command_proto::Ack { command_id: relay_id.clone(), result: true, data: Vec::new() };
            let node_ack_request = NodeAckRequest { remote_peer_id: stream.remote_peer_id.clone(), ack_request: Some(AckRequest(ack)) };
            let ack_request = NodeRequest::AckRequest(node_ack_request);
            node.send_ack(ack_request).await;
            return;
        } else {
            node.handle_close_session(&relay_set_session.session_id).await;
            return;
        }
    }
    if session.is_none() {
        debug!("start to process set session id,new session");
        let pair_stream = PairStream { early_stream: Some(stream.clone()), later_stream: None };
        let session = Session { id: relay_set_session.session_id.clone(), session_role: relay_set_session.role, pair_stream: pair_stream };
        let mut guard = node.session_map.write().unwrap();
        (*guard).insert(relay_set_session.session_id.clone(), session);
    } else {
        debug!("start to process set session id,new later stream");
        let mut guard = node.session_map.write().unwrap();
        let mut session = (*guard).get(&relay_set_session.session_id).unwrap().clone();
        session.pair_stream.later_stream = Some(stream.clone());
        (*guard).insert(relay_set_session.session_id.clone(), session);
    }
    debug!("start to process set session id,start to send ack");
    let ack = command_proto::Ack { command_id: relay_id.clone(), result: true, data: Vec::new() };
    let node_ack_request = NodeAckRequest { remote_peer_id: stream.remote_peer_id.clone(), ack_request: Some(AckRequest(ack)) };
    let ack_request = NodeRequest::AckRequest(node_ack_request);
    node.send_ack(ack_request).await;
}

pub async fn add_circuit_conn(node: Node, session_id: String, stream: WrappedStream) {
    let mut guard = node.circuit_map.write().unwrap();
    if !(*guard).contains_key(&session_id) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let circuit_conn = CircuitConn {
            id: session_id.clone(),
            out_stream: stream.clone(),
            in_channel_receiver: std::sync::Arc::new(tokio::sync::Mutex::new(receiver)),
            in_channel_sender: sender,
            transport_state: None,
        };
        (*guard).insert(session_id.clone(), circuit_conn);
    }
}