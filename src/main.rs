//! The `cargo-zempic` binary: argument parsing, report rendering and the
//! mapping from errors to process exit codes.
//!
//! Parsing, flag validation and error reporting happen here while the analysis
//! itself lives in the library, so the boundary owns the one place where a
//! result becomes a process exit code.

use anyhow::Context;
use cargo_zempic::cli::{Args, Command};
use cargo_zempic::finding::Severity;
use cargo_zempic::report::{
    ColorChoice, ColorEnvironment, Format, Report, ReportSettings, render_width, use_color,
};
use cargo_zempic::{AnalysisConfig, analyze};
use clap::Parser;
use std::io::IsTerminal;
use std::process::ExitCode;

/// The exit code §12.4 assigns to a zempic error: an unparseable manifest, a
/// metadata failure or invalid arguments.
const ZEMPIC_ERROR_EXIT: u8 = 2;

/// The exit code §12.4 assigns to a report carrying findings at or above the
/// configured threshold.
const FINDINGS_GATED_EXIT: u8 = 1;

fn main() -> ExitCode {
    let args = Args::parse();

    match run(&args) {
        Ok(gated) => {
            if gated {
                ExitCode::from(FINDINGS_GATED_EXIT)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("{error:?}");

            ExitCode::from(ZEMPIC_ERROR_EXIT)
        }
    }
}

/// Runs `check`, renders its report to stdout and reports whether a finding met
/// the configured threshold.
///
/// Configuration notices go to stderr so a piped report on stdout stays clean.
/// A gated run still prints its full report: the exit code is a signal, not a
/// substitute for output.
///
/// # Errors
///
/// Returns an error when a rejected target-selection flag is present, when the
/// analysis arguments cannot be converted, or when [`analyze`] fails; the error
/// chain carries the full context for the boundary to print.
fn run(args: &Args) -> anyhow::Result<bool> {
    match &args.command {
        Command::Check(check_args) => {
            cargo_zempic::cli::validate(&check_args.analysis).map_err(anyhow::Error::msg)?;

            let config = AnalysisConfig::from(&check_args.analysis);
            let report = analyze(&config).context("failed to analyze the workspace")?;

            let document = render(&report, check_args.analysis.color)
                .context("failed to render the report")?;

            print!("{document}");

            for notice in &report.notices {
                eprintln!("{}", notice.message());
            }

            Ok(gates(&report.findings, report.settings))
        }
    }
}

/// Render `report` in the format its settings select.
///
/// The human format has two shapes: the quiet form when the settings ask for
/// it, and the full form otherwise. The JSON and Markdown documents have one
/// shape each, so `--quiet` does not affect them.
///
/// # Errors
///
/// Returns an error when the JSON document cannot be serialized.
fn render(report: &Report, color_choice: ColorChoice) -> cargo_zempic::Result<String> {
    match report.settings.format {
        Format::Human => {
            let color = use_color(
                color_choice,
                &ColorEnvironment::from_process(std::io::stdout().is_terminal()),
            );
            let width = render_width();

            if report.settings.quiet {
                Ok(cargo_zempic::report::human::render_quiet(
                    report, color, width,
                ))
            } else {
                Ok(cargo_zempic::report::human::render(report, color, width))
            }
        }
        Format::Json => cargo_zempic::report::json::render(report),
        Format::Markdown => Ok(cargo_zempic::report::markdown::render(report)),
    }
}

/// Whether a finding meets the report's gate threshold.
///
/// [`FailOn::Never`](cargo_zempic::report::FailOn::Never) never gates. Any
/// other threshold gates when a finding's severity is at or above it, except
/// that [`Severity::Info`] never gates: an informational note must not break a
/// pipeline at any threshold.
fn gates(findings: &[cargo_zempic::finding::Finding], settings: ReportSettings) -> bool {
    let Some(threshold) = settings.fail_on.threshold() else {
        return false;
    };

    findings.iter().any(|finding| {
        finding.severity != Severity::Info && rank(finding.severity) >= rank(threshold)
    })
}

/// Rank a severity by its semantic ordering.
///
/// The derived `Ord` on [`Severity`] places `High` first and therefore lowest,
/// which would invert the gate; the explicit ranking keeps the comparison on the
/// semantic severity order instead.
fn rank(severity: Severity) -> u8 {
    match severity {
        Severity::Info => 0,
        Severity::Low => 1,
        Severity::Medium => 2,
        Severity::High => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::{gates, rank};
    use cargo_zempic::finding::Severity;
    use cargo_zempic::report::{FailOn, ReportSettings};

    /// An informational finding, which must never gate at any threshold.
    fn info() -> cargo_zempic::finding::Finding {
        finding(Severity::Info)
    }

    /// A finding of the given severity.
    fn finding(severity: Severity) -> cargo_zempic::finding::Finding {
        cargo_zempic::finding::Finding {
            rule: cargo_zempic::finding::RuleId::parse("advisory/no-std").unwrap(),
            package: None,
            tier: cargo_zempic::finding::Tier::Advanced,
            severity,
            magnitude: cargo_zempic::finding::Magnitude::None,
            confidence: cargo_zempic::finding::Confidence::High,
            effort: cargo_zempic::finding::Effort::Trivial,
            runtime_cost: cargo_zempic::finding::RuntimeCost::None,
            fixability: cargo_zempic::finding::Fixability::ReportOnly,
            change: None,
            location: None,
            title: "t".to_owned(),
            detail: "d".to_owned(),
            advice: None,
            tradeoff: "cost".to_owned(),
            references: Vec::new(),
        }
    }

    /// The settings for a threshold with quiet off and the human format.
    fn settings(fail_on: FailOn) -> ReportSettings {
        ReportSettings {
            fail_on,
            ..ReportSettings::default()
        }
    }

    #[test]
    fn info_findings_never_gate_at_any_threshold() {
        let findings = vec![info()];

        for fail_on in [FailOn::Never, FailOn::Low, FailOn::Medium, FailOn::High] {
            assert!(
                !gates(&findings, settings(fail_on)),
                "{fail_on:?} must not gate on an Info finding"
            );
        }
    }

    #[test]
    fn high_findings_gate_at_and_above_high() {
        let findings = vec![finding(Severity::High)];

        assert!(gates(&findings, settings(FailOn::High)));
        assert!(gates(&findings, settings(FailOn::Medium)));
        assert!(gates(&findings, settings(FailOn::Low)));
        assert!(!gates(&findings, settings(FailOn::Never)));
    }

    #[test]
    fn low_findings_gate_only_at_low() {
        let findings = vec![finding(Severity::Low)];

        assert!(!gates(&findings, settings(FailOn::Medium)));
        assert!(gates(&findings, settings(FailOn::Low)));
    }

    #[test]
    fn rank_places_high_above_info() {
        assert!(rank(Severity::High) > rank(Severity::Info));
        assert!(rank(Severity::Medium) > rank(Severity::Low));
    }
}
