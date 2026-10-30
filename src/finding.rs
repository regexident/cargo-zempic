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

    /// The heading a Markdown report carries for this tier.
    pub fn heading(&self) -> &'static str {
        match self {
            Tier::Foundational => "Foundational",
            Tier::Compiler => "Compiler",
            Tier::Behavioral => "Behavioral",
            Tier::Dependencies => "Dependencies",
            Tier::Advanced => "Advanced",
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

    /// The lowercase name a report states for this fixability, if any.
    ///
    /// `None` for [`Fixability::ReportOnly`]: a report naming it would say only
    /// that there is no automated fix, which the absence of a fixability states
    /// already.
    pub fn reported(&self) -> Option<&'static str> {
        match self {
            Fixability::ReportOnly => None,
            Fixability::Safe | Fixability::Consequential => Some(self.as_str()),
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
    validate_path(path_text).map_err(|defect| match defect {
        PathDefect::EmptySegment => RuleIdError::EmptySegment {
            text: original.to_owned(),
        },
        PathDefect::InvalidSegment { segment } => RuleIdError::InvalidSegment {
            text: original.to_owned(),
            segment,
        },
    })
}

/// The defect a path can carry, shared by identifiers and selectors.
enum PathDefect {
    /// A leading, trailing or doubled `/` produced an empty segment.
    EmptySegment,
    /// A segment does not match the `[a-z0-9][a-z0-9-]*` grammar.
    InvalidSegment { segment: String },
}

/// Split `path_text` on `/` and validate every segment.
///
/// A path is a sequence of one or more non-empty segments, so this is the one
/// definition of §4.2's `segment` production used by both [`RuleId`] and
/// [`Selector`]; a caller may pass an empty text for a selector's empty path.
fn validate_path(path_text: &str) -> std::result::Result<Vec<String>, PathDefect> {
    let mut segments = Vec::new();

    for segment in path_text.split('/') {
        if segment.is_empty() {
            return Err(PathDefect::EmptySegment);
        }

        if !segment_is_valid(segment) {
            return Err(PathDefect::InvalidSegment {
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

/// A pattern selecting rules for suppression or narrowing.
///
/// A selector extends a rule identifier: its path may be empty, matching every
/// category, and its dimension clause may name several dimensions. An empty
/// `dimensions` means any dimension; an empty `path` matches every path. A
/// canonical rule identifier is itself a legal selector that matches the rule
/// it names, so a full identifier is accepted wherever a selector is taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    path: Vec<String>,
    dimensions: Vec<crate::dimension::Dimension>,
}

impl Selector {
    /// Parse a selector.
    ///
    /// # Errors
    ///
    /// Returns a [`SelectorError`] naming the defect when `text` is empty, has
    /// an empty or ill-formed path segment, names an unknown dimension, carries
    /// more than one dimension qualifier, or has an empty dimension list.
    pub fn parse(text: &str) -> std::result::Result<Self, SelectorError> {
        let trimmed = text.trim();

        if trimmed.is_empty() {
            return Err(SelectorError::Empty);
        }

        let (path_text, dimension_text) = split_qualifier(trimmed)?;

        let path = if path_text.is_empty() {
            Vec::new()
        } else {
            validate_path(path_text).map_err(|defect| match defect {
                PathDefect::EmptySegment => SelectorError::EmptySegment {
                    text: text.to_owned(),
                },
                PathDefect::InvalidSegment { segment } => SelectorError::InvalidSegment {
                    text: text.to_owned(),
                    segment,
                },
            })?
        };

        let dimensions = match dimension_text {
            None => Vec::new(),
            Some(list) => parse_dimensions(list, text)?,
        };

        Ok(Selector { path, dimensions })
    }

    /// Whether this selector matches `id`.
    ///
    /// The path matches when it is a segment-wise prefix of the identifier's
    /// path, and the dimension clause matches when it is empty or the
    /// identifier's dimension is one the clause names.
    pub fn matches(&self, id: &RuleId) -> bool {
        let path_matches = self.path.len() <= id.path().len()
            && self
                .path
                .iter()
                .zip(id.path())
                .all(|(selected, actual)| selected == actual);

        if !path_matches {
            return false;
        }

        self.dimensions.is_empty()
            || id
                .dimension()
                .is_some_and(|dimension| self.dimensions.contains(&dimension))
    }

    /// The canonical spelling of this selector.
    pub fn text(&self) -> String {
        let mut text = self.path.join("/");

        if !self.dimensions.is_empty() {
            text.push(':');

            let names: Vec<&str> = self
                .dimensions
                .iter()
                .map(|dimension| dimension.as_str())
                .collect();

            text.push_str(&names.join(","));
        }

        text
    }
}

impl std::fmt::Display for Selector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.text())
    }
}

/// Why a selector could not be parsed.
#[derive(Debug, thiserror::Error)]
pub enum SelectorError {
    /// The input is empty or whitespace-only.
    #[error("selector is empty")]
    Empty,

    /// The path contains an empty segment, including a leading, trailing or
    /// doubled `/`.
    #[error("selector `{text}` contains an empty segment")]
    EmptySegment {
        /// The offending input.
        text: String,
    },

    /// A path segment does not match the `[a-z0-9][a-z0-9-]*` grammar.
    #[error("selector segment `{segment}` in `{text}` is not lowercase alphanumeric")]
    InvalidSegment {
        /// The offending input.
        text: String,
        /// The segment that did not match the grammar.
        segment: String,
    },

    /// A dimension qualifier names no implemented dimension.
    #[error("selector `{text}` names unknown dimension `{name}`")]
    UnknownDimension {
        /// The offending input.
        text: String,
        /// The dimension name that was not recognized.
        name: String,
    },

    /// The selector carries more than one dimension qualifier.
    #[error("selector `{text}` carries more than one dimension qualifier")]
    MultipleQualifiers {
        /// The offending input.
        text: String,
    },

    /// The dimension list after `:` is empty or contains an empty element.
    #[error("selector `{text}` has an empty dimension list")]
    EmptyDimensionList {
        /// The offending input.
        text: String,
    },
}

/// Split a selector into its path text and its optional dimension list.
///
/// The dimension list is the text after the first `:`; a second `:` is a
/// defect rather than part of any dimension name.
fn split_qualifier(text: &str) -> std::result::Result<(&str, Option<&str>), SelectorError> {
    let Some((path, list)) = text.split_once(':') else {
        return Ok((text, None));
    };

    if list.contains(':') {
        return Err(SelectorError::MultipleQualifiers {
            text: text.to_owned(),
        });
    }

    Ok((path, Some(list)))
}

/// Parse a comma-separated dimension list into dimensions.
fn parse_dimensions(
    list: &str,
    original: &str,
) -> std::result::Result<Vec<crate::dimension::Dimension>, SelectorError> {
    if list.is_empty() {
        return Err(SelectorError::EmptyDimensionList {
            text: original.to_owned(),
        });
    }

    let mut dimensions = Vec::new();

    for name in list.split(',') {
        if name.is_empty() {
            return Err(SelectorError::EmptyDimensionList {
                text: original.to_owned(),
            });
        }

        let dimension = crate::dimension::Dimension::from_name(name).ok_or_else(|| {
            SelectorError::UnknownDimension {
                text: original.to_owned(),
                name: name.to_owned(),
            }
        })?;

        dimensions.push(dimension);
    }

    Ok(dimensions)
}

/// The manifest position a finding is about.
///
/// Wraps the [`KeySpan`](crate::manifest::KeySpan) a manifest reader produces;
/// the path it reports is absolute, and relativizing it to the workspace root is
/// a rendering concern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    /// The manifest key's location.
    pub span: crate::manifest::KeySpan,
}

impl Location {
    /// Wrap a key span as a finding location.
    pub fn new(span: crate::manifest::KeySpan) -> Self {
        Self { span }
    }
}

/// The change a finding recommends to one setting.
///
/// A finding distinguishes three states, and the report must not conflate them:
/// explicitly set to the recommended value, explicitly set to something else, or
/// unset and falling back to a cargo default. The first produces no finding at
/// all, so a [`Change`] expresses the remaining two: [`current`](Self::current)
/// holds the rendered value for an explicit setting, and the literal `unset` for
/// a cargo default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The setting's current value, or the literal `unset` when no manifest sets
    /// it and a cargo default applies.
    pub current: String,
    /// The value the finding recommends.
    pub recommended: String,
}

