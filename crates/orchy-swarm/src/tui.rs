use std::io::{self, Stdout};

use crossterm::{execute, terminal};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub fn init() -> io::Result<Tui> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

pub fn restore() -> io::Result<()> {
    terminal::disable_raw_mode()?;
    execute!(io::stdout(), terminal::LeaveAlternateScreen)
}
