use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use orchy_runner::error::Result;

// Used for streaming agents (no full-screen clears): replay raw_buf into a
// taller parser to expose rows that would otherwise have scrolled off the top.
struct TallerParser {
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
    // Snapshot-based scrollback: raw_buf byte offsets captured just before each
    // full-screen clear (\x1b[2J). Stepping through these shows previous TUI states.
    snapshot_offsets: VecDeque<usize>,
    snapshot_cache: Option<(usize, vt100::Parser)>,
    // Fallback for streaming agents that never clear the screen.
    taller_parser: Option<TallerParser>,
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
            snapshot_offsets: VecDeque::new(),
            snapshot_cache: None,
            taller_parser: None,
            input_tx,
            driver_handle,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.is_idle.load(Ordering::Relaxed)
    }

    pub fn push_output(&mut self, bytes: Vec<u8>) {
        if contains_full_clear(&bytes) {
            self.save_snapshot();
        }
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

            self.snapshot_offsets.retain(|&off| off > trim_at);
            for off in self.snapshot_offsets.iter_mut() {
                *off -= trim_at;
            }
            self.snapshot_cache = None;
            self.taller_parser = None;
            self.scroll_offset = self.scroll_offset.min(self.max_scroll_unclamped());
            if self.scroll_offset == 0 {
                self.snapshot_cache = None;
            }
        }
    }

    fn save_snapshot(&mut self) {
        let offset = self.raw_buf.len();
        if self.snapshot_offsets.back() == Some(&offset) {
            return;
        }
        self.snapshot_offsets.push_back(offset);
        const MAX_SNAPSHOTS: usize = 100;
        if self.snapshot_offsets.len() > MAX_SNAPSHOTS {
            self.snapshot_offsets.pop_front();
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn snapshot_count(&self) -> usize {
        self.snapshot_offsets.len()
    }

    pub fn is_snapshot_mode(&self) -> bool {
        self.scroll_offset > 0 && !self.snapshot_offsets.is_empty()
    }

    pub fn scroll_up(&mut self, rows: u16, cols: u16) {
        let step = if self.snapshot_offsets.is_empty() { 10 } else { 1 };
        self.scroll_offset = self.scroll_offset.saturating_add(step);
        self.prepare_scroll(rows, cols);
        let max = self.max_scroll_unclamped();
        self.scroll_offset = self.scroll_offset.min(max);
    }

    pub fn scroll_down(&mut self) {
        let step = if self.snapshot_offsets.is_empty() { 10 } else { 1 };
        self.scroll_offset = self.scroll_offset.saturating_sub(step);
        if self.scroll_offset == 0 {
            self.snapshot_cache = None;
            self.taller_parser = None;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.snapshot_cache = None;
        self.taller_parser = None;
    }

    /// Must be called before `scroll_view` when `scroll_offset > 0`.
    pub fn prepare_scroll(&mut self, rows: u16, cols: u16) {
        if self.scroll_offset == 0 {
            return;
        }
        if !self.snapshot_offsets.is_empty() {
            let idx = self.snapshot_offsets
                .len()
                .saturating_sub(self.scroll_offset);
            let offset = self.snapshot_offsets[idx];
            if self.snapshot_cache.as_ref().map(|(o, _)| *o) != Some(offset) {
                let mut p = vt100::Parser::new(rows, cols, 0);
                p.process(&self.raw_buf[..offset]);
                self.snapshot_cache = Some((offset, p));
            }
        } else {
            self.maybe_rebuild_taller(rows, cols);
        }
    }

    /// Returns `(screen, start_row)` for rendering. Call `prepare_scroll` first.
    pub fn scroll_view(&self) -> (&vt100::Screen, u16) {
        if self.scroll_offset == 0 {
            return (self.screen.screen(), 0);
        }
        if !self.snapshot_offsets.is_empty() {
            if let Some((_, ref parser)) = self.snapshot_cache {
                return (parser.screen(), 0);
            }
        } else if let Some(ref tp) = self.taller_parser {
            let (total, _) = tp.parser.screen().size();
            let (vis, _) = self.screen.screen().size();
            let start = total
                .saturating_sub(vis)
                .saturating_sub(self.scroll_offset as u16);
            return (tp.parser.screen(), start);
        }
        (self.screen.screen(), 0)
    }

    fn max_scroll_unclamped(&self) -> usize {
        if !self.snapshot_offsets.is_empty() {
            return self.snapshot_offsets.len();
        }
        self.taller_parser
            .as_ref()
            .map(|tp| {
                let (total, _) = tp.parser.screen().size();
                let (vis, _) = self.screen.screen().size();
                total.saturating_sub(vis) as usize
            })
            .unwrap_or(0)
    }

    fn maybe_rebuild_taller(&mut self, rows: u16, cols: u16) {
        if self.scroll_offset == 0 {
            self.taller_parser = None;
            return;
        }
        let stale = self.taller_parser.as_ref().map_or(true, |tp| {
            tp.offset < self.scroll_offset || tp.buf_len != self.raw_buf.len()
        });
        if !stale {
            return;
        }
        let render_rows =
            (rows as usize + self.scroll_offset).min(rows as usize + 2000) as u16;
        let mut p = vt100::Parser::new(render_rows, cols, 0);
        p.process(&self.raw_buf);
        self.taller_parser = Some(TallerParser {
            offset: self.scroll_offset,
            buf_len: self.raw_buf.len(),
            parser: p,
        });
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.input_tx.send(bytes);
    }
}

fn contains_full_clear(bytes: &[u8]) -> bool {
    // \x1b[2J — erase entire display (most common full-screen clear)
    bytes.windows(4).any(|w| w == b"\x1b[2J")
}
