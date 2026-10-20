//! The versioned JSON report document.
//!
//! The document's key set is a stability promise: a consumer may depend on the
//! field names and their order, so the renderer defines its own serialization
//! types rather than deriving `Serialize` on the domain types. That keeps the
//! schema free to change independently of [`Finding`](crate::finding::Finding)
//! and [`Report`](crate::report::Report).
//!
//! `schema_version` is incremented only for a breaking change. Adding a key is
//! a backward-compatible extension and leaves the version in place.

use serde::Serialize;

use crate::report::{MAGNITUDE_LEGEND, Report, Summary};

/// The schema version this document is written to.
pub const SCHEMA_VERSION: u32 = 1;

/// Render `report` as a pretty-printed JSON document terminated by a newline.
///
/// # Errors
///
/// Returns [`ZempicError::ReportSerialize`](crate::ZempicError::ReportSerialize)
/// when the document cannot be serialized.
pub fn render(report: &Report) -> crate::Result<String> {
    let document = Document::from_report(report);

    let mut text = serde_json::to_string_pretty(&document)
        .map_err(|source| crate::ZempicError::ReportSerialize { source })?;

    text.push('\n');

    Ok(text)
}

/// The top-level document, whose field order is the emission order.
#[derive(Serialize)]
struct Document {
    schema_version: u32,
    zempic_version: &'static str,
    kb_version: u32,
    workspace_root: String,
    profile: String,
    dimensions: Vec<&'static str>,
    findings: Vec<FindingObject>,
    conflicts: Vec<ConflictObject>,
    summary: SummaryObject,
    legend: &'static str,
}

impl Document {
    /// Build the document from a report.
    fn from_report(report: &Report) -> Self {
        let root = report.resolution.workspace_root.clone();

        Self {
            schema_version: SCHEMA_VERSION,
            zempic_version: env!("CARGO_PKG_VERSION"),
            kb_version: crate::knowledge::KNOWLEDGE_BASE_VERSION,
            workspace_root: root.clone().into_string(),
            profile: report.resolution.profile.name.clone(),
            dimensions: report
                .resolution
                .dimensions
                .iter()
                .map(|dimension| dimension.as_str())
                .collect(),
            findings: report
                .findings
                .iter()
                .map(|finding| FindingObject::from_finding(finding, &root))
                .collect(),
            conflicts: report
                .conflicts
                .iter()
                .map(ConflictObject::from_conflict)
                .collect(),
            summary: SummaryObject::from_summary(&report.summary),
            legend: MAGNITUDE_LEGEND,
        }
    }
}

/// One finding, whose field order is the emission order.
#[derive(Serialize)]
struct FindingObject {
    rule: String,
    dimension: Option<&'static str>,
    package: Option<String>,
    tier: &'static str,
    severity: &'static str,
    magnitude: &'static str,
    confidence: &'static str,
    effort: &'static str,
    runtime_cost: &'static str,
    fixability: &'static str,
    change: Option<ChangeObject>,
    location: Option<LocationObject>,
    title: String,
    detail: String,
    advice: Option<String>,
    tradeoff: String,
    references: Vec<String>,
}

impl FindingObject {
    /// Build a finding object, splitting the rule path from its dimension and
    /// relativizing the location against `root`.
    fn from_finding(finding: &crate::finding::Finding, root: &camino::Utf8Path) -> Self {
        Self {
            rule: finding.rule.path_text(),
            dimension: finding.rule.dimension().map(|dimension| dimension.as_str()),
            package: finding.package.clone(),
            tier: finding.tier.as_str(),
            severity: finding.severity.as_str(),
            magnitude: finding.magnitude.as_str(),
            confidence: finding.confidence.as_str(),
            effort: finding.effort.as_str(),
            runtime_cost: finding.runtime_cost.as_str(),
            fixability: finding.fixability.as_str(),
            change: finding.change.as_ref().map(ChangeObject::from_change),
            location: finding
                .location
                .as_ref()
                .map(|location| LocationObject::from_location(location, root)),
            title: finding.title.clone(),
            detail: finding.detail.clone(),
            advice: finding.advice.clone(),
            tradeoff: finding.tradeoff.clone(),
            references: finding.references.clone(),
        }
    }
}

/// The change a finding recommends.
#[derive(Serialize)]
struct ChangeObject {
    current: String,
    recommended: String,
}

impl ChangeObject {
    /// Build a change object from the finding's change.
    fn from_change(change: &crate::finding::Change) -> Self {
        Self {
            current: change.current.clone(),
            recommended: change.recommended.clone(),
        }
    }
}

/// The manifest position a finding is about, with the file relative to the
/// workspace root.
#[derive(Serialize)]
struct LocationObject {
    file: String,
    key: String,
    line: usize,
    column: usize,
}

impl LocationObject {
    /// Build a location object, relativizing the file against `root` and
    /// falling back to the absolute path when it lies outside.
    fn from_location(location: &crate::finding::Location, root: &camino::Utf8Path) -> Self {
        let file = location
            .span
            .file
            .strip_prefix(root)
            .map(camino::Utf8Path::to_owned)
            .unwrap_or_else(|_| location.span.file.clone());

        Self {
            file: file.into_string(),
            key: location.span.key.clone(),
            line: location.span.line,
            column: location.span.column,
        }
    }
}

/// A cross-dimension conflict.
#[derive(Serialize)]
struct ConflictObject {
    rule_path: String,
    recommendations: Vec<RecommendationObject>,
}

impl ConflictObject {
    /// Build a conflict object.
    fn from_conflict(conflict: &crate::finding::Conflict) -> Self {
        Self {
            rule_path: conflict.rule_path.clone(),
            recommendations: conflict
                .recommendations
                .iter()
                .map(|recommendation| RecommendationObject {
                    dimension: recommendation.dimension.as_str(),
                    setting: recommendation.setting.clone(),
                })
                .collect(),
        }
    }
}

/// One dimension's recommendation within a conflict.
#[derive(Serialize)]
struct RecommendationObject {
    dimension: &'static str,
    setting: String,
}

/// The report's summary counts.
#[derive(Serialize)]
struct SummaryObject {
    total: usize,
    fixable: usize,
    by_severity: SeverityCountsObject,
    by_magnitude: MagnitudeCountsObject,
}

impl SummaryObject {
    /// Build a summary object. Unlike the human summary, every count is emitted
    /// including zeros, because a machine consumer reads a fixed shape.
    fn from_summary(summary: &Summary) -> Self {
        Self {
            total: summary.total,
            fixable: summary.fixable,
            by_severity: SeverityCountsObject {
                high: summary.by_severity.high,
                medium: summary.by_severity.medium,
                low: summary.by_severity.low,
                info: summary.by_severity.info,
            },
            by_magnitude: MagnitudeCountsObject {
                large: summary.by_magnitude.large,
                moderate: summary.by_magnitude.moderate,
                small: summary.by_magnitude.small,
                negligible: summary.by_magnitude.negligible,
                none: summary.by_magnitude.none,
            },
        }
    }
}

/// The findings per severity.
#[derive(Serialize)]
struct SeverityCountsObject {
    high: usize,
    medium: usize,
    low: usize,
    info: usize,
}

/// The findings per magnitude.
#[derive(Serialize)]
struct MagnitudeCountsObject {
    large: usize,
    moderate: usize,
    small: usize,
    negligible: usize,
    none: usize,
}
