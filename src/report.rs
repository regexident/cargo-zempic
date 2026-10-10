//! The report an analysis returns and the classes that configure it.

use crate::finding::{Finding, Magnitude, Severity, Tier};

pub mod human;

/// How many findings carry each severity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeverityCounts {
    /// Findings of [`Severity::High`].
    pub high: usize,
    /// Findings of [`Severity::Medium`].
    pub medium: usize,
    /// Findings of [`Severity::Low`].
    pub low: usize,
    /// Findings of [`Severity::Info`].
    pub info: usize,
}

/// How many findings carry each magnitude.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MagnitudeCounts {
    /// Findings of [`Magnitude::Large`].
    pub large: usize,
    /// Findings of [`Magnitude::Moderate`].
    pub moderate: usize,
    /// Findings of [`Magnitude::Small`].
    pub small: usize,
    /// Findings of [`Magnitude::Negligible`].
    pub negligible: usize,
    /// Findings of [`Magnitude::None`].
    pub none: usize,
}

/// Counts about a report's findings.
///
/// These are counts about the report itself, never a size figure: no percentage,
/// no byte count and no total of magnitudes is computed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// The number of findings.
    pub total: usize,
    /// The number of findings an automated fix exists for.
    pub fixable: usize,
    /// The findings per severity.
    pub by_severity: SeverityCounts,
    /// The findings per magnitude.
    pub by_magnitude: MagnitudeCounts,
}

impl Summary {
    /// Count `findings` by severity and magnitude.
    pub fn of(findings: &[Finding]) -> Self {
        let mut summary = Summary::default();

        for finding in findings {
            summary.total += 1;

            if finding.is_automatable() {
                summary.fixable += 1;
            }

            match finding.severity {
                Severity::High => summary.by_severity.high += 1,
                Severity::Medium => summary.by_severity.medium += 1,
                Severity::Low => summary.by_severity.low += 1,
                Severity::Info => summary.by_severity.info += 1,
            }

            match finding.magnitude {
                Magnitude::Large => summary.by_magnitude.large += 1,
                Magnitude::Moderate => summary.by_magnitude.moderate += 1,
                Magnitude::Small => summary.by_magnitude.small += 1,
                Magnitude::Negligible => summary.by_magnitude.negligible += 1,
                Magnitude::None => summary.by_magnitude.none += 1,
            }
        }

        summary
    }
}

/// A diagnostic a run surfaces alongside its findings.
///
/// Notices describe configuration rather than findings, so an expired or
/// undocumented ignore is reported even in a run that produced nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// An ignore whose expiry has passed and which no longer suppresses.
    ExpiredIgnore {
        /// The selector as written.
        selector: String,
        /// The expiry date in `YYYY-MM-DD` form.
        expired: String,
    },
    /// An ignore that records no reason.
    UndocumentedIgnore {
        /// The selector as written.
        selector: String,
    },
}

impl Notice {
    /// The message this notice prints.
    pub fn message(&self) -> String {
        match self {
            Notice::ExpiredIgnore { selector, expired } => format!(
                "ignore `{selector}` expired on {expired} and no longer suppresses anything"
            ),
            Notice::UndocumentedIgnore { selector } => {
                format!("ignore `{selector}` records no reason")
            }
        }
    }
}

/// The result of an analysis: the workspace resolved, the findings and
/// conflicts produced, the notices raised and the summary counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// The resolution the analysis ran against.
    pub resolution: crate::Resolution,
    /// The findings, in report order.
    pub findings: Vec<Finding>,
    /// The cross-dimension conflicts detected.
    pub conflicts: Vec<crate::finding::Conflict>,
    /// The configuration notices raised.
    pub notices: Vec<Notice>,
    /// The settings the run resolved from the command line and configuration.
    pub settings: ReportSettings,
    /// The summary counts.
    pub summary: Summary,
}

