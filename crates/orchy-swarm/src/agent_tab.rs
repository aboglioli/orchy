use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use orchy_runner::error::Result;

struct ScrollCache {
    offset: usize,
    buf_len: usize,
    parser: vt100::Parser,
}

pub struct AgentTab {
    pub id: usize,
    pub alias: String,
    #[allow(dead_code)]
    pub agent_type: String,
    pub is_idle: Arc<AtomicBool>,
    pub screen: vt100::Parser,
    pub scroll_offset: usize,
    raw_buf: Vec<u8>,
    scroll_cache: Option<ScrollCache>,
    pub input_tx: UnboundedSender<Vec<u8>>,
    pub driver_handle: JoinHandle<Result<()>>,
}

impl AgentTab {
    pub fn new(
        id: usize,
        alias: String,
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
            agent_type,
            is_idle,
            screen: vt100::Parser::new(rows, cols, 0),
            scroll_offset: 0,
            raw_buf: Vec::new(),
            scroll_cache: None,
            input_tx,
            driver_handle,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.is_idle.load(Ordering::Relaxed)
    }

    pub fn push_output(&mut self, bytes: Vec<u8>) {
        self.screen.process(&bytes);
        self.raw_buf.extend_from_slice(&bytes);
        const MAX: usize = 512 * 1024;
        if self.raw_buf.len() > MAX {
            self.raw_buf.drain(..64 * 1024);
        }
    }

    pub fn max_scroll(&self) -> usize {
        self.scroll_cache
            .as_ref()
            .map(|c| {
                let (rows, _) = c.parser.screen().size();
                let (vis_rows, _) = self.screen.screen().size();
                rows.saturating_sub(vis_rows) as usize
            })
            .unwrap_or(0)
    }

    /// Rebuild scroll cache when offset or raw buf content changed.
    /// Call this before rendering whenever scroll_offset > 0.
    pub fn maybe_rebuild_scroll(&mut self, rows: u16, cols: u16) {
        if self.scroll_offset == 0 {
            self.scroll_cache = None;
            return;
        }
        let stale = self.scroll_cache.as_ref().map_or(true, |c| {
            c.offset != self.scroll_offset || c.buf_len != self.raw_buf.len()
        });
        if !stale {
            return;
        }
        let render_rows = (rows as usize + self.scroll_offset).min(rows as usize + 2000) as u16;
        let mut p = vt100::Parser::new(render_rows, cols, 0);
        p.process(&self.raw_buf);
        self.scroll_cache = Some(ScrollCache {
            offset: self.scroll_offset,
            buf_len: self.raw_buf.len(),
            parser: p,
        });
    }

    pub fn scroll_screen(&self) -> Option<&vt100::Screen> {
        self.scroll_cache.as_ref().map(|c| c.parser.screen())
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.input_tx.send(bytes);
    }
}
