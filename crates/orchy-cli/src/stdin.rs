use std::io::Read;

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