impl Report {
    /// Assemble a report, rejecting any finding with no stated tradeoff.
    ///
    /// Findings are sorted by tier, then severity and magnitude descending,
    /// then by rule, package and location ascending. The final three
    /// components make the order total, so a given input always renders in the
    /// same order.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::MissingTradeoff`](crate::ZempicError::MissingTradeoff)
    /// naming the rule when a finding's `tradeoff` holds no non-whitespace
    /// character.
    pub fn new(
        resolution: crate::Resolution,
        mut findings: Vec<Finding>,
        conflicts: Vec<crate::finding::Conflict>,
        notices: Vec<Notice>,
        settings: ReportSettings,
    ) -> crate::Result<Self> {
        for finding in &findings {
            if !finding.tradeoff_is_present() {
                return Err(crate::ZempicError::MissingTradeoff {
                    rule: finding.rule.to_string(),
                });
            }
        }

        findings.sort_by_key(sort_key);

        let summary = Summary::of(&findings);

        Ok(Self {
            resolution,
            findings,
            conflicts,
            notices,
            settings,
            summary,
        })
    }

    /// Whether the report carries neither findings nor conflicts.
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty() && self.conflicts.is_empty()
    }

    /// The findings of `tier`, in report order.
    pub fn findings_in(&self, tier: Tier) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(move |finding| finding.tier == tier)
    }
}

/// The total ordering key for a finding.
/// `Reverse` expresses the descending components. The trailing components are
/// the rule identifier, the package name and the location, which together make
/// the key total so that a given set of findings always sorts identically. The
/// key is owned because a rule's path text and a location's components are
/// derived rather than borrowed.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    tier: Tier,
    severity: std::cmp::Reverse<Severity>,
    magnitude: std::cmp::Reverse<Magnitude>,
    rule: String,
    package: Option<String>,
    location: Option<(camino::Utf8PathBuf, usize, usize)>,
}

/// Build the total ordering key for `finding`.
fn sort_key(finding: &Finding) -> SortKey {
    let location = finding.location.as_ref().map(|location| {
        (
            location.span.file.clone(),
            location.span.line,
            location.span.column,
        )
    });

    SortKey {
        tier: finding.tier,
        severity: std::cmp::Reverse(finding.severity),
        magnitude: std::cmp::Reverse(finding.magnitude),
        rule: finding.rule.path_text(),
        package: finding.package.clone(),
        location,
    }
}

/// The output format a run renders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    /// The human-readable report.
    #[default]
    Human,
    /// A JSON document.
    Json,
    /// A Markdown document.
    Markdown,
}

impl Format {
    /// The name this format is selected by on the command line.
    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Human => "human",
            Format::Json => "json",
            Format::Markdown => "markdown",
        }
    }

    /// The format a name selects, or [`None`] when no format carries it.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "human" => Some(Format::Human),
            "json" => Some(Format::Json),
            "markdown" => Some(Format::Markdown),
            _ => None,
        }
    }
}

/// The severity at which a finding gates a non-zero exit code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FailOn {
    /// Never gate on severity.
    #[default]
    Never,
    /// Gate on [`Severity::Low`] or above.
    Low,
    /// Gate on [`Severity::Medium`] or above.
    Medium,
    /// Gate on [`Severity::High`].
    High,
}

impl FailOn {
    /// The name this threshold is selected by on the command line.
    pub fn as_str(&self) -> &'static str {
        match self {
            FailOn::Never => "never",
            FailOn::Low => "low",
            FailOn::Medium => "medium",
            FailOn::High => "high",
        }
    }

    /// The threshold a name selects, or [`None`] when no threshold carries it.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "never" => Some(FailOn::Never),
            "low" => Some(FailOn::Low),
            "medium" => Some(FailOn::Medium),
            "high" => Some(FailOn::High),
            _ => None,
        }
    }

    /// The least severity that gates, or [`None`] for [`FailOn::Never`].
    ///
    /// [`Severity::Info`] is below every threshold this returns, so an
    /// informational finding never gates an exit code.
    pub fn threshold(&self) -> Option<Severity> {
        match self {
            FailOn::Never => None,
            FailOn::Low => Some(Severity::Low),
            FailOn::Medium => Some(Severity::Medium),
            FailOn::High => Some(Severity::High),
        }
    }
}

/// The output settings a run resolved from the command line and configuration.
///
/// The format and threshold are resolved by [`analyze`](crate::analyze), which
/// is the only place the configuration file is read, so a report carries the
/// effective values rather than the inputs they were resolved from. The color
/// choice is not a field: color depends on the process environment and the
/// terminal rather than on the analysis, so the binary resolves it directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReportSettings {
    /// The output format the run renders.
    pub format: Format,
    /// The severity at which a finding gates a non-zero exit code.
    pub fail_on: FailOn,
    /// Whether findings render one per line.
    pub quiet: bool,
}

