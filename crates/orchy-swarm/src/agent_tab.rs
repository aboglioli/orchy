use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use orchy_runner::error::Result;

// Cached replay of raw_buf into a snapshot parser.
struct SnapshotCache {
    offset: usize,
    parser: vt100::Parser,
}

pub struct AgentTab {
    pub id: usize,
    pub alias: String,
    #[allow(dead_code)]
    pub agent_type: String,
    pub is_idle: Arc<AtomicBool>,
    pub screen: vt100::Parser,
    raw_buf: Vec<u8>,
    // Becomes true the first time the agent enters alternate-screen mode.
    // Used to select the scroll strategy:
    //   true  → TUI agent: snapshot-based scrollback
    //   false → streaming agent: vt100 built-in scrollback
    uses_alt_screen: bool,
    prev_idle: bool,
    // Snapshot-based scrollback for TUI agents.
    // Each entry is a raw_buf byte offset saved when the agent became idle
    // (response completed). Replaying raw_buf[..offset] reconstructs that state.
    snapshot_offsets: VecDeque<usize>,
    snapshot_cache: Option<SnapshotCache>,
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
        // Large built-in scrollback — used by streaming agents automatically
        // (lines that scroll off the top of the live screen accumulate here).
        const SCROLLBACK: usize = 5_000;
        Self {
            id,
            alias,
            agent_type,
            is_idle,
            screen: vt100::Parser::new(rows, cols, SCROLLBACK),
            raw_buf: Vec::new(),
            uses_alt_screen: false,
            prev_idle: false,
            snapshot_offsets: VecDeque::new(),
            snapshot_cache: None,
            input_tx,
            driver_handle,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.is_idle.load(Ordering::Relaxed)
    }

    pub fn push_output(&mut self, bytes: Vec<u8>) {
        self.screen.process(&bytes);

        if !self.uses_alt_screen && self.screen.screen().alternate_screen() {
            self.uses_alt_screen = true;
        }

        let now_idle = self.is_idle.load(Ordering::Relaxed);
        if now_idle && !self.prev_idle {
            self.raw_buf.extend_from_slice(&bytes);
            self.save_snapshot();
        } else {
            self.raw_buf.extend_from_slice(&bytes);
        }
        self.prev_idle = now_idle;

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
        self.snapshot_cache = None;
    }

    pub fn scroll_offset(&self) -> usize {
        if self.uses_alt_screen {
            self.snapshot_scroll_offset()
        } else {
            self.screen.screen().scrollback()
        }
    }

    pub fn scroll_up(&mut self, rows: u16, cols: u16) {
        if self.uses_alt_screen {
            let max = self.snapshot_offsets.len();
            let new = (self.snapshot_scroll_offset() + 1).min(max);
            self.set_snapshot_scroll(new, rows, cols);
        } else {
            let cur = self.screen.screen().scrollback();
            self.screen.set_scrollback(cur + 10);
        }
    }

    pub fn scroll_down(&mut self, rows: u16, cols: u16) {
        if self.uses_alt_screen {
            let cur = self.snapshot_scroll_offset();
            self.set_snapshot_scroll(cur.saturating_sub(1), rows, cols);
        } else {
            let cur = self.screen.screen().scrollback();
            self.screen.set_scrollback(cur.saturating_sub(10));
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.uses_alt_screen {
            self.snapshot_cache = None;
        } else {
            self.screen.set_scrollback(0);
        }
        // snapshot_scroll_offset is derived from snapshot_cache; clearing it
        // resets to 0. For streaming agents set_scrollback(0) resets it.
    }

    /// Must be called before `scroll_view` when `scroll_offset > 0`.
    pub fn prepare_scroll(&mut self, rows: u16, cols: u16) {
        if self.uses_alt_screen {
            // Ensure snapshot_cache is built for the current scroll position.
            let offset_idx = self.snapshot_scroll_offset();
            if offset_idx == 0 {
                self.snapshot_cache = None;
                return;
            }
            let idx = self.snapshot_offsets.len().saturating_sub(offset_idx);
            let offset = self.snapshot_offsets[idx];
            if self.snapshot_cache.as_ref().map(|c| c.offset) != Some(offset) {
                let mut p = vt100::Parser::new(rows, cols, 0);
                p.process(&self.raw_buf[..offset]);
                self.snapshot_cache = Some(SnapshotCache { offset, parser: p });
            }
        }
        // Streaming agents: vt100 scrollback is live in `self.screen`, no prep needed.
    }

    /// Returns `(screen, start_row)` for rendering.
    pub fn scroll_view(&self) -> (&vt100::Screen, u16) {
        if self.uses_alt_screen {
            if let Some(ref cache) = self.snapshot_cache {
                return (cache.parser.screen(), 0);
            }
        }
        // Streaming agents: set_scrollback already adjusts visible_rows in the screen.
        (self.screen.screen(), 0)
    }

    // -- snapshot helpers --

    fn snapshot_scroll_offset(&self) -> usize {
        self.snapshot_cache.as_ref().map_or(0, |c| {
            self.snapshot_offsets
                .iter()
                .rposition(|&off| off == c.offset)
                .map(|idx| self.snapshot_offsets.len() - idx)
                .unwrap_or(0)
        })
    }

    fn set_snapshot_scroll(&mut self, offset_idx: usize, rows: u16, cols: u16) {
        if offset_idx == 0 {
            self.snapshot_cache = None;
            return;
        }
        let idx = self.snapshot_offsets.len().saturating_sub(offset_idx);
        let offset = self.snapshot_offsets[idx];
        if self.snapshot_cache.as_ref().map(|c| c.offset) != Some(offset) {
            let mut p = vt100::Parser::new(rows, cols, 0);
            p.process(&self.raw_buf[..offset]);
            self.snapshot_cache = Some(SnapshotCache { offset, parser: p });
        }
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.input_tx.send(bytes);
    }
}
