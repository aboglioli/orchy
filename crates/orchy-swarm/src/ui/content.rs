use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    if app.tabs.is_empty() {
        let msg = Paragraph::new("No agents running. Press F2 to launch one.")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(msg, area);
        return;
    }

    if let Some(tab) = app.tabs.get(app.active_tab) {
        let screen = tab.screen.screen();
        let (screen_rows, screen_cols) = screen.size();
        let visible_rows = area.height.min(screen_rows);

        let mut lines: Vec<Line> = Vec::with_capacity(visible_rows as usize);

        for row in 0..visible_rows {
            let mut spans: Vec<Span> = Vec::new();
            let mut current_text = String::new();
            let mut current_style = Style::default();

            for col in 0..screen_cols {
                let cell = screen.cell(row, col);
                let contents = cell.map(|c| c.contents()).unwrap_or_default();
                let ch = if contents.is_empty() { " " } else { &contents };
                let style = cell.map(cell_style).unwrap_or_default();

                if style == current_style {
                    current_text.push_str(ch);
                } else {
                    if !current_text.is_empty() {
                        spans.push(Span::styled(current_text.clone(), current_style));
                        current_text.clear();
                    }
                    current_text.push_str(ch);
                    current_style = style;
                }
            }
            if !current_text.is_empty() {
                spans.push(Span::styled(current_text, current_style));
            }
            lines.push(Line::from(spans));
        }

        let paragraph = Paragraph::new(Text::from(lines));
        f.render_widget(paragraph, area);
    }
}

fn cell_style(cell: &vt100::Cell) -> Style {
    let mut style = Style::default()
        .fg(vt100_color(cell.fgcolor()))
        .bg(vt100_color(cell.bgcolor()));
    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
    }
    style
}

fn vt100_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
