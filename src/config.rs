//! `zempic.toml` discovery, merge and schema.

use crate::finding::Selector;
/// A calendar date, compared in calendar order without a date crate.
///
/// Fields are declared year, then month, then day, so the derived [`Ord`]
/// compares year first, then month, then day, which is calendar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    /// The full year, for example `2026`.
    pub year: u16,
    /// The month, `1` to `12`.
    pub month: u8,
    /// The day of the month, `1` to the month's length.
    pub day: u8,
}

impl CivilDate {
    /// The date today, read from the system clock.
    ///
    /// The clock is read against the Unix epoch and converted to a civil date;
    /// a clock before the epoch is treated as the epoch, since an ignore date
    /// that far in the past is expired regardless.
    pub fn today() -> Self {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        Self::from_days_since_epoch((seconds / 86_400) as i64)
    }

    /// The civil date `days` days after 1970-01-01.
    ///
    /// Uses Howard Hinnant's `civil_from_days`, which shifts the year to start
    /// in March so leap days fall at the end of the cycle and no month-length
    /// branch is needed.
    pub fn from_days_since_epoch(days: i64) -> Self {
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let march_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * march_month + 2) / 5 + 1;
        let month = march_month + if march_month < 10 { 3 } else { -9 };
        let year = year_of_era + era * 400 + i64::from(month <= 2);

        CivilDate {
            year: year as u16,
            month: month as u8,
            day: day as u8,
        }
    }

    /// Parse a `YYYY-MM-DD` date, rejecting an impossible month or day.
    ///
    /// Returns [`None`] for anything but exactly four, two and two ASCII
    /// digits, a month outside `1` to `12`, a day outside the month's length,
    /// or a 29 February in a non-leap year.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();

        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }

        let year = digits(&bytes[0..4])? as u16;
        let month = digits(&bytes[5..7])? as u8;
        let day = digits(&bytes[8..10])? as u8;

        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }

        Some(CivilDate { year, month, day })
    }

    /// The date as a zero-padded `YYYY-MM-DD` string.
    pub fn as_iso8601(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Parse exactly `digits.len()` ASCII digits as a number, or [`None`].
fn digits(digits: &[u8]) -> Option<u32> {
    let mut value = 0u32;

    for byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }

        value = value * 10 + u32::from(byte - b'0');
    }

    Some(value)
}

/// Whether `year` is a leap year in the Gregorian calendar.
fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// The number of days in `month` of `year`.
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// The schema version this crate reads.
pub const SCHEMA_VERSION: u32 = 1;

/// A suppression declared in a configuration file.
///
/// `text` is the selector as written in the file or on the command line.
/// `packages` is [`None`] when the key is absent, which means all packages; a
/// [`Some`] list is matched against a package name by exact equality, with the
/// single pattern `"*"` matching every package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IgnoreRule {
    /// The parsed selector the rule suppresses.
    pub selector: Selector,
    /// The selector as written.
    pub text: String,
    /// The recorded reason, or [`None`] when undocumented.
    pub reason: Option<String>,
    /// The packages the ignore is scoped to, or [`None`] for all.
    pub packages: Option<Vec<String>>,
    /// The date the ignore expires, after which it stops suppressing.
    pub expires: Option<CivilDate>,
}

/// The `[analysis]` section, validated but not yet honored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalysisSection {
    /// The optimization objectives to answer.
    pub optimize_for: Option<Vec<crate::dimension::Dimension>>,
    /// The cargo profile under review.
    pub profile: Option<String>,
    /// Whether advisory findings are included.
    pub include_advisory: Option<bool>,
    /// Whether nightly-only findings may be fixable.
    pub allow_nightly: Option<bool>,
}

/// The `[report]` section, validated but not yet honored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReportSection {
    /// The output format.
    pub format: Option<crate::report::Format>,
    /// The severity that gates a non-zero exit code.
    pub fail_on: Option<crate::report::FailOn>,
}

/// A parsed `zempic.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileConfig {
    /// The schema version the file declares.
    pub schema_version: u32,
    /// The `[analysis]` section.
    pub analysis: AnalysisSection,
    /// The `[report]` section.
    pub report: ReportSection,
    /// The `[[ignore]]` entries.
    pub ignore: Vec<IgnoreRule>,
    /// The `[knowledge_base.crates.*]` entries by crate name, which extend and
    /// override the embedded knowledge base.
    pub knowledge_base: std::collections::BTreeMap<String, crate::knowledge::CrateEntry>,
}

impl FileConfig {
    /// The configuration with no settings and no ignores.
    pub fn empty() -> Self {
        FileConfig {
            schema_version: SCHEMA_VERSION,
            analysis: AnalysisSection::default(),
            report: ReportSection::default(),
            ignore: Vec::new(),
            knowledge_base: std::collections::BTreeMap::new(),
        }
    }

