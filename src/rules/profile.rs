//! Rules about cargo profile settings and workspace profile placement.
//!
//! Every compiled artifact is built under exactly one profile, and the profile's
//! settings decide its size and behavior. Cargo fills any setting a manifest
//! leaves unset from a built-in default, so a key absent from `Cargo.toml` still
//! has an effective value; a rule therefore distinguishes an explicitly set
//! value from a default rather than reading the manifest text alone.
//!
//! Cargo honors a `[profile.*]` table only in the workspace root manifest, so a
//! member's own table is inert. The rules that report a profile setting read the
//! root manifest and are gated on the selection producing at least one linked
//! artifact, because a profile does not govern an `rlib` or a procedural macro.
//! The one rule about a member's own table lives here too, since it is about the
//! same table cargo parses and has no other module to live in.

use crate::finding::{Change, Confidence, Effort, Finding, Location, RuleId, RuntimeCost};
use crate::manifest::KeySpan;
use crate::profile::{Provenance, Resolved, Strip};
use crate::rules::{Rule, RuleContext, RuleMetadata};

/// Whether any selected package emits an artifact the profile governs.
///
/// §7.1: a profile governs a linked artifact but not an `rlib` or a procedural
/// macro. Every `profile/*` rule returns nothing when this is false.
fn profile_rules_apply(context: &RuleContext<'_>) -> bool {
    context
        .resolution
        .selected
        .iter()
        .any(|package| package.classification.produces_linked_artifact())
}

/// Reports symbols retained in the linked artifact.
pub struct StripRule;

/// The instance registered for [`StripRule`].
pub const STRIP_RULE: StripRule = StripRule;

impl Rule for StripRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let strip = &context.resolution.profile.strip;
        if strip.value != Strip::None {
            return Vec::new();
        }

        let metadata = registered("profile/strip");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: current_text(strip, render_strip),
                recommended: "\"symbols\"".to_owned(),
            }),
            location_of(strip),
            "Symbols are retained in the linked artifact",
            "The symbol table and the strings it references are kept in the linked \
             artifact even though they are not needed to run it."
                .to_owned(),
            "Stripping removes function names from backtraces and profiler output.",
        )]
    }
}

/// The registered metadata for `id`.
///
/// # Panics
///
/// Panics when `id` is not a registered rule, which is an authoring error the
/// registry's totality test catches.
fn registered(id: &str) -> &'static RuleMetadata {
    let rule = RuleId::parse(id).expect("the rule identifier is well-formed");

    crate::rules::metadata_for(&rule).expect("the rule is registered")
}

/// Build a finding from a registry entry's metadata and the rule-specific fields.
///
/// The constant fields — tier, magnitude, confidence, effort, runtime cost and
/// fixability — are read from the registry entry, so a rule cannot drift from
/// the entry that documents it. Severity is read from
/// [`crate::rules::severity_of`], the single assignment table.
fn build(
    metadata: &'static RuleMetadata,
    package: Option<String>,
    change: Option<Change>,
    location: Option<KeySpan>,
    title: &str,
    detail: String,
    tradeoff: &str,
) -> Finding {
    let severity = crate::rules::severity_of(&metadata.rule_id())
        .expect("every registered rule has a severity assignment");

    Finding {
        rule: metadata.rule_id(),
        package,
        tier: metadata.tier,
        severity,
        magnitude: metadata.magnitude,
        confidence: Confidence::High,
        effort: Effort::Trivial,
        runtime_cost: RuntimeCost::None,
        fixability: metadata.fixability,
        change,
        location: location.map(Location::new),
        title: title.to_owned(),
        detail,
        tradeoff: tradeoff.to_owned(),
        references: Vec::new(),
    }
}

/// The `change.current` text: the rendered value, or `unset` for a cargo default.
fn current_text<T: Copy>(setting: &Resolved<T>, render: fn(T) -> String) -> String {
    if setting.is_cargo_default() {
        "unset".to_owned()
    } else {
        render(setting.value)
    }
}

/// The location of a setting: its key span when a manifest set it, else [`None`].
fn location_of<T>(setting: &Resolved<T>) -> Option<KeySpan> {
    match &setting.provenance {
        Provenance::Explicit { key, .. } | Provenance::Inherited { key, .. } => Some(key.clone()),
        Provenance::CargoDefault => None,
    }
}

/// Render a `strip` value the way a manifest spells it.
fn render_strip(value: Strip) -> String {
    match value {
        Strip::None => "\"none\"".to_owned(),
        Strip::DebugInfo => "\"debuginfo\"".to_owned(),
        Strip::Symbols => "\"symbols\"".to_owned(),
    }
}
