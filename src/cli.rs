//! The command-line surface of the binary.
//!
//! Cargo invokes a subcommand as `cargo-zempic zempic <subcommand>`, injecting
//! its own name as the first argument. The argument root absorbs that injected
//! value so `cargo zempic ...` and `cargo-zempic ...` parse identically.

use crate::{AnalysisConfig, dimension::Dimension, workspace};
use clap::Parser;

pub mod reject;

/// Builds the parser for a `--ignore`/`--only` selector.
///
/// The value is parsed by [`Selector::parse`] at argument-parsing time, so a
/// malformed selector fails before any workspace is read and the diagnostic
/// names the offending value rather than surfacing after metadata loading.
fn selector_parser() -> clap::builder::ValueParser {
    clap::builder::ValueParser::new(crate::finding::Selector::parse)
}

/// The parsed command line, root of the argument grammar.
#[derive(Parser, Clone, PartialEq, Eq, Debug)]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Args {
    /// The `zempic` value cargo injects ahead of the subcommand.
    ///
    /// The positional is optional and restricted to that single value so
    /// `cargo-zempic check` and `cargo-zempic zempic check` yield identical
    /// arguments, while any other leading value remains a parse error.
    #[arg(hide = true, value_parser = clap::builder::PossibleValuesParser::new(["zempic"]))]
    pub dummy: Option<String>,

    /// The requested subcommand.
    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands the binary understands.
#[derive(Clone, Eq, PartialEq, Debug, clap::Subcommand)]
pub enum Command {
    /// Analyze the workspace and report.
    Check(CheckArgs),
}

/// Arguments specific to the `check` subcommand.
#[derive(Clone, Eq, PartialEq, Debug, clap::Args)]
pub struct CheckArgs {
    /// The analysis arguments shared by every analysis subcommand.
    #[command(flatten)]
    pub analysis: AnalysisArgs,
}

