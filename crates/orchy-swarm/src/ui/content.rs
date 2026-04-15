use ansi_to_tui::IntoText;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    if app.tabs.is_empty() {
        let msg = Paragraph::new("No agents running. Press Ctrl+N to launch one.")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(msg, area);
        return;
    }

    if let Some(tab) = app.tabs.get(app.active_tab) {
        let all_bytes = tab.all_output();
        let text = all_bytes.into_text().unwrap_or_default();
        let height = area.height as usize;
        let total = text.lines.len();
        let skip = total.saturating_sub(height + tab.scroll_offset);
        let visible: Vec<_> = text.lines.into_iter().skip(skip).take(height).collect();
        let paragraph =
            Paragraph::new(Text::from(visible)).block(Block::default().borders(Borders::NONE));
        f.render_widget(paragraph, area);
    }
}