/// A single observation about a workspace, with the cost of acting on it.
///
/// `tradeoff` is mandatory and never empty: the tool reports what it knows and
/// tags the cost rather than deciding which costs are acceptable. The predicate
/// that enforces the invariant is applied where findings enter a report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The rule that produced this finding.
    pub rule: RuleId,
    /// The package the finding is about, or [`None`] when it is about the
    /// workspace itself.
    pub package: Option<String>,
    /// The report section the finding belongs to.
    pub tier: Tier,
    /// How much acting on the finding matters.
    pub severity: Severity,
    /// The editorial size of the finding.
    pub magnitude: Magnitude,
    /// How certain the rule is of the finding.
    pub confidence: Confidence,
    /// How much work the recommendation takes.
    pub effort: Effort,
    /// What the recommendation costs at run time.
    pub runtime_cost: RuntimeCost,
    /// Whether the recommendation can be applied automatically.
    pub fixability: Fixability,
    /// The change the finding recommends to a setting, when it recommends one.
    pub change: Option<Change>,
    /// Where the finding is, when it points at a manifest key.
    pub location: Option<Location>,
    /// The finding's headline.
    pub title: String,
    /// The finding's explanation.
    pub detail: String,
    /// A knowledge-base recommendation about the finding's subject, when one applies.
    pub advice: Option<String>,
    /// The cost of acting on the finding. Mandatory and never empty.
    pub tradeoff: String,
    /// Links to sources, carrying no size figures.
    pub references: Vec<String>,
}