/// When color is used in a rendered report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorChoice {
    /// Use color only when the environment and the terminal permit it.
    #[default]
    Auto,
    /// Use color regardless of the environment.
    Always,
    /// Use no color regardless of the environment.
    Never,
}

impl ColorChoice {
    /// The name this choice is selected by on the command line.
    pub fn as_str(&self) -> &'static str {
        match self {
            ColorChoice::Auto => "auto",
            ColorChoice::Always => "always",
            ColorChoice::Never => "never",
        }
    }

    /// The choice a name selects, or [`None`] when no choice carries it.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "auto" => Some(ColorChoice::Auto),
            "always" => Some(ColorChoice::Always),
            "never" => Some(ColorChoice::Never),
            _ => None,
        }
    }
}

/// The environment inputs that decide whether `--color auto` uses color.
///
/// The process environment is read through [`from_process`](Self::from_process)
/// alone, so the decision in [`use_color`] is a pure function of its inputs and
/// remains testable in parallel with other tests: the test harness runs tests in
/// threads of one process, and mutating the environment from one test would be
/// unsound while another reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorEnvironment {
    /// The `NO_COLOR` variable, or [`None`] when it is unset.
    pub no_color: Option<String>,
    /// The `CLICOLOR` variable, or [`None`] when it is unset.
    pub clicolor: Option<String>,
    /// Whether standard output is a terminal.
    pub is_terminal: bool,
}

impl ColorEnvironment {
    /// Read the color environment, taking the terminal state as an input.
    pub fn from_process(is_terminal: bool) -> Self {
        Self {
            no_color: std::env::var("NO_COLOR").ok(),
            clicolor: std::env::var("CLICOLOR").ok(),
            is_terminal,
        }
    }
}

/// Whether a report is rendered with color.
///
/// [`ColorChoice::Never`] yields `false` and [`ColorChoice::Always`] yields
/// `true` regardless of the environment: an explicit request is the user
/// overriding detection. [`ColorChoice::Auto`] yields `false` when `NO_COLOR` is
/// set to a non-empty value, `false` when `CLICOLOR` is `0`, and otherwise
/// follows whether standard output is a terminal.
pub fn use_color(choice: ColorChoice, environment: &ColorEnvironment) -> bool {
    match choice {
        ColorChoice::Never => false,
        ColorChoice::Always => true,
        ColorChoice::Auto => {
            let suppressed = environment
                .no_color
                .as_deref()
                .is_some_and(|value| !value.is_empty())
                || environment.clicolor.as_deref() == Some("0");

            !suppressed && environment.is_terminal
        }
    }
}

/// The width a report is rendered to when no terminal reports one.
///
/// A non-terminal has no width to offer, so output is fixed at this many
/// columns to remain reproducible across runs and machines.
pub const MINIMUM_WIDTH: usize = 80;

/// The widest a report is rendered, however wide the terminal is.
///
/// A wide terminal is clamped to this many columns because an unreadably long
/// line is worse than an unused margin.
pub const MAXIMUM_WIDTH: usize = 100;

/// The width a report is rendered to, clamped into
/// `MINIMUM_WIDTH..=MAXIMUM_WIDTH`.
///
/// Falls back to [`MINIMUM_WIDTH`] when the terminal reports no size, which is
/// the case under an integration-test harness.
pub fn render_width() -> usize {
    match terminal_size::terminal_size() {
        Some((terminal_size::Width(width), _)) => width as usize,
        None => MINIMUM_WIDTH,
    }
    .clamp(MINIMUM_WIDTH, MAXIMUM_WIDTH)
}

/// Reset any color or emphasis.
pub const RESET: &str = "\x1b[0m";
/// Render text in bold.
pub const BOLD: &str = "\x1b[1m";
/// Render text in red.
pub const RED: &str = "\x1b[31m";
/// Render text in yellow.
pub const YELLOW: &str = "\x1b[33m";
/// Render text in blue.
pub const BLUE: &str = "\x1b[34m";
/// Render text dimmed.
pub const DIM: &str = "\x1b[2m";

