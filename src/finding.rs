//! The vocabulary a finding is expressed in.
//!
//! A rule is identified by a [`RuleId`], a path qualified by an optional
//! [`Dimension`](crate::dimension::Dimension), and selected for suppression or
//! narrowing by a [`Selector`]. Findings carry ordinal classes — tier, severity,
//! magnitude, confidence, effort, runtime cost and fixability — whose declaration
//! order fixes the order a report is sorted in.

/// The report section a finding belongs to.
///
/// The variants are declared in report order, so the derived [`Ord`] orders
/// tiers exactly as the report prints them: a finding's tier is the outermost
/// sort key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// Stripping, debug info and profile presence.
    Foundational,
    /// `opt-level`, `lto`, `codegen-units`, `incremental` and rustflags.
    Compiler,
    /// Panic strategy, assertions and nightly panic and location settings.
    Behavioral,
    /// Duplicates, features, the resolver and curated crate advice.
    Dependencies,
    /// Nightly techniques, post-build packaging and advisory notes.
    Advanced,
}

impl Tier {
    /// Every tier, in report order.
    pub const ALL: [Tier; 5] = [
        Tier::Foundational,
        Tier::Compiler,
        Tier::Behavioral,
        Tier::Dependencies,
        Tier::Advanced,
    ];

    /// The lowercase name this tier is reported under.
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Foundational => "foundational",
            Tier::Compiler => "compiler",
            Tier::Behavioral => "behavioral",
            Tier::Dependencies => "dependencies",
            Tier::Advanced => "advanced",
        }
    }

    /// The uppercase heading this tier carries in the report.
    pub fn title(&self) -> &'static str {
        match self {
            Tier::Foundational => "FOUNDATIONAL",
            Tier::Compiler => "COMPILER",
            Tier::Behavioral => "BEHAVIORAL",
            Tier::Dependencies => "DEPENDENCIES",
            Tier::Advanced => "ADVANCED",
        }
    }
}

/// How much acting on a finding matters.
///
/// The variants are declared least-significant first, so the derived [`Ord`]
/// runs in ascending significance and a threshold comparison such as
/// `severity >= Severity::Low` excludes [`Severity::Info`] structurally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Explanatory; never gates an exit code.
    Info,
    /// Situational, or carrying a tradeoff most projects decline.
    Low,
    /// Worth acting on for most projects in this dimension.
    Medium,
    /// Wrong or costly regardless of project specifics.
    High,
}

impl Severity {
    /// The lowercase name this severity is reported under.
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
        }
    }

    /// The severity a name selects, or [`None`] when no severity carries it.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "info" => Some(Severity::Info),
            "low" => Some(Severity::Low),
            "medium" => Some(Severity::Medium),
            "high" => Some(Severity::High),
            _ => None,
        }
    }
}

/// The editorial size of a finding.
///
/// The variants are declared least-significant first, so the derived [`Ord`]
/// runs in ascending significance. Magnitude is a judgement, not a measurement;
/// the report states as much in its legend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Magnitude {
    /// A correctness finding; magnitude does not apply.
    None,
    /// Included for completeness, or to unlock another rule.
    Negligible,
    /// Real but minor; worth taking when free.
    Small,
    /// Reliably worthwhile, rarely decisive on its own.
    Moderate,
    /// Routinely among the biggest single changes available.
    Large,
}

impl Magnitude {
    /// The lowercase name this magnitude is reported under.
    pub fn as_str(&self) -> &'static str {
        match self {
            Magnitude::None => "none",
            Magnitude::Negligible => "negligible",
            Magnitude::Small => "small",
            Magnitude::Moderate => "moderate",
            Magnitude::Large => "large",
        }
    }
}

/// How certain a rule is of its finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Confidence {
    /// The rule reasons over data that may not reflect the built artifact.
    Low,
    /// The rule's inputs are reliable in most workspaces.
    Medium,
    /// The rule's inputs are complete for the condition it tests.
    High,
}

impl Confidence {
    /// The lowercase name this confidence is reported under.
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::Low => "low",
            Confidence::Medium => "medium",
            Confidence::High => "high",
        }
    }
}

/// How much work applying a finding's recommendation takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effort {
    /// A single setting change.
    Trivial,
    /// A few coordinated changes.
    Moderate,
    /// A change spanning several settings or packages.
    High,
}

impl Effort {
    /// The lowercase name this effort is reported under.
    pub fn as_str(&self) -> &'static str {
        match self {
            Effort::Trivial => "trivial",
            Effort::Moderate => "moderate",
            Effort::High => "high",
        }
    }
}

/// What a finding's recommendation costs at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuntimeCost {
    /// No run-time effect.
    None,
    /// A minor run-time effect.
    Minor,
    /// A notable run-time effect.
    Notable,
    /// A severe run-time effect.
    Severe,
}

