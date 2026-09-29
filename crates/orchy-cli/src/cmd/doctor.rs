use orchy_application::Application;
use orchy_application::doctor::{DoctorCommand, DoctorDto};

use crate::error::{CliError, CliResult};
use crate::output::Output;

pub(crate) async fn run(app: &Application, fix: bool, out: &Output) -> CliResult<()> {
    let report = app.doctor.execute(DoctorCommand { fix }).await?;
    out.emit(&report, |r| render(r, out))?;
    match report.problems.len() {
        0 => Ok(()),
        n => Err(CliError::ProblemsRemain(n)),
    }
}

fn render(report: &DoctorDto, out: &Output) -> String {
    let mut lines = Vec::new();
    if report.fixed > 0 {
        lines.push(format!("fixed {}", report.fixed));
    }
    if report.problems.is_empty() {
        lines.push("the vault is healthy".to_owned());
        return lines.join("\n");
    }
    for problem in &report.problems {
        let marker = if problem.fixable { "fixable" } else { "manual" };
        lines.push(format!(
            "{}  {}  {}\n  {}",
            out.bold(&problem.location),
            problem.kind,
            out.dim(marker),
            problem.detail.lines().next().unwrap_or_default()
        ));
    }
    lines.join("\n")
}
