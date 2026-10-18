//! Rules about packages that produce no linked artifact.
//!
//! A library's cost lands on every consumer: each dependency it declares is
//! compiled into every binary that links it, and the library's own author does
//! not see that binary. The rules here apply only to packages whose targets
//! produce no linked artifact, and they read the package's manifest surface
//! rather than a profile, because no profile governs the library's own build.

use crate::finding::{Confidence, Effort, Finding, RuleId, RuntimeCost};
use crate::rules::{Rule, RuleContext, RuleMetadata};

/// The number of direct normal dependencies at which a library without features
/// is reported.
const FEATURE_GRANULARITY_THRESHOLD: usize = 10;

/// Whether a library declares enough dependencies and too few features.
///
/// Returns `true` when `normal_dependencies` reaches
/// [`FEATURE_GRANULARITY_THRESHOLD`] and `declared_features` is zero, so a
/// consumer has no feature to switch off to avoid compiling them.
fn lacks_feature_granularity(normal_dependencies: usize, declared_features: usize) -> bool {
    normal_dependencies >= FEATURE_GRANULARITY_THRESHOLD && declared_features == 0
}

/// Reports a library with many dependencies and no features.
pub struct NoFeatureGranularityRule;

/// The instance registered for [`NoFeatureGranularityRule`].
pub const NO_FEATURE_GRANULARITY_RULE: NoFeatureGranularityRule = NoFeatureGranularityRule;

impl Rule for NoFeatureGranularityRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("lib/no-feature-granularity");
        let mut findings = Vec::new();

        for package in crate::rules::deps::selected_packages(context) {
            let produces_linked_artifact = context
                .resolution
                .selected
                .iter()
                .find(|selected| selected.manifest_path == package.manifest_path)
                .is_some_and(|selected| selected.classification.produces_linked_artifact());

            if produces_linked_artifact {
                continue;
            }

            // The metadata feature map lists the implicit feature of every
            // optional dependency, so a package whose only features are those
            // counts as having declared some.
            let count = package
                .dependencies
                .iter()
                .filter(|dependency| dependency.kind == cargo_metadata::DependencyKind::Normal)
                .count();

            if !lacks_feature_granularity(count, package.features.len()) {
                continue;
            }

            let name = &package.name;

            findings.push(build(
                metadata,
                Some(name.to_string()),
                &format!("`{name}` offers no features to opt out of its dependencies"),
                format!(
                    "`{name}` declares {count} direct normal dependencies and no features, \
                     so every consumer compiles all of them."
                ),
                "Gating dependencies behind features adds conditional compilation and a \
                 feature matrix to test.",
            ));
        }

        findings
    }
}

/// Reports a library whose `default` feature set enables a heavy optional
/// dependency.
///
/// Every consumer that keeps default features compiles the dependency, whether
/// or not it uses the code that needs it. An optional dependency that the
/// analyzed feature selection does not resolve is skipped, because its version,
/// and so its knowledge-base entry, is unknown.
pub struct HeavyDefaultFeaturesRule;

/// The instance registered for [`HeavyDefaultFeaturesRule`].
pub const HEAVY_DEFAULT_FEATURES_RULE: HeavyDefaultFeaturesRule = HeavyDefaultFeaturesRule;

impl Rule for HeavyDefaultFeaturesRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("lib/heavy-default-features");
        let mut findings = Vec::new();

        for package in crate::rules::deps::selected_packages(context) {
            let produces_linked_artifact = context
                .resolution
                .selected
                .iter()
                .find(|selected| selected.manifest_path == package.manifest_path)
                .is_some_and(|selected| selected.classification.produces_linked_artifact());

            if produces_linked_artifact {
                continue;
            }

            let enabled = crate::rules::deps::expand_features(
                &package.features,
                &crate::rules::deps::optional_dependency_names(package),
                &["default"],
            );

            for key in &enabled.optional_dependencies {
                let Some(dependency) = package.dependencies.iter().find(|dependency| {
                    dependency.optional
                        && dependency.kind == cargo_metadata::DependencyKind::Normal
                        && crate::rules::deps::declared_key(dependency) == *key
                }) else {
                    continue;
                };

                let Some(resolved) =
                    crate::rules::deps::resolved_dependency(context.metadata, package, dependency)
                else {
                    continue;
                };

                let Some(entry) = context
                    .knowledge_base
                    .entry_for(&resolved.name, &resolved.version)
                else {
                    continue;
                };

                let Some(magnitude) = entry.magnitude.filter(|_| entry.is_heavy()) else {
                    continue;
                };

                let name = &package.name;

                let mut detail = format!(
                    "The `default` feature set of `{name}` enables the optional dependency \
                     `{key}`, which resolves to `{}` {} and is marked {} in the knowledge \
                     base, so every consumer that keeps default features compiles it.",
                    resolved.name,
                    resolved.version,
                    magnitude.as_str()
                );

                if entry.is_stale(&resolved.version) {
                    detail.push(' ');
                    detail.push_str(&crate::knowledge::stale_note(
                        &resolved.name,
                        &entry.verified_through,
                        &resolved.version,
                    ));
                }

                let mut finding = build(
                    metadata,
                    Some(name.to_string()),
                    &format!("Default features of `{name}` enable the heavy dependency `{key}`"),
                    detail,
                    "Removing it from the default set is a breaking change for consumers that \
                     rely on it without naming the feature.",
                );

                finding.magnitude = magnitude;
                finding.location = crate::rules::deps::manifest_for(context, package)
                    .and_then(|manifest| manifest.key_span("features.default"))
                    .map(crate::finding::Location::new);

                findings.push(finding);
            }
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
        effort: Effort::Moderate,
        runtime_cost: RuntimeCost::None,
        fixability: metadata.fixability,
        change: None,
        location: None,
        title: title.to_owned(),
        detail,
        tradeoff: tradeoff.to_owned(),
        references: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::lacks_feature_granularity;

    #[test]
    fn feature_granularity_threshold_is_ten_normal_dependencies() {
        assert!(!lacks_feature_granularity(9, 0));
        assert!(lacks_feature_granularity(10, 0));
        assert!(!lacks_feature_granularity(10, 1));
    }
}
