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
        let scroll = tab.scroll_offset();

        // When scrolled: replay into a taller parser (cached). Row 0 of that
        // parser is the oldest content; we show the top visible_rows rows.
        // When at bottom (scroll=0): normal rendering of current screen.
        let (screen, start_row) = if scroll > 0 {
            if let Some(s) = tab.scroll_screen() {
                let (total_rows, _) = s.size();
                let (vis_rows, _) = tab.screen.screen().size();
                let top = total_rows.saturating_sub(vis_rows).saturating_sub(scroll as u16);
                (s, top)
            } else {
                (tab.screen.screen(), 0)
            }
        } else {
            (tab.screen.screen(), 0)
        };

        let (screen_rows, screen_cols) = screen.size();
        let visible_rows = area.height.min(screen_rows.saturating_sub(start_row));

        let mut lines: Vec<Line> = Vec::with_capacity(visible_rows as usize);

        for row in 0..visible_rows {
            let vt_row = start_row + row;
            let mut spans: Vec<Span> = Vec::new();
            let mut current_text = String::new();
            let mut current_style = Style::default();

            for col in 0..screen_cols {
                let cell = screen.cell(vt_row, col);
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

        if scroll > 0 {
            use ratatui::style::Color;
            let hint = format!(" ↑ scrolled ({scroll} rows) — F8 to scroll down, F7 to scroll up ");
            let hint_line = Line::from(Span::styled(
                hint,
                Style::default().fg(Color::Black).bg(Color::Yellow),
            ));
            let hint_area = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };
            f.render_widget(Paragraph::new(hint_line), hint_area);
        }
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