    /// Parse `text` as the configuration at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::ConfigParse`](crate::ZempicError::ConfigParse)
    /// when the document is malformed or carries an unknown key,
    /// [`ZempicError::ConfigSchemaVersion`](crate::ZempicError::ConfigSchemaVersion)
    /// when `schema_version` is not [`SCHEMA_VERSION`],
    /// [`ZempicError::KnowledgeBaseEntry`](crate::ZempicError::KnowledgeBaseEntry)
    /// when a `[knowledge_base.crates.*]` entry holds an invalid value, and the
    /// selector, date, dimension, format or fail-on error for an invalid value,
    /// each naming the file and the offending value.
    pub fn parse(path: &camino::Utf8Path, text: &str) -> crate::Result<Self> {
        let raw: RawConfig =
            toml_edit::de::from_str(text).map_err(|source| crate::ZempicError::ConfigParse {
                path: path.to_owned(),
                source,
            })?;

        if raw.schema_version != SCHEMA_VERSION {
            return Err(crate::ZempicError::ConfigSchemaVersion {
                path: path.to_owned(),
                found: raw.schema_version,
                expected: SCHEMA_VERSION,
            });
        }

        let analysis = raw.analysis.into_section(path)?;
        let report = raw.report.into_section(path)?;

        let mut ignore = Vec::with_capacity(raw.ignore.len());

        for entry in raw.ignore {
            ignore.push(entry.into_rule(path)?);
        }

        let knowledge_base =
            crate::knowledge::parse_crates(path.as_str(), raw.knowledge_base.crates)?;

        Ok(FileConfig {
            schema_version: raw.schema_version,
            analysis,
            report,
            ignore,
            knowledge_base,
        })
    }

    /// Merge `self`, the higher-precedence source, over `base`.
    ///
    /// Each [`Some`] scalar of `self` replaces the base's; each [`None`] keeps
    /// the base's. `ignore` entries are the base's followed by `self`'s.
    /// `knowledge_base` entries merge by crate name, `self`'s replacing the
    /// base's entirely.
    pub fn merge_over(self, base: Self) -> Self {
        let mut knowledge_base = base.knowledge_base;

        knowledge_base.extend(self.knowledge_base);

        FileConfig {
            schema_version: self.schema_version,
            analysis: self.analysis.merge_over(base.analysis),
            report: self.report.merge_over(base.report),
            ignore: base.ignore.into_iter().chain(self.ignore).collect(),
            knowledge_base,
        }
    }

    /// The `[analysis]` section's scalar preferences, merged.
    fn analysis_merge(base: AnalysisSection, higher: AnalysisSection) -> AnalysisSection {
        AnalysisSection {
            optimize_for: higher.optimize_for.or(base.optimize_for),
            profile: higher.profile.or(base.profile),
            include_advisory: higher.include_advisory.or(base.include_advisory),
            allow_nightly: higher.allow_nightly.or(base.allow_nightly),
        }
    }
}

impl AnalysisSection {
    /// Merge `self`, the higher-precedence source, over `base`.
    fn merge_over(self, base: Self) -> Self {
        FileConfig::analysis_merge(base, self)
    }
}

impl ReportSection {
    /// Merge `self`, the higher-precedence source, over `base`.
    fn merge_over(self, base: Self) -> Self {
        ReportSection {
            format: self.format.or(base.format),
            fail_on: self.fail_on.or(base.fail_on),
        }
    }
}

/// Discover and merge the configuration for a workspace.
///
/// Reads `zempic.toml` in `workspace_root`, or `.zempic.toml` when the former
/// is absent, as the base, then merges each package directory's file over the
/// result in the order given. A directory equal to `workspace_root` is not read
/// twice. An absent file is not an error; an unreadable or unparseable one is.
///
/// # Errors
///
/// Returns [`ZempicError::ConfigRead`](crate::ZempicError::ConfigRead) when a
/// present file cannot be read and any error [`FileConfig::parse`] returns when
/// one does not validate.
pub fn discover(
    workspace_root: &camino::Utf8Path,
    package_directories: &[camino::Utf8PathBuf],
) -> crate::Result<FileConfig> {
    let mut config = match find_workspace_file(workspace_root)? {
        Some((path, text)) => FileConfig::parse(&path, &text)?,
        None => FileConfig::empty(),
    };

    for directory in package_directories {
        if directory == workspace_root {
            continue;
        }

        if let Some((path, text)) = find_workspace_file(directory)? {
            config = FileConfig::parse(&path, &text)?.merge_over(config);
        }
    }

    Ok(config)
}

/// Read `zempic.toml` or `.zempic.toml` from `directory`, if either exists.
///
/// Returns the path and text of the file that was read. `zempic.toml` wins over
/// `.zempic.toml` when both are present.
fn find_workspace_file(
    directory: &camino::Utf8Path,
) -> crate::Result<Option<(camino::Utf8PathBuf, String)>> {
    for name in ["zempic.toml", ".zempic.toml"] {
        let path = directory.join(name);

        if !path.exists() {
            continue;
        }

        let text =
            std::fs::read_to_string(&path).map_err(|source| crate::ZempicError::ConfigRead {
                path: path.clone(),
                source,
            })?;

        return Ok(Some((path, text)));
    }

    Ok(None)
}

/// The gates that decide which findings a run reports.
///
/// A gate left [`None`] is unstated by its source, so [`resolve_gates`] falls
/// back to the next source and, past them, to the default of the gate itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gates {
    /// The selectors whose matching findings are suppressed.
    pub ignore: Vec<IgnoreRule>,
    /// The selectors restricting the run to matching rules.
    pub only: Vec<Selector>,
    /// Whether advisory and report-only findings are included, defaulting to
    /// `true` when unspecified.
    pub include_advisory: Option<bool>,
    /// Whether nightly-only findings are raised from report-only to
    /// consequential, defaulting to `false` when unspecified.
    pub allow_nightly: Option<bool>,
}