/// The color a severity is rendered in.
pub fn severity_color(severity: Severity) -> &'static str {
    match severity {
        Severity::High => RED,
        Severity::Medium => YELLOW,
        Severity::Low => BLUE,
        Severity::Info => DIM,
    }
}

#[cfg(test)]
mod tests {

    use super::{FailOn, MagnitudeCounts, Report, ReportSettings, SeverityCounts, Summary};
    use crate::Resolution;
    use crate::dimension::Dimension;
    use crate::finding::{
        Confidence, Effort, Finding, Fixability, Magnitude, RuleId, RuntimeCost, Severity, Tier,
    };
    use camino::Utf8PathBuf;

    /// A resolution as an analysis would produce it, for report assembly.
    fn resolution() -> Resolution {
        Resolution {
            workspace_root: Utf8PathBuf::from("/w"),
            member_count: 1,
            selected: Vec::new(),
            profile: crate::profile::builtin_defaults(crate::profile::BuiltinProfile::Release),
            target_triple: "x86_64-unknown-linux-gnu".to_owned(),
            dimensions: vec![Dimension::Size],
        }
    }

    /// Build a finding from the fields the ordering and summary read.
    fn finding(rule: &str, tier: Tier, severity: Severity, magnitude: Magnitude) -> Finding {
        finding_with(rule, tier, severity, magnitude, Fixability::Safe)
    }

    /// Build a finding with an explicit fixability.
    fn finding_with(
        rule: &str,
        tier: Tier,
        severity: Severity,
        magnitude: Magnitude,
        fixability: Fixability,
    ) -> Finding {
        Finding {
            rule: RuleId::parse(rule).expect("the rule parses"),
            package: None,
            tier,
            severity,
            magnitude,
            confidence: Confidence::High,
            effort: Effort::Trivial,
            runtime_cost: RuntimeCost::None,
            fixability,
            change: None,
            location: None,
            title: "t".to_owned(),
            detail: "d".to_owned(),
            tradeoff: "cost".to_owned(),
            references: Vec::new(),
        }
    }

    /// The rule identifiers of a report's findings, in report order.
    fn rules(report: &Report) -> Vec<String> {
        report
            .findings
            .iter()
            .map(|finding| finding.rule.to_string())
            .collect()
    }

    #[test]
    fn report_orders_by_tier_then_severity_then_magnitude() {
        let findings = vec![
            finding(
                "advanced/a",
                Tier::Advanced,
                Severity::Info,
                Magnitude::None,
            ),
            finding(
                "profile/c",
                Tier::Foundational,
                Severity::Low,
                Magnitude::Small,
            ),
            finding(
                "profile/b",
                Tier::Foundational,
                Severity::High,
                Magnitude::Moderate,
            ),
            finding(
                "profile/a",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
            ),
            finding(
                "compiler/x",
                Tier::Compiler,
                Severity::Medium,
                Magnitude::Small,
            ),
            finding(
                "deps/y",
                Tier::Dependencies,
                Severity::High,
                Magnitude::Large,
            ),
        ];

        let report = Report::new(
            resolution(),
            findings,
            Vec::new(),
            Vec::new(),
            ReportSettings::default(),
        )
        .unwrap();

        assert_eq!(
            rules(&report),
            [
                "profile/a",
                "profile/b",
                "profile/c",
                "compiler/x",
                "deps/y",
                "advanced/a",
            ]
        );
    }

    #[test]
    fn report_order_is_total_across_equal_ranks() {
        let forward = vec![
            finding(
                "profile/c",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
            ),
            finding(
                "profile/a",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
            ),
            finding(
                "profile/b",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
            ),
        ];
        let reversed = vec![
            finding(
                "profile/b",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
            ),
            finding(
                "profile/a",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
            ),
            finding(
                "profile/c",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
            ),
        ];

        let forward = Report::new(
            resolution(),
            forward,
            Vec::new(),
            Vec::new(),
            ReportSettings::default(),
        )
        .unwrap();
        let reversed = Report::new(
            resolution(),
            reversed,
            Vec::new(),
            Vec::new(),
            ReportSettings::default(),
        )
        .unwrap();

        assert_eq!(rules(&forward), ["profile/a", "profile/b", "profile/c"]);
        assert_eq!(rules(&reversed), ["profile/a", "profile/b", "profile/c"]);
    }

