use std::fmt::Write;
use std::path::PathBuf;
use std::{env, fs};

use clap::{Arg, Command, CommandFactory};

use crate::cli::Cli;

const REGENERATE: &str = "ORCHY_WRITE_CLI_REFERENCE";

fn reference() -> String {
    let mut root = Cli::command();
    root.build();
    let mut out = String::from(
        "# orchy command reference\n\nGenerated from the CLI definition; do not edit by hand. Regenerate with `just cli-doc`.\n",
    );
    section(&root, "orchy", &mut out);
    out
}

fn section(command: &Command, path: &str, out: &mut String) {
    let about = command
        .get_long_about()
        .or_else(|| command.get_about())
        .map(ToString::to_string)
        .unwrap_or_default();
    let usage = command.clone().render_usage().to_string();
    let usage = usage.trim_start_matches("Usage: ");
    let _ = write!(out, "\n## `{path}`\n\n");
    if !about.is_empty() {
        let _ = write!(out, "{about}\n\n");
    }
    let _ = write!(out, "```text\n{usage}\n```\n");

    let arguments: Vec<&Arg> = command
        .get_arguments()
        .filter(|a| !a.is_hide_set() && a.get_id() != "help" && a.get_id() != "version")
        .filter(|a| path == "orchy" || !a.is_global_set())
        .collect();
    if !arguments.is_empty() {
        out.push('\n');
        for argument in arguments {
            let name = match (argument.get_long(), argument.get_short()) {
                (Some(long), _) => format!("--{long}"),
                (None, Some(short)) => format!("-{short}"),
                (None, None) => format!("<{}>", argument.get_id()),
            };
            match argument.get_help() {
                Some(help) => {
                    let _ = writeln!(out, "- `{name}` {help}");
                }
                None => {
                    let _ = writeln!(out, "- `{name}`");
                }
            }
        }
    }

    for sub in command
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
    {
        section(sub, &format!("{path} {}", sub.get_name()), out);
    }
}

#[test]
fn the_command_reference_matches_the_cli() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli.md");
    let generated = reference();
    if env::var_os(REGENERATE).is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &generated).unwrap();
        return;
    }
    let current = fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == generated,
        "docs/cli.md is stale; regenerate it with `just cli-doc`"
    );
}