/// Arguments governing what an analysis reads.
///
/// Every field is a plain input to the resolution; none constrains another, so
/// the struct converts into an [`AnalysisConfig`] without validating anything.
#[derive(Clone, Eq, PartialEq, Debug, clap::Args)]
pub struct AnalysisArgs {
    /// The optimization objectives to answer, defaulting to the resolution's
    /// own default when unset.
    #[arg(
        short = 'O',
        long = "optimize-for",
        value_name = "DIM",
        value_delimiter = ',',
        num_args = 1..,
        value_enum,
    )]
    pub optimize_for: Vec<Dimension>,

    /// The manifest to resolve from, or the current directory's when unset.
    #[arg(long = "manifest-path", value_name = "PATH")]
    pub manifest_path: Option<camino::Utf8PathBuf>,

    /// Package specs narrowing the analysis to named workspace members.
    #[arg(short = 'p', long = "package", value_name = "SPEC")]
    pub package: Vec<String>,

    /// Analyze every workspace member.
    #[arg(long = "workspace", alias = "all")]
    pub workspace: bool,

    /// Package specs to omit from a workspace-wide analysis.
    #[arg(long = "exclude", value_name = "SPEC")]
    pub exclude: Vec<String>,

    /// The cargo profile whose settings govern the analyzed artifacts.
    #[arg(long = "profile", value_name = "NAME")]
    pub profile: Option<String>,

    /// Features to activate during resolution.
    #[arg(
        short = 'F',
        long = "features",
        value_name = "LIST",
        value_delimiter = ',',
        num_args = 1..,
    )]
    pub features: Vec<String>,

    /// Activate every feature of every selected package.
    #[arg(long = "all-features")]
    pub all_features: bool,

    /// Disable the default feature set.
    #[arg(long = "no-default-features")]
    pub no_default_features: bool,

    /// The target triple to resolve for, or the host's when unset.
    #[arg(long = "target", value_name = "TRIPLE")]
    pub target: Option<String>,

    /// The report format. Overrides the `[report] format` of `zempic.toml`.
    #[arg(long = "format", value_name = "FMT", value_enum)]
    pub format: Option<crate::report::Format>,

    /// The severity that gates a non-zero exit code. Overrides the
    /// `[report] fail_on` of `zempic.toml`.
    #[arg(long = "fail-on", value_name = "SEVERITY", value_enum)]
    pub fail_on: Option<crate::report::FailOn>,

    /// Render one line per finding, without the header, tier headings or the
    /// per-finding detail, tradeoff and metadata lines.
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// When to color the report. `auto` honors `NO_COLOR` and `CLICOLOR`.
    #[arg(
        long = "color",
        value_name = "WHEN",
        default_value = "auto",
        value_enum
    )]
    pub color: crate::report::ColorChoice,

    /// Selectors whose matching rules are suppressed. A rule named by `--only`
    /// is reported even when an ignore would otherwise suppress it.
    #[arg(long = "ignore", value_name = "SELECTOR", value_parser = selector_parser())]
    pub ignore: Vec<crate::finding::Selector>,

    /// Restrict the run to matching rules. A rule named here is reported even
    /// when an ignore would otherwise suppress it.
    #[arg(long = "only", value_name = "SELECTOR", value_parser = selector_parser())]
    pub only: Vec<crate::finding::Selector>,

    /// Include advisory and report-only findings.
    #[arg(long = "include-advisory", overrides_with = "no_include_advisory")]
    pub include_advisory: bool,

    /// Suppress the advisory notes and report-only findings. Nightly findings
    /// stay visible, since a nightly rule reports report-only until
    /// `--allow-nightly`.
    #[arg(long = "no-include-advisory")]
    pub no_include_advisory: bool,

    /// Raise nightly-only findings from report-only to consequential.
    #[arg(long = "allow-nightly")]
    pub allow_nightly: bool,

    /// Cargo's target-selection flags, declared only to be rejected.
    #[command(flatten)]
    pub rejected: reject::RejectedTargetArgs,
}

/// Reject an invocation that selected a target-selection flag.
///
/// Returns `Ok(())` when the invocation carries no target-selection flag. The
/// conversion into an [`AnalysisConfig`] ignores the rejected flags entirely,
/// so this is the only gate that stops one from reaching the analysis.
///
/// # Errors
///
/// Returns [`ZempicError::RejectedTargetFlag`](crate::ZempicError::RejectedTargetFlag)
/// naming the first target-selection flag present.
pub fn validate(args: &AnalysisArgs) -> crate::Result<()> {
    match reject::rejected_flag(&args.rejected) {
        Some(flag) => Err(crate::ZempicError::RejectedTargetFlag { flag }),
        None => Ok(()),
    }
}