impl RuntimeCost {
    /// The lowercase name this runtime cost is reported under.
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeCost::None => "none",
            RuntimeCost::Minor => "minor",
            RuntimeCost::Notable => "notable",
            RuntimeCost::Severe => "severe",
        }
    }
}

/// Whether a finding's recommendation can be applied automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fixability {
    /// Cannot break a build; applied without confirmation.
    Safe,
    /// May break a build or reach beyond the selection; needs confirmation.
    Consequential,
    /// No automated fix exists.
    ReportOnly,
}

impl Fixability {
    /// The lowercase name this fixability is reported under.
    pub fn as_str(&self) -> &'static str {
        match self {
            Fixability::Safe => "safe",
            Fixability::Consequential => "consequential",
            Fixability::ReportOnly => "report-only",
        }
    }

    /// Whether an automated fix exists for this fixability.
    pub fn is_automatable(&self) -> bool {
        match self {
            Fixability::Safe | Fixability::Consequential => true,
            Fixability::ReportOnly => false,
        }
    }
}

/// The canonical identity of a rule: a path and an optional dimension.
///
/// A rule carries a dimension qualifier only when its recommendation differs
/// per dimension. The fields are private so that [`RuleId::parse`] is the only
/// constructor and every identifier in the crate is known to satisfy the
/// grammar. Identifiers are part of the stability surface: renaming one breaks
/// existing ignore lists.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId {
    path: Vec<String>,
    dimension: Option<crate::dimension::Dimension>,
}

impl RuleId {
    /// Parse a canonical rule identifier.
    ///
    /// # Errors
    ///
    /// Returns a [`RuleIdError`] naming the defect when `text` is empty, has an
    /// empty, ill-formed or repeated position, names an unknown dimension, or
    /// carries more than one dimension qualifier.
    pub fn parse(text: &str) -> std::result::Result<Self, RuleIdError> {
        let trimmed = text.trim();

        if trimmed.is_empty() {
            return Err(RuleIdError::Empty);
        }

        let (path_text, dimension) = split_dimension(trimmed)?;
        let path = parse_path(path_text, text)?;

        Ok(RuleId { path, dimension })
    }

    /// The path segments, unqualified by any dimension.
    pub fn path(&self) -> &[String] {
        &self.path
    }

    /// The dimension qualifier, or [`None`] when the rule is dimension-neutral.
    pub fn dimension(&self) -> Option<crate::dimension::Dimension> {
        self.dimension
    }

    /// The path segments joined with `/`.
    pub fn path_text(&self) -> String {
        self.path.join("/")
    }

    /// The first path segment, naming the rule's category.
    pub fn category(&self) -> &str {
        self.path
            .first()
            .map(String::as_str)
            .expect("a parsed identifier always carries at least one segment")
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.path_text())?;

        if let Some(dimension) = self.dimension {
            formatter.write_str(":")?;
            formatter.write_str(dimension.as_str())?;
        }

        Ok(())
    }
}

/// Why a rule identifier could not be parsed.
#[derive(Debug, thiserror::Error)]
pub enum RuleIdError {
    /// The input is empty or whitespace-only.
    #[error("rule identifier is empty")]
    Empty,

    /// The path contains an empty segment, including a leading, trailing or
    /// doubled `/`.
    #[error("rule identifier `{text}` contains an empty segment")]
    EmptySegment {
        /// The offending input.
        text: String,
    },

    /// A path segment does not match the `[a-z0-9][a-z0-9-]*` grammar.
    #[error("rule identifier segment `{segment}` in `{text}` is not lowercase alphanumeric")]
    InvalidSegment {
        /// The offending input.
        text: String,
        /// The segment that did not match the grammar.
        segment: String,
    },

    /// A dimension qualifier names no implemented dimension.
    #[error("rule identifier `{text}` names unknown dimension `{name}`")]
    UnknownDimension {
        /// The offending input.
        text: String,
        /// The dimension name that was not recognized.
        name: String,
    },

    /// The identifier carries more than one dimension qualifier.
    #[error("rule identifier `{text}` carries more than one dimension")]
    MultipleDimensions {
        /// The offending input.
        text: String,
    },
}

/// Split an identifier into its path text and its optional dimension.
///
/// A canonical identifier carries at most one dimension; a comma list is legal
/// only in a selector. Anything after a second `:` or any `,` is therefore a
/// defect rather than a dimension.
fn split_dimension(
    text: &str,
) -> std::result::Result<(&str, Option<crate::dimension::Dimension>), RuleIdError> {
    let Some((path, qualifier)) = text.split_once(':') else {
        return Ok((text, None));
    };

    if qualifier.contains(':') || qualifier.contains(',') {
        return Err(RuleIdError::MultipleDimensions {
            text: text.to_owned(),
        });
    }

    let dimension = crate::dimension::Dimension::from_name(qualifier).ok_or_else(|| {
        RuleIdError::UnknownDimension {
            text: text.to_owned(),
            name: qualifier.to_owned(),
        }
    })?;

    Ok((path, Some(dimension)))
}