impl Finding {
    /// Whether the finding belongs to the advisory category or is report-only.
    ///
    /// This covers both categories the advisory gate names: a rule under the
    /// `advisory` path, and any finding whose fixability is
    /// [`Fixability::ReportOnly`]. A high-severity report-only finding is
    /// therefore advisory by this predicate.
    pub fn is_advisory(&self) -> bool {
        self.rule.category() == "advisory" || self.fixability == Fixability::ReportOnly
    }

    /// Whether an automated fix exists for the finding.
    pub fn is_automatable(&self) -> bool {
        self.fixability.is_automatable()
    }

    /// Whether the finding states a tradeoff.
    ///
    /// A tradeoff of only whitespace is treated as absent, so a finding cannot
    /// satisfy the mandatory-tradeoff invariant with a blank line.
    pub fn tradeoff_is_present(&self) -> bool {
        self.tradeoff
            .chars()
            .any(|character| !character.is_whitespace())
    }
}

/// Two or more dimensions recommending different settings for one rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// The rule path the recommendations disagree about.
    pub rule_path: String,
    /// One recommendation per dimension in the conflict.
    pub recommendations: Vec<Recommendation>,
}

/// One dimension's recommendation within a [`Conflict`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recommendation {
    /// The dimension offering the recommendation.
    pub dimension: crate::dimension::Dimension,
    /// The setting the dimension recommends.
    pub setting: String,
}

#[cfg(test)]
mod tests {
    use super::{
        Change, Confidence, Effort, Finding, Fixability, Magnitude, RuleId, RuleIdError,
        RuntimeCost, Selector, SelectorError, Severity, Tier,
    };
    use crate::dimension::Dimension;

    #[test]
    fn tier_heading_names_every_tier() {
        assert_eq!(
            Tier::ALL.map(|tier| tier.heading()),
            [
                "Foundational",
                "Compiler",
                "Behavioral",
                "Dependencies",
                "Advanced"
            ]
        );
    }

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

    /// Parse a selector and a rule identifier and report whether they match.
    fn matches(selector: &str, id: &str) -> bool {
        Selector::parse(selector)
            .expect("the selector parses")
            .matches(&RuleId::parse(id).expect("the identifier parses"))
    }

    #[test]
    fn selector_matches_a_single_rule_facet() {
        assert!(matches("profile/opt-level:size", "profile/opt-level:size"));
        assert!(!matches("profile/opt-level:size", "profile/opt-level"));
        assert!(!matches("profile/opt-level:size", "profile/lto"));
    }

    #[test]
    fn selector_without_a_dimension_matches_every_facet() {
        assert!(matches("profile/opt-level", "profile/opt-level:size"));
        assert!(matches("profile/opt-level", "profile/opt-level"));
        assert!(!matches("profile/opt-level", "profile/opt-levels"));
        assert!(!matches("profile/opt-level", "profile/lto"));
    }

    #[test]
    fn selector_matches_a_whole_category() {
        assert!(matches("profile", "profile/strip"));
        assert!(matches("profile", "profile/opt-level:size"));
        assert!(matches("profile", "profile/dependency-opt-level"));
        assert!(!matches("profile", "deps/default-features"));
        assert!(!matches("profile", "config/link-dead-code"));
    }

    #[test]
    fn selector_with_only_a_dimension_matches_across_categories() {
        assert!(matches(":size", "profile/opt-level:size"));
        assert!(!matches(":size", "profile/strip"));
        assert!(!matches(":size", "deps/duplicate-versions"));
    }

    #[test]
    fn selector_path_prefix_is_segment_wise() {
        assert!(!matches("profile", "profiles/strip"));
        assert!(!matches("prof", "profile/strip"));
        assert!(!matches("profile/opt", "profile/opt-level"));
    }