impl From<&AnalysisArgs> for AnalysisConfig {
    fn from(args: &AnalysisArgs) -> Self {
        AnalysisConfig {
            dimensions: args.optimize_for.clone(),
            manifest_path: args.manifest_path.clone(),
            selection: workspace::PackageSelection {
                packages: args.package.clone(),
                workspace: args.workspace,
                exclude: args.exclude.clone(),
            },
            profile: args.profile.clone(),
            features: workspace::FeatureSelection {
                features: args.features.clone(),
                all_features: args.all_features,
                no_default_features: args.no_default_features,
            },
            target: args.target.clone(),
            format: args.format,
            fail_on: args.fail_on,
            quiet: args.quiet,
            gates: crate::config::Gates {
                ignore: args
                    .ignore
                    .iter()
                    .map(|selector| crate::config::IgnoreRule {
                        selector: selector.clone(),
                        text: selector.text(),
                        reason: None,
                        packages: None,
                        expires: None,
                    })
                    .collect(),
                only: args.only.clone(),
                include_advisory: if args.no_include_advisory {
                    Some(false)
                } else if args.include_advisory {
                    Some(true)
                } else {
                    None
                },
                allow_nightly: if args.allow_nightly { Some(true) } else { None },
            },
            today: crate::config::CivilDate::today(),
            environment: crate::cargo_config::CargoEnvironment::from_process(),
            toolchain: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AnalysisArgs, Args, Command};
    use crate::{AnalysisConfig, dimension::Dimension, workspace};
    use clap::Parser;

    /// The nine target-selection flags with the value each takes, or `None` for
    /// a flag that is a plain boolean.
    const REJECTED_FLAGS: [(&str, Option<&str>); 9] = [
        ("--bin", Some("foo")),
        ("--bins", None),
        ("--example", Some("foo")),
        ("--examples", None),
        ("--bench", Some("foo")),
        ("--benches", None),
        ("--test", Some("foo")),
        ("--tests", None),
        ("--all-targets", None),
    ];

    /// The analysis arguments a parsed invocation selected.
    fn analysis(args: &Args) -> &AnalysisArgs {
        match &args.command {
            Command::Check(check) => &check.analysis,
        }
    }

    #[test]
    fn dummy_positional_absorbs_the_injected_argument() {
        let injected = Args::try_parse_from(["cargo-zempic", "zempic", "check"])
            .expect("the injected positional is absorbed");
        let direct =
            Args::try_parse_from(["cargo-zempic", "check"]).expect("the direct invocation parses");

        assert_eq!(injected.command, direct.command);
    }

    #[test]
    fn bare_invocation_fails_to_parse() {
        let error =
            Args::try_parse_from(["cargo-zempic"]).expect_err("a missing subcommand is rejected");

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn dummy_positional_rejects_any_other_value() {
        let result = Args::try_parse_from(["cargo-zempic", "nonsense", "check"]);

        assert!(result.is_err());
    }

    #[test]
    fn optimize_for_accepts_comma_separated_and_repeated_forms() {
        let comma = Args::try_parse_from(["cargo-zempic", "check", "-O", "size,size"])
            .expect("the comma-separated form parses");
        let repeated = Args::try_parse_from(["cargo-zempic", "check", "-O", "size", "-O", "size"])
            .expect("the repeated form parses");

        let comma_dimensions = &analysis(&comma).optimize_for;
        let repeated_dimensions = &analysis(&repeated).optimize_for;

        assert_eq!(comma_dimensions, &[Dimension::Size, Dimension::Size]);
        assert_eq!(repeated_dimensions, &[Dimension::Size, Dimension::Size]);
    }

    #[test]
    fn profile_defaults_to_release() {
        let args = Args::try_parse_from(["cargo-zempic", "check"]).expect("the default parses");

        assert_eq!(analysis(&args).profile, None);
    }

    #[test]
    fn profile_argument_is_absent_when_not_given() {
        let absent = Args::try_parse_from(["cargo-zempic", "check"]).expect("a plain check parses");
        let present = Args::try_parse_from(["cargo-zempic", "check", "--profile", "custom"])
            .expect("an explicit profile parses");

        // No clap default may supply a profile: a default is indistinguishable
        // from a user selection and would silently defeat `[analysis] profile`.
        assert_eq!(analysis(&absent).profile, None);
        assert_eq!(analysis(&present).profile, Some("custom".to_owned()));
    }

    #[test]
    fn unknown_dimension_fails_to_parse() {
        let result = Args::try_parse_from(["cargo-zempic", "check", "-O", "bogus"]);

        assert!(result.is_err());
    }

    #[test]
    fn workspace_flag_accepts_the_all_alias() {
        let args =
            Args::try_parse_from(["cargo-zempic", "check", "--all"]).expect("the alias parses");

        assert!(analysis(&args).workspace);
    }

    #[test]
    fn analysis_args_convert_into_an_analysis_config() {
        let args = Args::try_parse_from([
            "cargo-zempic",
            "check",
            "-O",
            "size",
            "--manifest-path",
            "path/to/Cargo.toml",
            "-p",
            "alpha",
            "--workspace",
            "--exclude",
            "beta",
            "--profile",
            "custom",
            "-F",
            "one,two",
            "--all-features",
            "--no-default-features",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--ignore",
            "profile/lto",
            "--only",
            "profile",
            "--no-include-advisory",
            "--allow-nightly",
        ])
        .expect("a full invocation parses");

        let config = AnalysisConfig::from(analysis(&args));

        assert_eq!(
            config,
            AnalysisConfig {
                dimensions: vec![Dimension::Size],
                manifest_path: Some(camino::Utf8PathBuf::from("path/to/Cargo.toml")),
                selection: workspace::PackageSelection {
                    packages: vec!["alpha".to_owned()],
                    workspace: true,
                    exclude: vec!["beta".to_owned()],
                },
                profile: Some("custom".to_owned()),
                features: workspace::FeatureSelection {
                    features: vec!["one".to_owned(), "two".to_owned()],
                    all_features: true,
                    no_default_features: true,
                },
                target: Some("x86_64-unknown-linux-gnu".to_owned()),
                format: None,
                fail_on: None,
                quiet: false,
                gates: crate::config::Gates {
                    ignore: vec![crate::config::IgnoreRule {
                        selector: crate::finding::Selector::parse("profile/lto").unwrap(),
                        text: "profile/lto".to_owned(),
                        reason: None,
                        packages: None,
                        expires: None,
                    }],
                    only: vec![crate::finding::Selector::parse("profile").unwrap()],
                    include_advisory: Some(false),
                    allow_nightly: Some(true),
                },
                today: crate::config::CivilDate::today(),
                environment: crate::cargo_config::CargoEnvironment::from_process(),
                toolchain: None,
            }
        );
    }

    #[test]
    fn advisory_and_nightly_flags_are_tri_state() {
        let plain = Args::try_parse_from(["cargo-zempic", "check"]).expect("a plain check parses");
        let included = Args::try_parse_from(["cargo-zempic", "check", "--include-advisory"])
            .expect("the assertion parses");
        let negated = Args::try_parse_from(["cargo-zempic", "check", "--no-include-advisory"])
            .expect("the negation parses");
        let nightly = Args::try_parse_from(["cargo-zempic", "check", "--allow-nightly"])
            .expect("the nightly acceptance parses");

        let plain = AnalysisConfig::from(analysis(&plain)).gates;
        let included = AnalysisConfig::from(analysis(&included)).gates;
        let negated = AnalysisConfig::from(analysis(&negated)).gates;
        let nightly = AnalysisConfig::from(analysis(&nightly)).gates;

        // An unstated gate must stay `None` so the configuration file, not a
        // default, decides it: a plain invocation would otherwise overwrite
        // `[analysis] include_advisory` with the default on every run.
        assert_eq!(plain.include_advisory, None);
        assert_eq!(plain.allow_nightly, None);
        assert_eq!(included.include_advisory, Some(true));
        assert_eq!(included.allow_nightly, None);
        assert_eq!(negated.include_advisory, Some(false));
        assert_eq!(negated.allow_nightly, None);
        assert_eq!(nightly.allow_nightly, Some(true));
        assert_eq!(nightly.include_advisory, None);
    }

    #[test]
    fn each_rejected_flag_parses_rather_than_erroring_in_clap() {
        for (flag, value) in REJECTED_FLAGS {
            let mut argv = vec!["cargo-zempic", "check", flag];

            if let Some(value) = value {
                argv.push(value);
            }

            let parsed = Args::try_parse_from(argv);

            assert!(
                parsed.is_ok(),
                "`{flag}` must parse so validation owns its rejection"
            );
        }
    }

    #[test]
    fn rejected_flag_fails_validation_with_the_specific_diagnostic() {
        let args = Args::try_parse_from(["cargo-zempic", "check", "--bin", "foo"])
            .expect("the rejected flag parses");

        let error =
            super::validate(analysis(&args)).expect_err("the rejected flag fails validation");

        assert!(matches!(
            error,
            crate::ZempicError::RejectedTargetFlag { flag: "--bin" }
        ));
        assert!(
            error.to_string().starts_with("`--bin` is not supported\n"),
            "the diagnostic names the flag, got: {error}"
        );
    }

    #[test]
    fn rejected_flag_diagnostic_explains_per_package_narrowing() {
        let message = crate::ZempicError::RejectedTargetFlag {
            flag: "--all-targets",
        }
        .to_string();

        assert!(
            message
                .contains("per-package, not per-target. Use `--package` to narrow the analysis."),
            "the diagnostic explains the narrowing, got: {message:?}"
        );
    }

    #[test]
    fn an_invocation_without_rejected_flags_validates() {
        let args = Args::try_parse_from(["cargo-zempic", "check"]).expect("a plain check parses");

        assert!(super::validate(analysis(&args)).is_ok());
    }

    #[test]
    fn output_arguments_convert_into_the_analysis_config() {
        let selected = Args::try_parse_from([
            "cargo-zempic",
            "check",
            "--format",
            "json",
            "--fail-on",
            "medium",
            "--quiet",
        ])
        .expect("the output arguments parse");
        let plain = Args::try_parse_from(["cargo-zempic", "check"]).expect("a plain check parses");

        let selected = AnalysisConfig::from(analysis(&selected));
        let plain = AnalysisConfig::from(analysis(&plain));

        assert_eq!(selected.format, Some(crate::report::Format::Json));
        assert_eq!(selected.fail_on, Some(crate::report::FailOn::Medium));
        assert!(selected.quiet);

        // No default value may supply a format or threshold: a clap default is
        // indistinguishable from a user selection and would silently defeat
        // `[report]` in "zempic.toml".
        assert_eq!(plain.format, None);
        assert_eq!(plain.fail_on, None);
        assert!(!plain.quiet);
    }

    #[test]
    fn color_defaults_to_auto_and_parses_each_choice() {
        let default =
            Args::try_parse_from(["cargo-zempic", "check"]).expect("a plain check parses");

        assert_eq!(analysis(&default).color, crate::report::ColorChoice::Auto);

        for (name, expected) in [
            ("auto", crate::report::ColorChoice::Auto),
            ("always", crate::report::ColorChoice::Always),
            ("never", crate::report::ColorChoice::Never),
        ] {
            let args = Args::try_parse_from(["cargo-zempic", "check", "--color", name])
                .expect("a color choice parses");

            assert_eq!(analysis(&args).color, expected);
        }
    }

    /// Asserts that every variant's command-line spelling equals the name
    /// `as_str` reports and that `from_name` maps that name back to it.
    fn assert_names_agree<T>(
        as_str: impl Fn(&T) -> &'static str,
        from_name: impl Fn(&str) -> Option<T>,
    ) where
        T: clap::ValueEnum + PartialEq + std::fmt::Debug + Copy,
    {
        for variant in T::value_variants() {
            assert_eq!(
                variant
                    .to_possible_value()
                    .expect("no variant is skipped")
                    .get_name(),
                as_str(variant)
            );
            assert_eq!(from_name(as_str(variant)), Some(*variant));
        }
    }

    #[test]
    fn command_line_names_match_configuration_names() {
        assert_names_agree(Dimension::as_str, Dimension::from_name);
        assert_names_agree(
            crate::report::Format::as_str,
            crate::report::Format::from_name,
        );
        assert_names_agree(
            crate::report::FailOn::as_str,
            crate::report::FailOn::from_name,
        );
        assert_names_agree(
            crate::report::ColorChoice::as_str,
            crate::report::ColorChoice::from_name,
        );
    }
}
