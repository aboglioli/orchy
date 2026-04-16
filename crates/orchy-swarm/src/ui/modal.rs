use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub struct AgentTypeOption {
    pub name: &'static str,
    pub agent_type: &'static str,
    pub command: &'static str,
    pub installed: bool,
}

impl Clone for AgentTypeOption {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            agent_type: self.agent_type,
            command: self.command,
            installed: self.installed,
        }
    }
}

pub struct ModalState {
    pub filter: String,
    pub agent_types: Vec<AgentTypeOption>,
    pub selected: usize,
}

impl ModalState {
    pub fn new() -> Self {
        let agent_types = detect_installed_agents();
        let selected = agent_types.iter().position(|a| a.installed).unwrap_or(0);
        Self {
            filter: String::new(),
            agent_types,
            selected,
        }
    }

    pub fn visible(&self) -> Vec<(usize, &AgentTypeOption)> {
        let f = self.filter.to_lowercase();
        self.agent_types
            .iter()
            .enumerate()
            .filter(|(_, a)| f.is_empty() || a.name.to_lowercase().contains(&f))
            .collect()
    }

    pub fn selected_visible_idx(&self) -> usize {
        let visible = self.visible();
        visible
            .iter()
            .position(|(i, _)| *i == self.selected)
            .unwrap_or(0)
    }

    pub fn move_up(&mut self) {
        let visible = self.visible();
        if visible.is_empty() {
            return;
        }
        let cur = self.selected_visible_idx();
        if cur > 0 {
            self.selected = visible[cur - 1].0;
        }
    }

    pub fn move_down(&mut self) {
        let visible = self.visible();
        if visible.is_empty() {
            return;
        }
        let cur = self.selected_visible_idx();
        if cur + 1 < visible.len() {
            self.selected = visible[cur + 1].0;
        }
    }

    pub fn selected_agent(&self) -> Option<&AgentTypeOption> {
        let visible = self.visible();
        visible
            .iter()
            .find(|(i, _)| *i == self.selected)
            .map(|(_, a)| *a)
    }
}

fn detect_installed_agents() -> Vec<AgentTypeOption> {
    let candidates = [
        ("Claude Code", "claude", "claude"),
        ("Cursor Agent", "cursor", "agent"),
        ("OpenCode", "opencode", "opencode"),
        ("Gemini CLI", "gemini", "gemini"),
        ("Aider", "aider", "aider"),
    ];
    candidates
        .iter()
        .map(|(name, agent_type, command)| {
            let installed = which_installed(command);
            AgentTypeOption { name, agent_type, command, installed }
        })
        .collect()
}

fn which_installed(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn render(f: &mut Frame, modal: &ModalState, area: Rect) {
    let visible = modal.visible();
    let modal_width = 52u16;
    let modal_height = (visible.len() as u16).max(1) + 7;

    let x = area.x + area.width.saturating_sub(modal_width) / 2;
    let y = area.y + area.height.saturating_sub(modal_height) / 2;
    let modal_area = Rect {
        x,
        y,
        width: modal_width.min(area.width),
        height: modal_height.min(area.height),
    };

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" Launch Agent ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let filter_line = Line::from(vec![
        Span::styled("  Filter  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("[{:<30}]", &modal.filter),
            Style::default().fg(Color::White).bg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(filter_line), chunks[1]);

    let list_area = chunks[3];
    if visible.is_empty() {
        f.render_widget(
            Paragraph::new("  no matches").style(Style::default().fg(Color::DarkGray)),
            list_area,
        );
    } else {
        let list_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(visible.iter().map(|_| Constraint::Length(1)).collect::<Vec<_>>())
            .split(list_area);

        for ((orig_idx, agent), &chunk) in visible.iter().zip(list_chunks.iter()) {
            let is_selected = *orig_idx == modal.selected;
            let prefix = if is_selected { "  ❯ " } else { "    " };

            let name_style = if agent.installed {
                if is_selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                }
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let (status_text, status_style) = if agent.installed {
                ("✓ installed", Style::default().fg(Color::Green))
            } else {
                ("✗ not found", Style::default().fg(Color::DarkGray))
            };

            let line = Line::from(vec![
                Span::styled(format!("{}{:<20}", prefix, agent.name), name_style),
                Span::styled(status_text, status_style),
            ]);
            f.render_widget(Paragraph::new(line), chunk);
        }
    }

    let hint = Paragraph::new("  type to filter  ↑↓ select  Enter launch  Esc cancel")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint, chunks[4]);
}
