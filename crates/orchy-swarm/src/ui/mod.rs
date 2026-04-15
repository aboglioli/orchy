pub mod content;
pub mod modal;
pub mod statusbar;
pub mod tabs;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

use crate::app::App;

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    tabs::render(f, app, chunks[0]);
    content::render(f, app, chunks[1]);
    statusbar::render(f, app, chunks[2]);

    if let Some(modal) = &app.modal {
        modal::render(f, modal, f.area());
    }
}
