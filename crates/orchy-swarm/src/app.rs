use std::collections::HashMap;
use std::io::Stdout;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use orchy_runner::config::RunnerConfig;
use orchy_runner::driver::AgentDriver;

use crate::agent_tab::AgentTab;
use crate::ui::modal::{AgentTypeOption, ModalState};

pub struct App {
    pub tabs: Vec<AgentTab>,
    pub active_tab: usize,
    pub modal: Option<ModalState>,
    pub orchy_url: String,
    pub project: String,
    running: bool,
}

impl App {
    pub fn new(orchy_url: String, project: String) -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: 0,
            modal: None,
            orchy_url,
            project,
            running: true,
        }
    }

    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> anyhow::Result<()> {
        let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<(usize, Vec<u8>)>();

        while self.running {
            while let Ok((idx, bytes)) = ev_rx.try_recv() {
                if let Some(tab) = self.tabs.get_mut(idx) {
                    tab.push_output(bytes);
                }
            }

            terminal.draw(|f| crate::ui::render(f, self))?;

            if event::poll(Duration::from_millis(16))?
                && let Event::Key(key) = event::read()?
            {
                self.handle_key(key, &ev_tx).await?;
            }
        }

        for tab in &self.tabs {
            let _ = tab.input_tx.send(b"/exit\r".to_vec());
        }

        Ok(())
    }

    async fn handle_key(
        &mut self,
        key: KeyEvent,
        ev_tx: &mpsc::UnboundedSender<(usize, Vec<u8>)>,
    ) -> anyhow::Result<()> {
        if let Some(modal) = &mut self.modal {
            match key.code {
                KeyCode::Esc => self.modal = None,
                KeyCode::Enter => {
                    if let Some(selected) = modal.selected_agent() {
                        if selected.installed {
                            let selected = selected.clone();
                            let alias = selected.agent_type.to_string();
                            self.modal = None;
                            self.launch_agent(alias, selected, ev_tx).await?;
                        }
                    }
                }
                KeyCode::Up => modal.move_up(),
                KeyCode::Down => modal.move_down(),
                KeyCode::Char(c) => {
                    modal.filter.push(c);
                    // keep selection valid after filter change
                    if modal.selected_agent().is_none() {
                        if let Some((i, _)) = modal.visible().first() {
                            modal.selected = *i;
                        }
                    }
                }
                KeyCode::Backspace => {
                    modal.filter.pop();
                }
                _ => {}
            }
            return Ok(());
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c'))
            | (KeyModifiers::CONTROL, KeyCode::Char('q')) => {
                self.running = false;
            }
            (_, KeyCode::F(2)) => {
                self.modal = Some(ModalState::new());
            }
            (_, KeyCode::F(4)) => {
                self.kill_active_tab();
            }
            (KeyModifiers::ALT, KeyCode::Char(c)) if c.is_ascii_digit() => {
                let idx = (c as usize).saturating_sub('1' as usize);
                if idx < self.tabs.len() {
                    self.active_tab = idx;
                }
            }
            (KeyModifiers::ALT, KeyCode::Right) | (_, KeyCode::F(6)) => {
                if !self.tabs.is_empty() {
                    self.active_tab = (self.active_tab + 1) % self.tabs.len();
                }
            }
            (KeyModifiers::ALT, KeyCode::Left) | (_, KeyCode::F(5)) => {
                if !self.tabs.is_empty() {
                    self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
                }
            }
            _ => {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    let bytes = key_event_to_bytes(key);
                    if !bytes.is_empty() {
                        tab.send_input(bytes);
                    }
                }
            }
        }

        Ok(())
    }

    async fn launch_agent(
        &mut self,
        alias: String,
        agent_type_opt: AgentTypeOption,
        ev_tx: &mpsc::UnboundedSender<(usize, Vec<u8>)>,
    ) -> anyhow::Result<()> {
        let mut env = HashMap::new();
        env.insert(
            "TERM".to_string(),
            std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()),
        );

        let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 24));
        let pty_rows = term_rows.saturating_sub(4).max(10);
        let pty_cols = term_cols;

        let config = RunnerConfig {
            alias: alias.clone(),
            agent_type: agent_type_opt.agent_type.to_string(),
            description: format!("{} agent", alias),
            url: self.orchy_url.clone(),
            project: self.project.clone(),
            namespace: None,
            roles: Vec::new(),
            command: agent_type_opt.command.to_string(),
            args: Vec::new(),
            env,
            working_dir: None,
            pty_rows,
            pty_cols,
            idle_patterns: orchy_runner::config::default_idle_patterns_for(
                agent_type_opt.agent_type,
            ),
            idle_wake: Duration::from_secs(120),
            heartbeat_interval: Duration::from_secs(30),
        };

        let tab_idx = self.tabs.len();
        let ev_tx_clone = ev_tx.clone();

        let (mut session, handle) = AgentDriver::start(config)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        tokio::spawn(async move {
            while let Some(bytes) = session.output_rx.recv().await {
                if ev_tx_clone.send((tab_idx, bytes)).is_err() {
                    break;
                }
            }
        });

        self.tabs.push(AgentTab::new(
            alias,
            session.agent_id,
            agent_type_opt.agent_type.to_string(),
            session.is_idle,
            pty_rows,
            pty_cols,
            session.input_tx,
            handle,
        ));

        self.active_tab = self.tabs.len() - 1;
        Ok(())
    }

    fn kill_active_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let tab = self.tabs.remove(self.active_tab);
        let _ = tab.input_tx.send(b"/exit\r".to_vec());
        tab.driver_handle.abort();
        if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
            self.active_tab = self.tabs.len() - 1;
        }
    }
}

fn key_event_to_bytes(key: KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let b = c as u8;
                if b.is_ascii_alphabetic() {
                    vec![b.to_ascii_lowercase() - b'a' + 1]
                } else {
                    vec![]
                }
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::F(n) => match n {
            1 => vec![0x1b, b'O', b'P'],
            2 => vec![0x1b, b'O', b'Q'],
            3 => vec![0x1b, b'O', b'R'],
            4 => vec![0x1b, b'O', b'S'],
            5 => vec![0x1b, b'[', b'1', b'5', b'~'],
            6 => vec![0x1b, b'[', b'1', b'7', b'~'],
            7 => vec![0x1b, b'[', b'1', b'8', b'~'],
            8 => vec![0x1b, b'[', b'1', b'9', b'~'],
            _ => vec![],
        },
        _ => vec![],
    }
}
