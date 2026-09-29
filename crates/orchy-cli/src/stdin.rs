use std::io::{IsTerminal, Read};

use crate::error::{CliError, CliResult};

/// Agents cannot answer an interactive prompt, so content either comes from a flag or from a
/// pipe; orchy never blocks asking for it.
pub(crate) fn or_read(provided: Option<String>) -> CliResult<String> {
    if let Some(value) = provided {
        return Ok(value);
    }
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer)?;
    if buffer.trim().is_empty() {
        return Err(CliError::config(
            "no content: pass --body/--content or pipe it on stdin",
        ));
    }
    Ok(buffer)
}

/// Content that may legitimately be empty: a flag wins, `-` insists on stdin, and otherwise
/// stdin is read only when something is piped in, so an interactive shell is never blocked.
pub(crate) fn optional(provided: Option<String>) -> CliResult<Option<String>> {
    match provided.as_deref() {
        Some("-") => or_read(None).map(Some),
        Some(_) => Ok(provided),
        None if std::io::stdin().is_terminal() => Ok(None),
        None => {
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            Ok((!buffer.trim().is_empty()).then_some(buffer))
        }
    }
}