    #[test]
    fn summary_counts_findings_by_severity_and_magnitude() {
        let findings = vec![
            finding_with(
                "profile/a",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
                Fixability::Safe,
            ),
            finding_with(
                "profile/b",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
                Fixability::Consequential,
            ),
            finding_with(
                "profile/c",
                Tier::Foundational,
                Severity::Medium,
                Magnitude::Small,
                Fixability::Safe,
            ),
            finding_with(
                "profile/d",
                Tier::Foundational,
                Severity::Low,
                Magnitude::Small,
                Fixability::ReportOnly,
            ),
            finding_with(
                "profile/e",
                Tier::Foundational,
                Severity::Info,
                Magnitude::None,
                Fixability::ReportOnly,
            ),
        ];

        let report = Report::new(
            resolution(),
            findings,
            Vec::new(),
            Vec::new(),
            ReportSettings::default(),
        )
        .unwrap();

        assert_eq!(
            report.summary,
            Summary {
                total: 5,
                fixable: 3,
                by_severity: SeverityCounts {
                    high: 2,
                    medium: 1,
                    low: 1,
                    info: 1,
                },
                by_magnitude: MagnitudeCounts {
                    large: 2,
                    moderate: 0,
                    small: 2,
                    negligible: 0,
                    none: 1,
                },
            }
        );
    }

    #[test]
    fn report_rejects_a_finding_with_a_blank_tradeoff() {
        let mut blank = finding(
            "profile/strip",
            Tier::Foundational,
            Severity::High,
            Magnitude::Large,
        );
        blank.tradeoff = "   ".to_owned();

        let error = Report::new(
            resolution(),
            vec![blank],
            Vec::new(),
            Vec::new(),
            ReportSettings::default(),
        )
        .expect_err("a blank tradeoff is rejected");

        assert!(matches!(error, crate::ZempicError::MissingTradeoff { .. }));
        assert!(error.to_string().contains("profile/strip"));
    }

    #[test]
    fn fail_on_thresholds_never_include_info() {
        assert_eq!(FailOn::Never.threshold(), None);
        assert_eq!(FailOn::Low.threshold(), Some(Severity::Low));

        for fail_on in [FailOn::Never, FailOn::Low, FailOn::Medium, FailOn::High] {
            if let Some(threshold) = fail_on.threshold() {
                assert!(Severity::Info < threshold, "{fail_on:?} must exclude Info");
            }
        }
    }

    /// A color environment with explicit values, so the decision is tested
    /// without reading or mutating the process environment.
    fn environment_with(
        is_terminal: bool,
        no_color: Option<&str>,
        clicolor: Option<&str>,
    ) -> crate::report::ColorEnvironment {
        crate::report::ColorEnvironment {
            no_color: no_color.map(str::to_owned),
            clicolor: clicolor.map(str::to_owned),
            is_terminal,
        }
    }

    #[test]
    fn color_choice_never_and_always_ignore_the_environment() {
        assert!(!crate::report::use_color(
            crate::report::ColorChoice::Never,
            &environment_with(true, None, None)
        ));

        // `Always` must survive an environment that would otherwise suppress
        // color, since an explicit request overrides detection.
        assert!(crate::report::use_color(
            crate::report::ColorChoice::Always,
            &environment_with(false, Some("1"), Some("0"))
        ));
    }

    #[test]
    fn auto_color_honors_no_color_and_clicolor() {
        use crate::report::{ColorChoice, use_color};

        assert!(!use_color(
            ColorChoice::Auto,
            &environment_with(true, Some("1"), None)
        ));
        assert!(use_color(
            ColorChoice::Auto,
            &environment_with(true, Some(""), None)
        ));
        assert!(!use_color(
            ColorChoice::Auto,
            &environment_with(true, None, Some("0"))
        ));
        assert!(use_color(
            ColorChoice::Auto,
            &environment_with(true, None, Some("1"))
        ));
        assert!(!use_color(
            ColorChoice::Auto,
            &environment_with(false, None, None)
        ));
    }

    #[test]
    fn render_width_stays_inside_the_clamp() {
        let width = crate::report::render_width();

        assert!(
            (crate::report::MINIMUM_WIDTH..=crate::report::MAXIMUM_WIDTH).contains(&width),
            "width {width} is outside the clamp"
        );
    }
}
