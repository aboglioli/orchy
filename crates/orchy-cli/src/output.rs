use std::io::{IsTerminal, Write};

use serde::Serialize;

use crate::error::CliResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Text,
    Json,
}

pub(crate) struct Output {
    format: Format,
    colour: bool,
}

impl Output {
    pub(crate) fn new(json: bool, no_colour: bool) -> Self {
        let interactive = std::io::stdout().is_terminal();
        Self {
            format: if json { Format::Json } else { Format::Text },
            colour: interactive && !no_colour && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    pub(crate) fn emit<T: Serialize>(
        &self,
        value: &T,
        render: impl FnOnce(&T) -> String,
    ) -> CliResult<()> {
        let mut stdout = std::io::stdout().lock();
        match self.format {
            Format::Json => {
                serde_json::to_writer_pretty(&mut stdout, value)
                    .map_err(|e| crate::error::CliError::config(e.to_string()))?;
                writeln!(stdout)?;
            }
            Format::Text => {
                let text = render(value);
                if !text.is_empty() {
                    writeln!(stdout, "{text}")?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn note(&self, message: impl std::fmt::Display) {
        if self.format == Format::Text {
            eprintln!("{}", self.dim(&message.to_string()));
        }
    }

    pub(crate) fn dim(&self, text: &str) -> String {
        if self.colour {
            format!("\x1b[2m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    pub(crate) fn bold(&self, text: &str) -> String {
        if self.colour {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }
}

pub(crate) fn short(id: &str) -> &str {
    id.get(id.len().saturating_sub(6)..).unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_ids_are_the_tail_and_never_panic_on_a_stub() {
        assert_eq!(short("01ARZ3NDEKTSV4RRFFQ69G5FAV"), "9G5FAV");
        assert_eq!(short("abc"), "abc");
        assert_eq!(short(""), "");
    }

    #[test]
    fn colour_is_off_when_not_a_terminal() {
        let output = Output::new(false, false);
        assert_eq!(output.bold("x"), "x", "tests do not run on a tty");
    }
}
