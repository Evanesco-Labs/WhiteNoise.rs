use super::protocols::relay_behaviour::WrappedStream;
use futures::{StreamExt, channel::mpsc::UnboundedReceiver, lock::Mutex};
use log::{info, error, warn, debug};
use snow::{TransportState};
use super::utils::{write_payload_arc};

#[derive(Clone)]
pub struct CircuitConn {
    pub id: String,
    pub out_stream: WrappedStream,
    pub in_channel_receiver: std::sync::Arc<Mutex<UnboundedReceiver<Vec<u8>>>>,
    pub in_channel_sender: futures::channel::mpsc::UnboundedSender<Vec<u8>>,
    pub transport_state: std::option::Option<std::sync::Arc<std::sync::Mutex<TransportState>>>,
}

impl CircuitConn {
    pub async fn read(&mut self, buf: &mut [u8]) -> Vec<u8> {
        if self.transport_state.is_none() {
            return Vec::new();
        }
        let data = self.in_channel_receiver.lock().await.next().await.unwrap();
        let buf_len = data[0] as usize * 256 + data[1] as usize;
        debug!("relay data len:{},real buf len:{}", data.len(), buf_len);

        let payload = data[2..(2 + buf_len)].to_vec();
        let len = self.transport_state.clone().unwrap().lock().unwrap().read_message(&payload, buf).unwrap();
        let real_size = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        debug!("real size:{},buf len:{}", real_size, len);
        return buf[4..(real_size + 4)].to_vec();
    }
    pub async fn write(&mut self, payload: &[u8], buf: &mut [u8]) {
        if self.transport_state.is_none() {
            return;
        }
        let len = self.transport_state.clone().unwrap().lock().unwrap().write_message(payload, buf).unwrap();
        let buf_tmp = &buf[..len];
        write_payload_arc(self.out_stream.clone(), buf_tmp, len, self.id.clone().as_ref()).await;
    }
}

