use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let local = app.tabs.len();
    let total = app.remote_agents.len();
    let line = Line::from(vec![
        Span::styled(" ⬡ ", Style::default().fg(Color::Cyan)),
        Span::raw(&app.orchy_url),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("project: {}", app.project)),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("local: {local}")),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("orchy: {total} agents")),
    ]);
    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, area);
}
