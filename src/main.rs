//! The `cargo-zempic` binary: argument parsing, report rendering and the
//! mapping from errors to process exit codes.
//!
//! Parsing, flag validation and error reporting happen here while the analysis
//! itself lives in the library, so the boundary owns the one place where a
//! result becomes a process exit code.

use anyhow::Context;
use cargo_zempic::cli::{Args, Command};
use cargo_zempic::profile::ResolvedProfile;
use cargo_zempic::report::Report;
use cargo_zempic::{AnalysisConfig, Resolution, analyze};
use clap::Parser;
use std::process::ExitCode;

/// The exit code §12.4 assigns to a zempic error: an unparseable manifest, a
/// metadata failure or invalid arguments.
const ZEMPIC_ERROR_EXIT: u8 = 2;

fn main() -> ExitCode {
    let args = Args::parse();

    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:?}");

            ExitCode::from(ZEMPIC_ERROR_EXIT)
        }
    }
}

/// Runs `check` and renders its report to stdout.
///
/// Configuration notices go to stderr so a piped report on stdout stays clean.
///
/// # Errors
///
/// Returns an error when a rejected target-selection flag is present, when the
/// analysis arguments cannot be converted, or when [`analyze`] fails; the error
/// chain carries the full context for the boundary to print.
fn run(args: Args) -> anyhow::Result<()> {
    match args.command {
        Command::Check(check_args) => {
            cargo_zempic::cli::validate(&check_args.analysis).map_err(anyhow::Error::msg)?;

            let config = AnalysisConfig::from(&check_args.analysis);
            let report = analyze(&config).context("failed to analyze the workspace")?;

            print!("{}", render_resolution(&report.resolution));
            print!("{}", render_findings(&report));

            for notice in &report.notices {
                eprintln!("{}", notice.message());
            }

            Ok(())
        }
    }
}

/// Renders the line stating whether the report carries findings.
///
/// With no findings the line is exactly `No findings.`; otherwise it states the
/// finding count and nothing more. Severity breakdowns, magnitudes and the
/// magnitude legend are rendered by the report renderer, not here.
fn render_findings(report: &Report) -> String {
    if report.is_empty() {
        "No findings.\n".to_owned()
    } else {
        format!("{} findings.\n", report.findings.len())
    }
}

/// Renders the human report header described by §12.1.
///
/// The header names where the analysis ran, which profile governed it, which
/// dimensions it answers and which target it resolved for. It carries no size
/// figure: the counts are facts about the report itself, and the header omits
/// the `toolchain:` line because nightly availability is outside this scope.
fn render_resolution(resolution: &Resolution) -> String {
    let mut output = String::new();

    output.push_str(&format!("cargo-zempic {}\n", env!("CARGO_PKG_VERSION")));

    output.push_str(&format!(
        "{:>14} {}\n",
        "workspace:",
        format_args!(
            "{}  ({} members, {} selected)",
            resolution.workspace_root,
            resolution.member_count,
            resolution.selected.len()
        )
    ));

    output.push_str(&format!(
        "{:>14} {}\n",
        "profile:",
        format_args!(
            "{}  (inherits: {})",
            resolution.profile.name,
            render_inherits(&resolution.profile)
        )
    ));

    let dimensions: Vec<&str> = resolution
        .dimensions
        .iter()
        .map(|dimension| dimension.as_str())
        .collect();

    output.push_str(&format!(
        "{:>14} {}\n",
        "optimizing for:",
        dimensions.join(", ")
    ));

    output.push_str(&format!("{:>14} {}\n", "target:", resolution.target_triple));

    output
}

/// Renders a profile's `inherits` chain as `parent -> ... -> root`, or `none`
/// when the profile inherits from no other.
fn render_inherits(profile: &ResolvedProfile) -> String {
    if profile.inherits_chain.is_empty() {
        "none".to_owned()
    } else {
        profile.inherits_chain.join(" -> ")
    }
}
