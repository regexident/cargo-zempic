//! Cargo's target-selection flags, declared to be rejected.
//!
//! Cargo's `--bin`, `--example`, `--bench` and `--test` families select
//! individual targets, but zempic analyzes settings that apply per profile and
//! per package. Accepting one of these flags would imply a precision the
//! analysis does not have, so they are declared here — hidden from `--help` —
//! purely so [`diagnostic`] can explain the mismatch instead of clap emitting
//! its generic unexpected-argument text.

use clap::Args;

/// The target-selection flags zempic declines to honor.
///
/// The four value-taking flags accept a target name and may repeat; the five
/// boolean flags select a whole target category. Every flag is hidden so the
/// help output advertises only the arguments that shape the analysis.
#[derive(Args, Clone, Debug, PartialEq, Eq)]
pub struct RejectedTargetArgs {
    /// A single binary target.
    #[arg(long = "bin", value_name = "NAME", hide = true)]
    pub bin: Vec<String>,

    /// Every binary target.
    #[arg(long = "bins", hide = true)]
    pub bins: bool,

    /// A single example target.
    #[arg(long = "example", value_name = "NAME", hide = true)]
    pub example: Vec<String>,

    /// Every example target.
    #[arg(long = "examples", hide = true)]
    pub examples: bool,

    /// A single benchmark target.
    #[arg(long = "bench", value_name = "NAME", hide = true)]
    pub bench: Vec<String>,

    /// Every benchmark target.
    #[arg(long = "benches", hide = true)]
    pub benches: bool,

    /// A single test target.
    #[arg(long = "test", value_name = "NAME", hide = true)]
    pub test: Vec<String>,

    /// Every test target.
    #[arg(long = "tests", hide = true)]
    pub tests: bool,

    /// Every target category at once.
    #[arg(long = "all-targets", hide = true)]
    pub all_targets: bool,
}

/// The canonical spelling of the first rejected flag present, or `None`.
///
/// The flags are inspected in a fixed order, so the
/// reported flag is stable when an invocation carries several.
pub fn rejected_flag(args: &RejectedTargetArgs) -> Option<&'static str> {
    if !args.bin.is_empty() {
        return Some("--bin");
    }
    if args.bins {
        return Some("--bins");
    }
    if !args.example.is_empty() {
        return Some("--example");
    }
    if args.examples {
        return Some("--examples");
    }
    if !args.bench.is_empty() {
        return Some("--bench");
    }
    if args.benches {
        return Some("--benches");
    }
    if !args.test.is_empty() {
        return Some("--test");
    }
    if args.tests {
        return Some("--tests");
    }
    if args.all_targets {
        return Some("--all-targets");
    }

    None
}

/// The diagnostic explaining that `flag` is not supported.
///
/// The explanation names why the flag cannot be honored and points at
/// `--package` as the supported way to narrow the analysis.
pub fn diagnostic(flag: &str) -> String {
    format!(
        "error: `{flag}` is not supported\n\n  zempic analyzes cargo profiles and dependencies, which apply per-profile and\n  per-package, not per-target. Use `--package` to narrow the analysis.\n"
    )
}