/// Validate and split a path into its segments.
fn parse_path(path_text: &str, original: &str) -> std::result::Result<Vec<String>, RuleIdError> {
    let mut segments = Vec::new();

    for segment in path_text.split('/') {
        if segment.is_empty() {
            return Err(RuleIdError::EmptySegment {
                text: original.to_owned(),
            });
        }

        if !segment_is_valid(segment) {
            return Err(RuleIdError::InvalidSegment {
                text: original.to_owned(),
                segment: segment.to_owned(),
            });
        }

        segments.push(segment.to_owned());
    }

    Ok(segments)
}

/// Whether `segment` matches the `[a-z0-9][a-z0-9-]*` grammar.
fn segment_is_valid(segment: &str) -> bool {
    let mut characters = segment.chars();

    let Some(first) = characters.next() else {
        return false;
    };

    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }

    characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    })
}

#[cfg(test)]
mod tests {
    use super::{Magnitude, RuleId, RuleIdError, Severity, Tier};
    use crate::dimension::Dimension;

    #[test]
    fn rule_id_parses_a_dimension_neutral_path() {
        let id = RuleId::parse("workspace/ignored-member-profile").expect("a neutral path parses");

        assert_eq!(id.path(), ["workspace", "ignored-member-profile"]);
        assert_eq!(id.dimension(), None);
        assert_eq!(id.path_text(), "workspace/ignored-member-profile");
        assert_eq!(id.category(), "workspace");
    }

    #[test]
    fn rule_id_parses_a_dimension_qualified_path() {
        let id = RuleId::parse("profile/opt-level:size").expect("a qualified path parses");

        assert_eq!(id.path(), ["profile", "opt-level"]);
        assert_eq!(id.dimension(), Some(Dimension::Size));
        assert_eq!(id.path_text(), "profile/opt-level");
        assert_eq!(id.category(), "profile");
        assert_eq!(id.to_string(), "profile/opt-level:size");
    }

    #[test]
    fn rule_id_round_trips_through_display() {
        for text in [
            "profile/strip",
            "profile/opt-level:size",
            "workspace/ignored-member-profile",
            "deps/build-only-note",
        ] {
            let id = RuleId::parse(text).expect("a canonical identifier parses");

            assert_eq!(id.to_string(), text);
        }
    }

    #[test]
    fn rule_id_rejects_malformed_input() {
        let assert_variant = |text: &str, matches: fn(&RuleIdError) -> bool| {
            let error = RuleId::parse(text).expect_err("a malformed identifier is rejected");

            assert!(matches(&error), "`{text}` gave {error:?}");
        };

        for text in ["", "   "] {
            assert_variant(text, |error| matches!(error, RuleIdError::Empty));
        }

        for text in ["/profile", "profile/", "profile//strip"] {
            assert_variant(text, |error| {
                matches!(error, RuleIdError::EmptySegment { .. })
            });
        }

        for text in [
            "Profile",
            "profile/-x",
            "profile/opt_level",
            "profile/opt.level",
        ] {
            assert_variant(text, |error| {
                matches!(error, RuleIdError::InvalidSegment { .. })
            });
        }

        for text in ["profile:bogus", "profile:"] {
            assert_variant(text, |error| {
                matches!(error, RuleIdError::UnknownDimension { .. })
            });
        }

        assert_variant("profile/opt-level:size,size", |error| {
            matches!(error, RuleIdError::MultipleDimensions { .. })
        });
    }

    #[test]
    fn tier_order_is_report_order() {
        assert_eq!(
            Tier::ALL,
            [
                Tier::Foundational,
                Tier::Compiler,
                Tier::Behavioral,
                Tier::Dependencies,
                Tier::Advanced,
            ]
        );
        assert!(Tier::Foundational < Tier::Compiler);
        assert!(Tier::Compiler < Tier::Behavioral);
        assert!(Tier::Behavioral < Tier::Dependencies);
        assert!(Tier::Dependencies < Tier::Advanced);
        assert_eq!(Tier::Foundational.as_str(), "foundational");
        assert_eq!(Tier::Advanced.title(), "ADVANCED");
    }

    #[test]
    fn severity_order_is_ascending_significance() {
        assert!(Severity::Info < Severity::Low);
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert_eq!(Severity::from_name("info"), Some(Severity::Info));
        assert_eq!(Severity::from_name("high"), Some(Severity::High));
        assert_eq!(Severity::from_name("bogus"), None);
    }

    #[test]
    fn magnitude_order_is_ascending_significance() {
        assert!(Magnitude::None < Magnitude::Negligible);
        assert!(Magnitude::Negligible < Magnitude::Small);
        assert!(Magnitude::Small < Magnitude::Moderate);
        assert!(Magnitude::Moderate < Magnitude::Large);
        assert_eq!(Magnitude::Large.as_str(), "large");
    }
}
