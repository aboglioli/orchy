use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use orchy_runner::error::Result;

pub struct AgentTab {
    pub id: usize,
    pub alias: String,
    #[allow(dead_code)]
    pub agent_id: String,
    #[allow(dead_code)]
    pub agent_type: String,
    pub is_idle: Arc<AtomicBool>,
    pub screen: vt100::Parser,
    pub input_tx: UnboundedSender<Vec<u8>>,
    pub driver_handle: JoinHandle<Result<()>>,
}

impl AgentTab {
    pub fn new(
        id: usize,
        alias: String,
        agent_id: String,
        agent_type: String,
        is_idle: Arc<AtomicBool>,
        rows: u16,
        cols: u16,
        input_tx: UnboundedSender<Vec<u8>>,
        driver_handle: JoinHandle<Result<()>>,
    ) -> Self {
        Self {
            id,
            alias,
            agent_id,
            agent_type,
            is_idle,
            screen: vt100::Parser::new(rows, cols, 1000),
            input_tx,
            driver_handle,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.is_idle.load(Ordering::Relaxed)
    }

    pub fn push_output(&mut self, bytes: Vec<u8>) {
        self.screen.process(&bytes);
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.input_tx.send(bytes);
    }
}