/// Merge the command line's gates over the configuration file's `[analysis]`.
///
/// A gate the command line states wins over the file's; a gate neither states
/// stays [`None`]. The ignore list and the `--only` selectors come from the
/// command line alone, because the file's ignores are merged separately from
/// its `[[ignore]]` entries.
pub fn resolve_gates(cli: &Gates, file: &AnalysisSection) -> Gates {
    Gates {
        ignore: cli.ignore.clone(),
        only: cli.only.clone(),
        include_advisory: cli.include_advisory.or(file.include_advisory),
        allow_nightly: cli.allow_nightly.or(file.allow_nightly),
    }
}

impl Gates {
    /// Whether any `--only` selector explicitly requests `id`.
    ///
    /// A rule named by `--only` is read as an explicit request for it, so it is
    /// exempt from every ignore and from the advisory gate.
    pub fn is_explicitly_requested(&self, id: &crate::finding::RuleId) -> bool {
        self.only.iter().any(|selector| selector.matches(id))
    }
}

impl IgnoreRule {
    /// Whether the ignore has expired as of `today`.
    ///
    /// An ignore expires only once `today` is strictly past its expiry, so an
    /// ignore expiring today still suppresses through the whole day.
    pub fn has_expired(&self, today: CivilDate) -> bool {
        self.expires.is_some_and(|date| date < today)
    }

    /// Whether the ignore is scoped to `package`.
    ///
    /// Returns `true` when no package list is present, when the list contains
    /// the wildcard `"*"`, or when `package` names one of the listed packages.
    /// A finding about the workspace itself (`package` is [`None`]) is not
    /// matched by a package-scoped ignore.
    pub fn applies_to_package(&self, package: Option<&str>) -> bool {
        match &self.packages {
            None => true,
            Some(list) => {
                list.iter().any(|name| name == "*")
                    || package.is_some_and(|name| list.iter().any(|listed| listed == name))
            }
        }
    }
}

/// The findings a run reports and the notices its configuration raised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filtered {
    /// The findings the gates retained.
    pub findings: Vec<crate::finding::Finding>,
    /// The configuration notices raised, in ignore order.
    pub notices: Vec<crate::report::Notice>,
}

/// Apply the gates to `findings`, producing the retained findings and notices.
///
/// A finding matched by an `--only` selector is exempt from every ignore and
/// from the advisory gate, because naming a rule is an explicit request for it.
/// Suppression reads only the ignores that have not expired as of `today`.
///
/// Notices are produced from the ignores alone, so an expired or undocumented
/// ignore warns even in a run with nothing to suppress. They describe every
/// declared ignore, whatever the run's package selection or `--only` scope,
/// because `filter` judges the configuration itself rather than the run.
///
/// The advisory gate never suppresses a finding in the `nightly` category,
/// whether its fixability is [`crate::finding::Fixability::ReportOnly`] for
/// want of [`Gates::allow_nightly`] or [`crate::finding::Fixability::Consequential`]
/// because the gate accepted the remedy, because a nightly rule's fixability is
/// a gate state rather than a statement about advisory content. Every other
/// suppression — the ignores, the `--only` scope, and the `--only` exemption —
/// applies to nightly findings unchanged. Whether a finding belongs to the
/// advisory category is [`crate::finding::Finding::is_advisory`]'s question;
/// this gate asks the narrower one of whether report-only fixability alone
/// accounts for it.
pub fn filter(findings: Vec<crate::finding::Finding>, gates: &Gates, today: CivilDate) -> Filtered {
    let mut notices = Vec::new();

    for ignore in &gates.ignore {
        if ignore.has_expired(today) {
            notices.push(crate::report::Notice::ExpiredIgnore {
                selector: ignore.text.clone(),
                expired: ignore
                    .expires
                    .expect("an expired ignore carries an expiry")
                    .as_iso8601(),
            });
        }

        if !has_reason(ignore.reason.as_deref()) {
            notices.push(crate::report::Notice::UndocumentedIgnore {
                selector: ignore.text.clone(),
            });
        }
    }

    let active: Vec<&IgnoreRule> = gates
        .ignore
        .iter()
        .filter(|ignore| !ignore.has_expired(today))
        .collect();

    let retained = findings
        .into_iter()
        .filter(|finding| {
            if gates.is_explicitly_requested(&finding.rule) {
                return true;
            }

            if !gates.only.is_empty()
                && !gates
                    .only
                    .iter()
                    .any(|selector| selector.matches(&finding.rule))
            {
                return false;
            }

            if !gates.include_advisory.unwrap_or(true)
                && finding.is_advisory()
                && finding.rule.category() != "nightly"
            {
                return false;
            }

            !active.iter().any(|ignore| {
                ignore.selector.matches(&finding.rule)
                    && ignore.applies_to_package(finding.package.as_deref())
            })
        })
        .collect();

    Filtered {
        findings: retained,
        notices,
    }
}

/// Whether a reason records anything beyond whitespace.
fn has_reason(reason: Option<&str>) -> bool {
    reason.is_some_and(|text| text.chars().any(|character| !character.is_whitespace()))
}

/// The private deserialization mirror of `zempic.toml`.
///
/// Unknown keys are rejected, so a configuration carrying a section this crate
/// does not declare fails loudly rather than being silently discarded.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    schema_version: u32,
    #[serde(default)]
    analysis: RawAnalysis,
    #[serde(default)]
    report: RawReport,
    #[serde(default)]
    ignore: Vec<RawIgnore>,
    #[serde(default)]
    knowledge_base: RawKnowledgeBaseSection,
}

/// The `[knowledge_base]` table as deserialized.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawKnowledgeBaseSection {
    #[serde(default)]
    crates: std::collections::BTreeMap<String, crate::knowledge::RawCrateEntry>,
}

