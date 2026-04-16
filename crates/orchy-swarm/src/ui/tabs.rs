use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.tabs.is_empty() {
        let hint = Paragraph::new("  [F2] new agent  [Ctrl+Q] quit")
            .alignment(Alignment::Left);
        f.render_widget(hint, inner);
        return;
    }

    let mut spans: Vec<Span> = Vec::new();

    for (i, tab) in app.tabs.iter().enumerate() {
        let is_active = i == app.active_tab;

        let (indicator, indicator_color) = if tab.is_idle() {
            ("●", Color::Green)
        } else {
            ("●", Color::Yellow)
        };

        let label_style = if is_active {
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        spans.push(Span::styled(format!(" {}:{} ", i + 1, tab.alias), label_style));
        spans.push(Span::styled(
            indicator,
            Style::default().fg(indicator_color),
        ));
        spans.push(Span::raw("  "));
    }

    spans.push(Span::styled(
        "  [F2] new  [F4] close  [Alt+←→] switch",
        Style::default().fg(Color::DarkGray),
    ));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, inner);
}