    #[test]
    fn selector_accepts_a_comma_separated_dimension_list() {
        let selector = Selector::parse("profile:size,size").expect("a comma list parses");

        assert!(selector.matches(&RuleId::parse("profile/opt-level:size").unwrap()));
        assert_eq!(selector.text(), "profile:size,size");
    }

    #[test]
    fn selector_rejects_malformed_input() {
        let assert_variant = |text: &str, matches: fn(&SelectorError) -> bool| {
            let error = Selector::parse(text).expect_err("a malformed selector is rejected");

            assert!(matches(&error), "`{text}` gave {error:?}");
        };

        for text in ["", "   "] {
            assert_variant(text, |error| matches!(error, SelectorError::Empty));
        }

        for text in ["profile//strip", "/profile", "profile/"] {
            assert_variant(text, |error| {
                matches!(error, SelectorError::EmptySegment { .. })
            });
        }

        for text in ["Profile", "profile/-x", "profile/opt_level"] {
            assert_variant(text, |error| {
                matches!(error, SelectorError::InvalidSegment { .. })
            });
        }

        for text in ["profile:speed", "profile:bogus"] {
            assert_variant(text, |error| {
                matches!(error, SelectorError::UnknownDimension { .. })
            });
        }

        assert_variant("profile:size:size", |error| {
            matches!(error, SelectorError::MultipleQualifiers { .. })
        });

        for text in [":", "profile:", "profile:size,", "profile:,size"] {
            assert_variant(text, |error| {
                matches!(error, SelectorError::EmptyDimensionList { .. })
            });
        }
    }

    #[test]
    fn every_rule_id_is_a_selector_that_matches_itself() {
        for text in [
            "profile/strip",
            "profile/opt-level:size",
            "workspace/ignored-member-profile",
            "deps/build-only-note",
            "advisory/lib-profile-not-applicable",
        ] {
            let selector = Selector::parse(text).expect("a canonical identifier is a selector");
            let id = RuleId::parse(text).expect("a canonical identifier parses");

            assert!(selector.matches(&id), "`{text}` must match itself");
        }
    }

    /// Build a finding from the fields the advisory predicates read.
    fn sample(rule: &str, fixability: Fixability, tradeoff: &str) -> Finding {
        Finding {
            rule: RuleId::parse(rule).expect("the rule parses"),
            package: None,
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
            tradeoff: tradeoff.to_owned(),
            references: Vec::new(),
        }
    }

    #[test]
    fn finding_is_advisory_by_category_or_by_report_only() {
        assert!(sample("advisory/upx", Fixability::ReportOnly, "x").is_advisory());
        assert!(sample("advisory/no-std", Fixability::Safe, "x").is_advisory());
        assert!(sample("config/rustflags-shadowed", Fixability::ReportOnly, "x").is_advisory());
        assert!(!sample("profile/strip", Fixability::Safe, "x").is_advisory());
        assert!(!sample("profile/lto", Fixability::Consequential, "x").is_advisory());
    }

    #[test]
    fn finding_is_automatable_for_safe_and_consequential() {
        assert!(sample("profile/strip", Fixability::Safe, "x").is_automatable());
        assert!(sample("profile/lto", Fixability::Consequential, "x").is_automatable());
        assert!(!sample("config/rustflags-shadowed", Fixability::ReportOnly, "x").is_automatable());
    }

    #[test]
    fn finding_tradeoff_presence_rejects_blank_text() {
        assert!(!sample("profile/strip", Fixability::Safe, "").tradeoff_is_present());
        assert!(!sample("profile/strip", Fixability::Safe, "   ").tradeoff_is_present());
        assert!(!sample("profile/strip", Fixability::Safe, "\n\t").tradeoff_is_present());
        assert!(sample("profile/strip", Fixability::Safe, "link times rise").tradeoff_is_present());
    }

    #[test]
    fn change_records_the_current_and_recommended_values() {
        let change = Change {
            current: "unset".to_owned(),
            recommended: "\"symbols\"".to_owned(),
        };

        assert_eq!(change.current, "unset");
        assert_eq!(change.recommended, "\"symbols\"");

        let mut with_change = sample("profile/strip", Fixability::Safe, "x");
        with_change.change = Some(change.clone());

        assert_eq!(
            with_change
                .change
                .as_ref()
                .map(|change| change.recommended.as_str()),
            Some("\"symbols\"")
        );

        let without_change = sample("profile/strip", Fixability::Safe, "x");
        assert_ne!(with_change, without_change);
    }
}