/// The `[analysis]` table as deserialized.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawAnalysis {
    #[serde(default)]
    optimize_for: Option<Vec<String>>,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    include_advisory: Option<bool>,
    #[serde(default)]
    allow_nightly: Option<bool>,
}

/// The `[report]` table as deserialized.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawReport {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    fail_on: Option<String>,
}

/// One `[[ignore]]` entry as deserialized.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIgnore {
    rule: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    packages: Option<Vec<String>>,
    #[serde(default)]
    expires: Option<String>,
}

impl RawAnalysis {
    /// Validate the section, converting its values into the public types.
    fn into_section(self, path: &camino::Utf8Path) -> crate::Result<AnalysisSection> {
        let optimize_for = match self.optimize_for {
            None => None,
            Some(names) => {
                let mut dimensions = Vec::with_capacity(names.len());

                for name in names {
                    let dimension =
                        crate::dimension::Dimension::from_name(&name).ok_or_else(|| {
                            crate::ZempicError::ConfigDimension {
                                path: path.to_owned(),
                                found: name.clone(),
                            }
                        })?;

                    dimensions.push(dimension);
                }

                Some(dimensions)
            }
        };

        Ok(AnalysisSection {
            optimize_for,
            profile: self.profile,
            include_advisory: self.include_advisory,
            allow_nightly: self.allow_nightly,
        })
    }
}

impl RawReport {
    /// Validate the section, converting its values into the public types.
    fn into_section(self, path: &camino::Utf8Path) -> crate::Result<ReportSection> {
        let format = match self.format {
            None => None,
            Some(name) => Some(crate::report::Format::from_name(&name).ok_or_else(|| {
                crate::ZempicError::ConfigFormat {
                    path: path.to_owned(),
                    found: name.clone(),
                }
            })?),
        };

        let fail_on = match self.fail_on {
            None => None,
            Some(name) => Some(crate::report::FailOn::from_name(&name).ok_or_else(|| {
                crate::ZempicError::ConfigFailOn {
                    path: path.to_owned(),
                    found: name.clone(),
                }
            })?),
        };

        Ok(ReportSection { format, fail_on })
    }
}

