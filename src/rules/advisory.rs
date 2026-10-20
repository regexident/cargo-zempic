//! Notes that explain the analysis rather than recommend a change.
//!
//! An advisory rule reports no setting to edit. It tells the reader why a part
//! of the analysis did not run or what the result does not cover, so that a
//! missing finding is not mistaken for a clean bill of health. Every advisory
//! finding is informational and carries no magnitude.

use crate::finding::{Confidence, Effort, Finding, RuleId, RuntimeCost};
use crate::rules::{Rule, RuleContext, RuleMetadata};

/// Explains why profile rules skip a package that produces no linked artifact.
///
/// A library's code is compiled under the profile of whichever binary links it,
/// so the analyzed profile does not shape the library's own artifact. The note
/// is emitted once per selected package, so a mixed selection explains each
/// skipped package instead of staying silent about it.
pub struct LibProfileNotApplicableRule;

/// The instance registered for [`LibProfileNotApplicableRule`].
pub const LIB_PROFILE_NOT_APPLICABLE_RULE: LibProfileNotApplicableRule =
    LibProfileNotApplicableRule;

impl Rule for LibProfileNotApplicableRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("advisory/lib-profile-not-applicable");
        let profile = &context.resolution.profile.name;
        let mut findings = Vec::new();

        for package in &context.resolution.selected {
            if package.classification.produces_linked_artifact() {
                continue;
            }

            let name = &package.name;

            findings.push(build(
                metadata,
                Some(name.clone()),
                &format!("The `{profile}` profile does not shape `{name}`'s artifact"),
                format!(
                    "`{name}` produces no linked artifact. A library's code is compiled \
                     under the profile of whichever binary links it, so the profile and \
                     rustflags rules were not run for it. The dependency and library \
                     rules still apply."
                ),
                "None; the note explains why no profile or rustflags rule reports on this package.",
            ));
        }

        findings
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
/// The constant fields are read from the registry entry, so a rule cannot drift
/// from the entry that documents it. Severity is read from
/// [`crate::rules::severity_of`], the single assignment table.
fn build(
    metadata: &'static RuleMetadata,
    package: Option<String>,
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
        change: None,
        location: None,
        title: title.to_owned(),
        detail,
        advice: None,
        tradeoff: tradeoff.to_owned(),
        references: Vec::new(),
    }
}
