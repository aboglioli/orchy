use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use orchy_runner::error::Result;

pub struct AgentTab {
    pub alias: String,
    #[allow(dead_code)]
    pub agent_id: String,
    #[allow(dead_code)]
    pub agent_type: String,
    pub is_idle: Arc<AtomicBool>,
    pub output_buf: VecDeque<Vec<u8>>,
    pub scroll_offset: usize,
    pub input_tx: UnboundedSender<Vec<u8>>,
    pub driver_handle: JoinHandle<Result<()>>,
}

impl AgentTab {
    pub fn is_idle(&self) -> bool {
        self.is_idle.load(Ordering::Relaxed)
    }

    pub fn push_output(&mut self, bytes: Vec<u8>) {
        self.output_buf.push_back(bytes);
        while self.output_buf.len() > 10_000 {
            self.output_buf.pop_front();
        }
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.input_tx.send(bytes);
    }

    pub fn all_output(&self) -> Vec<u8> {
        self.output_buf.iter().flatten().copied().collect()
    }
}
