use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use tokio::sync::mpsc::UnboundedSender;

pub struct AgentSession {
    pub alias: String,
    pub agent_id: String,
    pub agent_type: String,
    pub is_idle: Arc<AtomicBool>,
    pub last_output_ms: Arc<AtomicU64>,
    pub output_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    pub input_tx: UnboundedSender<Vec<u8>>,
}