impl RawIgnore {
    /// Validate the entry, converting its values into the public types.
    fn into_rule(self, path: &camino::Utf8Path) -> crate::Result<IgnoreRule> {
        let selector =
            Selector::parse(&self.rule).map_err(|source| crate::ZempicError::ConfigSelector {
                path: path.to_owned(),
                text: self.rule.clone(),
                source,
            })?;

        let expires = match &self.expires {
            None => None,
            Some(text) => {
                Some(
                    CivilDate::parse(text).ok_or_else(|| crate::ZempicError::ConfigDate {
                        path: path.to_owned(),
                        key: "expires".to_owned(),
                        found: text.clone(),
                    })?,
                )
            }
        };

        Ok(IgnoreRule {
            selector,
            text: self.rule,
            reason: self.reason,
            packages: self.packages,
            expires,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AnalysisSection, CivilDate, FileConfig, Filtered, Gates, IgnoreRule, filter, resolve_gates,
    };
    use crate::ZempicError;
    use crate::dimension::Dimension;
    use crate::finding::{
        Confidence, Effort, Finding, Fixability, Magnitude, RuleId, RuntimeCost, Selector,
        Severity, Tier,
    };
    use crate::report::{FailOn, Format, Notice};
    use camino::Utf8Path;

    /// The §6.2 schema.
    const FULL_SCHEMA: &str = "\
schema_version = 1

[analysis]
optimize_for = [\"size\"]
profile = \"release\"
include_advisory = true
allow_nightly = false

[report]
format = \"human\"
fail_on = \"never\"

[[ignore]]
rule = \"profile/lto\"
reason = \"link times in CI exceed our budget\"
packages = [\"*\"]
expires = \"2026-12-31\"

[knowledge_base.crates.internal-http]
versions = \">=1.0, <3.0\"
verified_through = \"2.4.1\"

[[knowledge_base.crates.internal-http.features]]
name = \"telemetry\"
default = true
magnitude = \"moderate\"
advice = \"disable unless the service exports metrics\"
fixable = true
";

    #[test]
    fn civil_date_parses_and_rejects_impossible_dates() {
        assert_eq!(
            CivilDate::parse("2026-12-31"),
            Some(CivilDate {
                year: 2026,
                month: 12,
                day: 31,
            })
        );
        assert_eq!(
            CivilDate::parse("2024-02-29"),
            Some(CivilDate {
                year: 2024,
                month: 2,
                day: 29,
            })
        );

        for text in [
            "2026-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-01-32",
            "2026-1-01",
            "26-01-01",
            "2026/01/01",
            "",
        ] {
            assert_eq!(CivilDate::parse(text), None, "`{text}` must be rejected");
        }
    }

    #[test]
    fn civil_date_converts_days_since_the_epoch() {
        assert_eq!(
            CivilDate::from_days_since_epoch(0),
            CivilDate {
                year: 1970,
                month: 1,
                day: 1,
            }
        );
        assert_eq!(
            CivilDate::from_days_since_epoch(11016),
            CivilDate {
                year: 2000,
                month: 2,
                day: 29,
            }
        );
        assert_eq!(
            CivilDate::from_days_since_epoch(19782),
            CivilDate {
                year: 2024,
                month: 2,
                day: 29,
            }
        );
        assert_eq!(
            CivilDate::from_days_since_epoch(20730),
            CivilDate {
                year: 2026,
                month: 10,
                day: 4,
            }
        );
        assert_eq!(
            CivilDate::from_days_since_epoch(20818),
            CivilDate {
                year: 2026,
                month: 12,
                day: 31,
            }
        );
    }

    #[test]
    fn civil_date_orders_chronologically() {
        assert!(CivilDate::parse("2026-01-31").unwrap() < CivilDate::parse("2026-02-01").unwrap());
        assert!(CivilDate::parse("1999-12-31").unwrap() < CivilDate::parse("2000-01-01").unwrap());
    }

    /// Parse a configuration document at a fixed path.
    fn parse(text: &str) -> crate::Result<FileConfig> {
        FileConfig::parse(Utf8Path::new("zempic.toml"), text)
    }

    #[test]
    fn file_config_parses_the_whole_schema() {
        let config = parse(FULL_SCHEMA).expect("the schema parses");

        assert_eq!(config.schema_version, 1);
        assert_eq!(config.analysis.optimize_for, Some(vec![Dimension::Size]));
        assert_eq!(config.analysis.profile, Some("release".to_owned()));
        assert_eq!(config.analysis.include_advisory, Some(true));
        assert_eq!(config.analysis.allow_nightly, Some(false));
        assert_eq!(config.report.format, Some(Format::Human));
        assert_eq!(config.report.fail_on, Some(FailOn::Never));

        assert_eq!(config.ignore.len(), 1);

        let entry = &config.ignore[0];
        assert_eq!(entry.text, "profile/lto");
        assert_eq!(
            entry.reason,
            Some("link times in CI exceed our budget".to_owned())
        );
        assert_eq!(entry.packages, Some(vec!["*".to_owned()]));
        assert_eq!(
            entry.expires,
            Some(CivilDate {
                year: 2026,
                month: 12,
                day: 31,
            })
        );

        let entry = &config.knowledge_base["internal-http"];
        assert_eq!(
            entry.versions,
            semver::VersionReq::parse(">=1.0, <3.0").unwrap()
        );
        assert_eq!(
            entry.verified_through,
            semver::Version::parse("2.4.1").unwrap()
        );
        assert_eq!(entry.features.len(), 1);
        assert_eq!(entry.features[0].name, "telemetry");
        assert!(entry.features[0].default);
        assert_eq!(entry.features[0].magnitude, Magnitude::Moderate);
        assert!(entry.features[0].fixable);
        assert_eq!(
            entry.features[0].advice,
            "disable unless the service exports metrics"
        );
    }

    #[test]
    fn file_config_parses_a_knowledge_base_entry() {
        let config = parse(
            "schema_version = 1\n\n[knowledge_base.crates.harness]\nversions = \">=0.3, <0.4\"\nverified_through = \"0.3.0\"\nmagnitude = \"large\"\ndev_only = true\n",
        )
        .expect("the entry parses");

        let entry = &config.knowledge_base["harness"];
        assert_eq!(entry.magnitude, Some(Magnitude::Large));
        assert!(entry.dev_only);
        assert!(entry.features.is_empty());
    }

    #[test]
    fn file_config_rejects_an_invalid_knowledge_base_range() {
        let error = parse(
            "schema_version = 1\n\n[knowledge_base.crates.demo]\nversions = \"not a range\"\nverified_through = \"1.0.0\"\n",
        )
        .expect_err("an invalid range is rejected");

        assert!(matches!(error, ZempicError::KnowledgeBaseEntry { .. }));
        assert!(error.to_string().contains("zempic.toml"), "{error}");
        assert!(error.to_string().contains("demo"), "{error}");
    }

    #[test]
    fn file_config_rejects_an_unknown_key() {
        let error = parse("schema_version = 1\n\n[analysis]\nnonsense = true\n")
            .expect_err("an unknown key is rejected");

        assert!(matches!(error, ZempicError::ConfigParse { .. }));
        assert!(error.to_string().contains("zempic.toml"));
        assert!(error.to_string().contains("nonsense"));
    }

    #[test]
    fn file_config_rejects_a_wrong_schema_version() {
        let error = parse("schema_version = 2\n").expect_err("a wrong version is rejected");

        assert!(matches!(
            error,
            ZempicError::ConfigSchemaVersion {
                found: 2,
                expected: 1,
                ..
            }
        ));
    }

    #[test]
    fn file_config_rejects_a_malformed_selector() {
        let error = parse("schema_version = 1\n\n[[ignore]]\nrule = \"Profile/LTO\"\n")
            .expect_err("a malformed selector is rejected");

        assert!(matches!(error, ZempicError::ConfigSelector { .. }));
        assert!(error.to_string().contains("Profile/LTO"));
    }

    #[test]
    fn file_config_rejects_a_malformed_expiry_date() {
        let error = parse(
            "schema_version = 1\n\n[[ignore]]\nrule = \"profile/lto\"\nexpires = \"2026-13-45\"\n",
        )
        .expect_err("a malformed date is rejected");

        assert!(matches!(error, ZempicError::ConfigDate { .. }));
        assert!(error.to_string().contains("2026-13-45"));
    }

    #[test]
    fn file_config_rejects_an_unknown_report_format() {
        let error = parse("schema_version = 1\n\n[report]\nformat = \"yaml\"\n")
            .expect_err("an unknown format is rejected");

        assert!(matches!(error, ZempicError::ConfigFormat { .. }));
        assert!(error.to_string().contains("yaml"));
    }

    #[test]
    fn merge_replaces_knowledge_base_entries_by_crate_name() {
        let entry = |name: &str, verified_through: &str| {
            format!(
                "[knowledge_base.crates.{name}]\nversions = \">=1.0, <2\"\nverified_through = \"{verified_through}\"\n"
            )
        };
        let base = parse(&format!(
            "schema_version = 1\n\n{}\n{}",
            entry("alpha", "1.0.0"),
            entry("beta", "1.0.0")
        ))
        .unwrap();
        let higher = parse(&format!("schema_version = 1\n\n{}", entry("beta", "1.5.0"))).unwrap();

        let merged = higher.merge_over(base);

        assert_eq!(
            merged.knowledge_base.keys().collect::<Vec<_>>(),
            ["alpha", "beta"]
        );
        assert_eq!(
            merged.knowledge_base["alpha"].verified_through,
            semver::Version::parse("1.0.0").unwrap()
        );
        assert_eq!(
            merged.knowledge_base["beta"].verified_through,
            semver::Version::parse("1.5.0").unwrap()
        );
    }

    #[test]
    fn merge_overrides_scalars_and_unions_ignores() {
        let base = FileConfig {
            schema_version: 1,
            analysis: super::AnalysisSection {
                profile: Some("release".to_owned()),
                include_advisory: Some(true),
                ..Default::default()
            },
            report: Default::default(),
            ignore: vec![ignore_rule("profile/lto")],
            knowledge_base: Default::default(),
        };
        let higher = FileConfig {
            schema_version: 1,
            analysis: super::AnalysisSection {
                profile: Some("small".to_owned()),
                include_advisory: None,
                ..Default::default()
            },
            report: Default::default(),
            ignore: vec![ignore_rule("deps/default-features")],
            knowledge_base: Default::default(),
        };

        let merged = higher.merge_over(base);

        assert_eq!(merged.analysis.profile, Some("small".to_owned()));
        assert_eq!(merged.analysis.include_advisory, Some(true));

        let texts: Vec<&str> = merged
            .ignore
            .iter()
            .map(|entry| entry.text.as_str())
            .collect();
        assert_eq!(texts, ["profile/lto", "deps/default-features"]);
    }

    /// Build an ignore rule from a selector string.
    fn ignore_rule(text: &str) -> IgnoreRule {
        IgnoreRule {
            selector: Selector::parse(text).expect("the selector parses"),
            text: text.to_owned(),
            reason: None,
            packages: None,
            expires: None,
        }
    }

    /// Build a finding from a rule, an optional package and a fixability.
    fn finding(rule: &str, package: Option<&str>, fixability: Fixability) -> Finding {
        Finding {
            rule: RuleId::parse(rule).expect("the rule parses"),
            package: package.map(str::to_owned),
            tier: Tier::Foundational,
            severity: Severity::High,
            magnitude: Magnitude::Large,
            confidence: Confidence::High,
            effort: Effort::Trivial,
            runtime_cost: RuntimeCost::None,
            fixability,
            change: None,
            location: None,
            title: "t".to_owned(),
            detail: "d".to_owned(),
            advice: None,
            tradeoff: "cost".to_owned(),
            references: Vec::new(),
        }
    }

    /// Build an ignore rule with an optional reason, package list and expiry.
    fn ignore(
        selector: &str,
        reason: Option<&str>,
        packages: Option<&[&str]>,
        expires: Option<&str>,
    ) -> IgnoreRule {
        IgnoreRule {
            selector: Selector::parse(selector).expect("the selector parses"),
            text: selector.to_owned(),
            reason: reason.map(str::to_owned),
            packages: packages.map(|list| list.iter().map(|name| (*name).to_owned()).collect()),
            expires: expires.map(|text| CivilDate::parse(text).expect("the date parses")),
        }
    }

    /// The day every filtering case is evaluated against.
    fn today() -> CivilDate {
        CivilDate::parse("2026-10-04").unwrap()
    }

    /// The rule identifiers of the filtered findings, in output order.
    fn retained(filtered: &Filtered) -> Vec<String> {
        filtered
            .findings
            .iter()
            .map(|finding| finding.rule.to_string())
            .collect()
    }

    #[test]
    fn ignore_suppresses_a_matching_finding() {
        let gates = Gates {
            ignore: vec![ignore("profile/lto", Some("reason"), None, None)],
            only: Vec::new(),
            include_advisory: Some(true),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![
                finding("profile/lto", None, Fixability::Safe),
                finding("profile/strip", None, Fixability::Safe),
            ],
            &gates,
            today(),
        );

        assert_eq!(retained(&filtered), ["profile/strip"]);
        assert!(filtered.notices.is_empty());
    }

    #[test]
    fn only_restricts_the_run_to_matching_rules() {
        let gates = Gates {
            ignore: Vec::new(),
            only: vec![Selector::parse("profile").unwrap()],
            include_advisory: Some(true),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![
                finding("profile/lto", None, Fixability::Safe),
                finding("deps/default-features", None, Fixability::Safe),
            ],
            &gates,
            today(),
        );

        assert_eq!(retained(&filtered), ["profile/lto"]);
    }

    #[test]
    fn only_exempts_a_rule_from_an_ignore() {
        let gates = Gates {
            ignore: vec![ignore("profile/lto", Some("reason"), None, None)],
            only: vec![Selector::parse("profile/lto").unwrap()],
            include_advisory: Some(true),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![finding("profile/lto", None, Fixability::Safe)],
            &gates,
            today(),
        );

        assert_eq!(retained(&filtered), ["profile/lto"]);
    }

    #[test]
    fn notices_stay_file_wide_under_package_scope_and_only() {
        let gates = Gates {
            ignore: vec![ignore(
                "profile/lto",
                None,
                Some(&["beta"]),
                Some("2020-01-01"),
            )],
            only: vec![Selector::parse("profile/lto").unwrap()],
            include_advisory: Some(true),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![
                finding("profile/lto", Some("alpha"), Fixability::Safe),
                finding("profile/lto", Some("beta"), Fixability::Safe),
            ],
            &gates,
            today(),
        );

        assert_eq!(retained(&filtered), ["profile/lto", "profile/lto"]);
        assert_eq!(
            filtered.notices,
            vec![
                crate::report::Notice::ExpiredIgnore {
                    selector: "profile/lto".to_owned(),
                    expired: "2020-01-01".to_owned(),
                },
                crate::report::Notice::UndocumentedIgnore {
                    selector: "profile/lto".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn an_active_package_scoped_ignore_yields_to_only() {
        let scoped = || vec![ignore("profile/lto", Some("reason"), Some(&["beta"]), None)];
        let findings = || {
            vec![
                finding("profile/lto", Some("alpha"), Fixability::Safe),
                finding("profile/lto", Some("beta"), Fixability::Safe),
            ]
        };

        let without_only = Gates {
            ignore: scoped(),
            only: Vec::new(),
            include_advisory: Some(true),
            allow_nightly: None,
        };
        let filtered = filter(findings(), &without_only, today());

        assert_eq!(retained(&filtered), ["profile/lto"]);
        assert_eq!(filtered.findings[0].package.as_deref(), Some("alpha"));
        assert!(filtered.notices.is_empty());

        let with_only = Gates {
            ignore: scoped(),
            only: vec![Selector::parse("profile/lto").unwrap()],
            include_advisory: Some(true),
            allow_nightly: None,
        };
        let filtered = filter(findings(), &with_only, today());

        assert_eq!(retained(&filtered), ["profile/lto", "profile/lto"]);
        assert!(filtered.notices.is_empty());
    }

    #[test]
    fn only_exempts_a_rule_from_the_advisory_gate() {
        let gates = Gates {
            ignore: Vec::new(),
            only: vec![Selector::parse("advisory/upx").unwrap()],
            include_advisory: Some(false),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![finding("advisory/upx", None, Fixability::ReportOnly)],
            &gates,
            today(),
        );

        assert_eq!(retained(&filtered), ["advisory/upx"]);
    }

    #[test]
    fn advisory_gate_drops_advisory_and_report_only_findings() {
        let gates = Gates {
            ignore: Vec::new(),
            only: Vec::new(),
            include_advisory: Some(false),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![
                finding("advisory/upx", None, Fixability::ReportOnly),
                finding("config/rustflags-shadowed", None, Fixability::ReportOnly),
                finding("profile/strip", None, Fixability::Safe),
            ],
            &gates,
            today(),
        );

        assert_eq!(retained(&filtered), ["profile/strip"]);
    }

    #[test]
    fn package_scoped_ignore_applies_only_to_that_package() {
        let gates = Gates {
            ignore: vec![ignore(
                "profile/lto",
                Some("reason"),
                Some(&["hot-path"]),
                None,
            )],
            only: Vec::new(),
            include_advisory: Some(true),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![
                finding("profile/lto", None, Fixability::Safe),
                finding("profile/lto", Some("other"), Fixability::Safe),
                finding("profile/lto", Some("hot-path"), Fixability::Safe),
            ],
            &gates,
            today(),
        );

        let packages: Vec<Option<String>> = filtered
            .findings
            .iter()
            .map(|finding| finding.package.clone())
            .collect();

        assert_eq!(packages, [None, Some("other".to_owned())]);
    }

    #[test]
    fn expired_ignore_stops_suppressing_and_warns() {
        let gates = Gates {
            ignore: vec![ignore(
                "profile/lto",
                Some("reason"),
                None,
                Some("2026-10-03"),
            )],
            only: Vec::new(),
            include_advisory: Some(true),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![finding("profile/lto", None, Fixability::Safe)],
            &gates,
            today(),
        );

        assert_eq!(retained(&filtered), ["profile/lto"]);
        assert_eq!(
            filtered.notices,
            vec![Notice::ExpiredIgnore {
                selector: "profile/lto".to_owned(),
                expired: "2026-10-03".to_owned(),
            }]
        );
    }

    #[test]
    fn ignore_expiring_today_still_suppresses() {
        let gates = Gates {
            ignore: vec![ignore(
                "profile/lto",
                Some("reason"),
                None,
                Some("2026-10-04"),
            )],
            only: Vec::new(),
            include_advisory: Some(true),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![finding("profile/lto", None, Fixability::Safe)],
            &gates,
            today(),
        );

        assert!(filtered.findings.is_empty());
        assert!(filtered.notices.is_empty());
    }

    #[test]
    fn undocumented_ignore_produces_a_notice() {
        let gates = Gates {
            ignore: vec![ignore("profile/lto", None, None, None)],
            only: Vec::new(),
            include_advisory: Some(true),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![finding("profile/lto", None, Fixability::Safe)],
            &gates,
            today(),
        );

        assert!(filtered.findings.is_empty());
        assert_eq!(
            filtered.notices,
            vec![Notice::UndocumentedIgnore {
                selector: "profile/lto".to_owned(),
            }]
        );

        let blank = Gates {
            ignore: vec![ignore("profile/lto", Some("   "), None, None)],
            only: Vec::new(),
            include_advisory: Some(true),
            allow_nightly: None,
        };
        let filtered = filter(
            vec![finding("profile/lto", None, Fixability::Safe)],
            &blank,
            today(),
        );

        assert_eq!(
            filtered.notices,
            vec![Notice::UndocumentedIgnore {
                selector: "profile/lto".to_owned(),
            }]
        );
    }

    #[test]
    fn resolve_gates_prefers_the_command_line_advisory_gate() {
        let cli = Gates {
            include_advisory: Some(false),
            ..Gates::default()
        };
        let file = AnalysisSection {
            include_advisory: Some(true),
            ..AnalysisSection::default()
        };

        assert_eq!(resolve_gates(&cli, &file).include_advisory, Some(false));
    }

    #[test]
    fn resolve_gates_falls_back_to_the_file_advisory_gate() {
        let file = AnalysisSection {
            include_advisory: Some(false),
            ..AnalysisSection::default()
        };

        // Plausible bug: resolving `None` to the `true` default before merging,
        // which would let the default override the file's explicit `false`.
        assert_eq!(
            resolve_gates(&Gates::default(), &file).include_advisory,
            Some(false)
        );
    }

    #[test]
    fn resolve_gates_keeps_an_unstated_gate_unstated() {
        let resolved = resolve_gates(&Gates::default(), &AnalysisSection::default());

        assert_eq!(resolved.include_advisory, None);
        assert_eq!(resolved.allow_nightly, None);
    }

    #[test]
    fn resolve_gates_prefers_the_command_line_nightly_gate() {
        let cli = Gates {
            allow_nightly: Some(true),
            ..Gates::default()
        };

        assert_eq!(
            resolve_gates(&cli, &AnalysisSection::default()).allow_nightly,
            Some(true)
        );
    }

    #[test]
    fn resolve_gates_falls_back_to_the_file_nightly_gate() {
        let file = AnalysisSection {
            allow_nightly: Some(true),
            ..AnalysisSection::default()
        };

        // Plausible bug: resolving the advisory gate alone and dropping
        // `allow_nightly`, leaving the file's opt-in unread.
        assert_eq!(
            resolve_gates(&Gates::default(), &file).allow_nightly,
            Some(true)
        );
    }

    #[test]
    fn resolve_gates_takes_ignores_and_only_from_the_command_line() {
        let cli = Gates {
            ignore: vec![ignore("profile/lto", Some("reason"), None, None)],
            only: vec![Selector::parse("profile").unwrap()],
            include_advisory: Some(true),
            allow_nightly: None,
        };

        assert_eq!(resolve_gates(&cli, &AnalysisSection::default()), cli);
    }

    #[test]
    fn an_unstated_advisory_gate_includes_advisory_findings() {
        let filtered = filter(
            vec![
                finding("advisory/upx", None, Fixability::ReportOnly),
                finding("profile/strip", None, Fixability::Safe),
            ],
            &Gates::default(),
            today(),
        );

        // Plausible bug: reading an unstated gate as `false`, which would
        // suppress every report-only finding by default.
        assert_eq!(retained(&filtered), ["advisory/upx", "profile/strip"]);
    }

    #[test]
    fn advisory_gate_leaves_a_report_only_nightly_finding_visible() {
        let gates = Gates {
            ignore: Vec::new(),
            only: Vec::new(),
            include_advisory: Some(false),
            allow_nightly: None,
        };

        // A nightly finding is report-only only because `allow_nightly` is
        // unaccepted, so the advisory gate does not suppress it; a report-only
        // finding in any other category is advisory content and is suppressed.
        let filtered = filter(
            vec![
                finding("nightly/location-detail", None, Fixability::ReportOnly),
                finding(
                    "nightly/panic-immediate-abort",
                    None,
                    Fixability::ReportOnly,
                ),
                finding("config/rustflags-shadowed", None, Fixability::ReportOnly),
            ],
            &gates,
            today(),
        );

        assert_eq!(
            retained(&filtered),
            ["nightly/location-detail", "nightly/panic-immediate-abort"]
        );
    }

    #[test]
    fn only_still_exempts_a_nightly_finding_from_the_advisory_gate() {
        let gates = Gates {
            ignore: vec![ignore(
                "nightly/location-detail",
                Some("reason"),
                None,
                None,
            )],
            only: vec![Selector::parse("nightly/location-detail").unwrap()],
            include_advisory: Some(false),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![
                finding("nightly/location-detail", None, Fixability::ReportOnly),
                finding(
                    "nightly/panic-immediate-abort",
                    None,
                    Fixability::ReportOnly,
                ),
            ],
            &gates,
            today(),
        );

        // The explicit request carries the named rule past the gate and past
        // an ignore that matches it; the other nightly rule lies outside the
        // `--only` scope, which still restricts the run.
        assert_eq!(retained(&filtered), ["nightly/location-detail"]);
    }

    #[test]
    fn a_closed_only_selector_still_hides_an_unnamed_nightly_finding() {
        let gates = Gates {
            ignore: Vec::new(),
            only: vec![Selector::parse("profile/strip").unwrap()],
            include_advisory: Some(false),
            allow_nightly: None,
        };

        let filtered = filter(
            vec![
                finding("nightly/location-detail", None, Fixability::ReportOnly),
                finding("profile/strip", None, Fixability::Safe),
            ],
            &gates,
            today(),
        );

        assert_eq!(retained(&filtered), ["profile/strip"]);
    }
}
