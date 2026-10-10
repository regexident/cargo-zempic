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
}

impl FileConfig {
    /// The configuration with no settings and no ignores.
    pub fn empty() -> Self {
        FileConfig {
            schema_version: SCHEMA_VERSION,
            analysis: AnalysisSection::default(),
            report: ReportSection::default(),
            ignore: Vec::new(),
        }
    }

    /// Parse `text` as the configuration at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::ConfigParse`](crate::ZempicError::ConfigParse)
    /// when the document is malformed or carries an unknown key,
    /// [`ZempicError::ConfigSchemaVersion`](crate::ZempicError::ConfigSchemaVersion)
    /// when `schema_version` is not [`SCHEMA_VERSION`], and the selector, date,
    /// dimension, format or fail-on error for an invalid value, each naming the
    /// file and the offending value.
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

        Ok(FileConfig {
            schema_version: raw.schema_version,
            analysis,
            report,
            ignore,
        })
    }

    /// Merge `self`, the higher-precedence source, over `base`.
    ///
    /// Each [`Some`] scalar of `self` replaces the base's; each [`None`] keeps
    /// the base's. `ignore` entries are the base's followed by `self`'s.
    pub fn merge_over(self, base: Self) -> Self {
        FileConfig {
            schema_version: self.schema_version,
            analysis: self.analysis.merge_over(base.analysis),
            report: self.report.merge_over(base.report),
            ignore: base.ignore.into_iter().chain(self.ignore).collect(),
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gates {
    /// The selectors whose matching findings are suppressed.
    pub ignore: Vec<IgnoreRule>,
    /// The selectors restricting the run to matching rules.
    pub only: Vec<Selector>,
    /// Whether advisory and report-only findings are included.
    pub include_advisory: bool,
}

impl Default for Gates {
    fn default() -> Self {
        Gates {
            ignore: Vec::new(),
            only: Vec::new(),
            include_advisory: true,
        }
    }
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
    use super::{CivilDate, FileConfig, IgnoreRule};
    use crate::ZempicError;
    use crate::dimension::Dimension;
    use crate::finding::Selector;
    use crate::report::{FailOn, Format};
    use camino::Utf8Path;

    /// The §6.2 schema with its knowledge-base block omitted.
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
    fn file_config_rejects_a_knowledge_base_table() {
        let error =
            parse("schema_version = 1\n\n[knowledge_base.crates.demo]\nversions = \">=1.0\"\n")
                .expect_err("a knowledge-base table is rejected");

        assert!(matches!(error, ZempicError::ConfigParse { .. }));
        assert!(error.to_string().contains("knowledge_base"));
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
}
