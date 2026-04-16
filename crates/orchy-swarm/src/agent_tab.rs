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
    scroll_offset: usize,
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
            let trim_start = 64 * 1024;
            let trim_at = self.raw_buf[trim_start..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|pos| trim_start + pos + 1)
                .unwrap_or(trim_start);
            self.raw_buf.drain(..trim_at);
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_up(&mut self, rows: u16, cols: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(10);
        self.maybe_rebuild_scroll(rows, cols);
        let max = self.max_scroll();
        self.scroll_offset = self.scroll_offset.min(max);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(10);
        if self.scroll_offset == 0 {
            self.scroll_cache = None;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.scroll_cache = None;
    }

    pub fn prepare_scroll(&mut self, rows: u16, cols: u16) {
        if self.scroll_offset > 0 {
            self.maybe_rebuild_scroll(rows, cols);
        }
    }

    /// Returns `(screen, start_row)` for rendering.
    pub fn scroll_view(&self) -> (&vt100::Screen, u16) {
        if self.scroll_offset == 0 {
            return (self.screen.screen(), 0);
        }
        if let Some(ref cache) = self.scroll_cache {
            let (total, _) = cache.parser.screen().size();
            let (vis, _) = self.screen.screen().size();
            let start = total
                .saturating_sub(vis)
                .saturating_sub(self.scroll_offset as u16);
            return (cache.parser.screen(), start);
        }
        (self.screen.screen(), 0)
    }

    fn max_scroll(&self) -> usize {
        self.scroll_cache
            .as_ref()
            .map(|c| {
                let (total, _) = c.parser.screen().size();
                let (vis, _) = self.screen.screen().size();
                total.saturating_sub(vis) as usize
            })
            .unwrap_or(0)
    }

    fn maybe_rebuild_scroll(&mut self, rows: u16, cols: u16) {
        if self.scroll_offset == 0 {
            self.scroll_cache = None;
            return;
        }
        let stale = self.scroll_cache.as_ref().map_or(true, |c| {
            c.offset < self.scroll_offset || c.buf_len != self.raw_buf.len()
        });
        if !stale {
            return;
        }
        let render_rows =
            (rows as usize + self.scroll_offset).min(rows as usize + 2000) as u16;
        let mut p = vt100::Parser::new(render_rows, cols, 0);
        p.process(&self.raw_buf);
        self.scroll_cache = Some(ScrollCache {
            offset: self.scroll_offset,
            buf_len: self.raw_buf.len(),
            parser: p,
        });
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.input_tx.send(bytes);
    }
}
