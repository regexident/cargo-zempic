//! The `cargo-zempic` binary: argument parsing, report rendering and the
//! mapping from errors to process exit codes.
//!
//! Parsing, flag validation and error reporting happen here while the analysis
//! itself lives in the library, so the boundary owns the one place where a
//! result becomes a process exit code.

use anyhow::Context;
use cargo_zempic::cli::{Args, Command};
use cargo_zempic::report::Report;
use cargo_zempic::{AnalysisConfig, analyze};
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

            print!("{}", render_report(&report));

            for notice in &report.notices {
                eprintln!("{}", notice.message());
            }

            Ok(())
        }
    }
}

/// Render a report through the human renderer.
///
/// The call site supplies the display inputs: color from the resolved
/// [`ColorChoice`](cargo_zempic::report::ColorChoice) and width from the
/// terminal, so the renderer stays a pure function of its report and inputs.
fn render_report(report: &Report) -> String {
    cargo_zempic::report::human::render(report, false, cargo_zempic::report::render_width())
}
