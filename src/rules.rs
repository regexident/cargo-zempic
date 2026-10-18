//! The rule registry and the single table that assigns every rule its severity.

pub mod advisory;
pub mod config;
pub mod deps;
pub mod library;
pub mod profile;

use crate::finding::{RuleId, Selector, Severity};

/// The inputs a rule reads when it evaluates a workspace.
///
/// A rule receives the dimension it answers, the resolved workspace, the
/// workspace root manifest and the selected members' parsed manifests. The root
/// manifest is kept separate from the member manifests because cargo honors a
/// `[profile.*]` table only in the root, so a rule that cares about that
/// distinction can tell the two apart.
pub struct RuleContext<'a> {
    /// The optimization objective the evaluation answers.
    pub dimension: crate::dimension::Dimension,
    /// The workspace resolved for the run.
    pub resolution: &'a crate::Resolution,
    /// The `cargo metadata` output the workspace was resolved from, filtered to
    /// the resolved target platform.
    pub metadata: &'a cargo_metadata::Metadata,
    /// The embedded knowledge base extended by the configuration's entries.
    pub knowledge_base: &'a crate::knowledge::KnowledgeBase,
    /// The parsed workspace root manifest.
    pub root_manifest: &'a crate::manifest::ManifestDocument,
    /// The parsed manifests of the selected members, excluding the root
    /// package's own manifest, paired with each member's name in selection
    /// order.
    pub member_manifests: &'a [(String, crate::manifest::ManifestDocument)],
    /// The target triple the resolution runs for.
    pub target_triple: &'a str,
    /// The resolved cargo configuration rustflags for every selected package,
    /// paired with each package's name in selection order.
    ///
    /// Unlike [`member_manifests`](Self::member_manifests), this list includes
    /// the root package of a non-virtual workspace: a cargo configuration
    /// chain is resolved per package directory, and the root package's
    /// directory is one like any other, so excluding it would silently drop
    /// the common case of a single-package workspace.
    pub cargo_configs: &'a [(String, crate::cargo_config::ResolvedRustflags)],
}

/// A rule's behavior: the findings one evaluation produces for a workspace.
///
/// A rule carries no state beyond its identity, so every instance is a unit
/// struct with a `const`. [`RuleMetadata::rule`] holds the instance, and the
/// registry's totality test enumerates it together with the metadata so a rule
/// cannot be documented without being evaluated.
///
/// The trait requires [`Sync`] because [`registry`] is a `const` slice of
/// [`RuleMetadata`] and a `const` demands it.
pub trait Rule: Sync {
    /// Evaluate the rule against `context`, producing zero or more findings.
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<crate::finding::Finding>;
}

/// The metadata every rule registers: what it is and how to run it.
///
/// The struct carries no severity field. Severity is assigned in exactly one
/// place, [`severity_assignments`], so a rule's severity cannot drift from the
/// table the whole project is reviewed against.
#[derive(Clone, Copy)]
pub struct RuleMetadata {
    /// The rule's behavior, evaluated once per selected dimension.
    pub rule: &'static dyn Rule,
    /// The rule's canonical identifier.
    pub id: &'static str,
    /// The report section the rule's findings belong to.
    pub tier: crate::finding::Tier,
    /// The editorial size of the rule's findings.
    pub magnitude: crate::finding::Magnitude,
    /// Whether the rule's findings can be applied automatically.
    pub fixability: crate::finding::Fixability,
    /// A one-line description of the rule.
    pub summary: &'static str,
    /// The rule's documentation, embedded from its doc file.
    pub documentation: &'static str,
}

impl std::fmt::Debug for RuleMetadata {
    /// Format the metadata without the behavior, which is not printable.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuleMetadata")
            .field("id", &self.id)
            .field("tier", &self.tier)
            .field("magnitude", &self.magnitude)
            .field("fixability", &self.fixability)
            .finish_non_exhaustive()
    }
}

impl PartialEq for RuleMetadata {
    /// Two entries are equal when they name the same rule.
    ///
    /// An identifier uniquely names a registry entry, so comparing it alone is
    /// the whole of a rule's identity.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RuleMetadata {}

impl RuleMetadata {
    /// The rule's identifier, parsed.
    ///
    /// A registered identifier that does not parse is an authoring error the
    /// registry's own test catches before any user can reach it.
    ///
    /// # Panics
    ///
    /// Panics with a message naming the identifier when it is not a well-formed
    /// [`RuleId`].
    pub fn rule_id(&self) -> RuleId {
        RuleId::parse(self.id)
            .unwrap_or_else(|error| panic!("registered rule `{}` is malformed: {error}", self.id))
    }
}

/// Every rule the crate knows, as metadata.
///
/// The registry is the set a run evaluates and the set the documentation and
/// severity tests enumerate. Each entry's severity is looked up in
/// [`severity_assignments`] rather than stored on the entry.
pub fn registry() -> &'static [RuleMetadata] {
    const REGISTRY: &[RuleMetadata] = &[
        RuleMetadata {
            rule: &crate::rules::advisory::LIB_PROFILE_NOT_APPLICABLE_RULE,
            id: "advisory/lib-profile-not-applicable",
            tier: crate::finding::Tier::Advanced,
            magnitude: crate::finding::Magnitude::None,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Explaining why profile rules skip a package with no linked artifact",
            documentation: include_str!("rules/docs/advisory/lib-profile-not-applicable.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::STRIP_RULE,
            id: "profile/strip",
            tier: crate::finding::Tier::Foundational,
            magnitude: crate::finding::Magnitude::Large,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting a profile that retains symbols in the linked artifact",
            documentation: include_str!("rules/docs/profile/strip.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::DEBUG_INFO_RULE,
            id: "profile/debug-info",
            tier: crate::finding::Tier::Foundational,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting retained debug information in the linked artifact",
            documentation: include_str!("rules/docs/profile/debug-info.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::MISSING_CONFIG_RULE,
            id: "profile/missing-config",
            tier: crate::finding::Tier::Foundational,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Reporting a profile that sets nothing explicitly",
            documentation: include_str!("rules/docs/profile/missing-config.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::OPT_LEVEL_RULE,
            id: "profile/opt-level:size",
            tier: crate::finding::Tier::Compiler,
            magnitude: crate::finding::Magnitude::Large,
            fixability: crate::finding::Fixability::Consequential,
            summary: "Reporting an optimization level that does not target size",
            documentation: include_str!("rules/docs/profile/opt-level.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::LTO_RULE,
            id: "profile/lto",
            tier: crate::finding::Tier::Compiler,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::Consequential,
            summary: "Reporting partial link-time optimization",
            documentation: include_str!("rules/docs/profile/lto.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::CODEGEN_UNITS_RULE,
            id: "profile/codegen-units",
            tier: crate::finding::Tier::Compiler,
            magnitude: crate::finding::Magnitude::Negligible,
            fixability: crate::finding::Fixability::Consequential,
            summary: "Reporting more than one code generation unit",
            documentation: include_str!("rules/docs/profile/codegen-units.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::INCREMENTAL_RULE,
            id: "profile/incremental",
            tier: crate::finding::Tier::Compiler,
            magnitude: crate::finding::Magnitude::Small,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting incremental compilation in a release-like profile",
            documentation: include_str!("rules/docs/profile/incremental.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::DEPENDENCY_OPT_LEVEL_RULE,
            id: "profile/dependency-opt-level",
            tier: crate::finding::Tier::Compiler,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting unoptimized dependencies under a debuggable profile",
            documentation: include_str!("rules/docs/profile/dependency-opt-level.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::PANIC_ABORT_RULE,
            id: "profile/panic-abort",
            tier: crate::finding::Tier::Behavioral,
            magnitude: crate::finding::Magnitude::Small,
            fixability: crate::finding::Fixability::Consequential,
            summary: "Reporting a profile that unwinds instead of aborting",
            documentation: include_str!("rules/docs/profile/panic-abort.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::DEBUG_ASSERTIONS_RULE,
            id: "profile/debug-assertions",
            tier: crate::finding::Tier::Behavioral,
            magnitude: crate::finding::Magnitude::Small,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting debug assertions in a release-like profile",
            documentation: include_str!("rules/docs/profile/debug-assertions.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::OVERFLOW_CHECKS_RULE,
            id: "profile/overflow-checks",
            tier: crate::finding::Tier::Behavioral,
            magnitude: crate::finding::Magnitude::Small,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting overflow checks in a release-like profile",
            documentation: include_str!("rules/docs/profile/overflow-checks.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::IGNORED_MEMBER_PROFILE_RULE,
            id: "workspace/ignored-member-profile",
            tier: crate::finding::Tier::Foundational,
            magnitude: crate::finding::Magnitude::None,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting a member manifest whose profile table cargo ignores",
            documentation: include_str!("rules/docs/workspace/ignored-member-profile.md"),
        },
        RuleMetadata {
            rule: &crate::rules::profile::SPLIT_DEBUGINFO_RULE,
            id: "profile/split-debuginfo",
            tier: crate::finding::Tier::Foundational,
            magnitude: crate::finding::Magnitude::Small,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting in-binary debug information on an Apple target",
            documentation: include_str!("rules/docs/profile/split-debuginfo.md"),
        },
        RuleMetadata {
            rule: &crate::rules::config::RUSTFLAGS_SHADOWED_RULE,
            id: "config/rustflags-shadowed",
            tier: crate::finding::Tier::Compiler,
            magnitude: crate::finding::Magnitude::None,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Reporting a rustflags source made inert by a higher-ranked one",
            documentation: include_str!("rules/docs/config/rustflags-shadowed.md"),
        },
        RuleMetadata {
            rule: &crate::rules::config::LINK_DEAD_CODE_RULE,
            id: "config/link-dead-code",
            tier: crate::finding::Tier::Compiler,
            magnitude: crate::finding::Magnitude::None,
            fixability: crate::finding::Fixability::Safe,
            summary: "Reporting a build that retains dead code",
            documentation: include_str!("rules/docs/config/link-dead-code.md"),
        },
        RuleMetadata {
            rule: &crate::rules::config::LINK_GC_SECTIONS_RULE,
            id: "config/link-gc-sections",
            tier: crate::finding::Tier::Compiler,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::Consequential,
            summary: "Reporting a linker that retains unreferenced sections",
            documentation: include_str!("rules/docs/config/link-gc-sections.md"),
        },
        RuleMetadata {
            rule: &crate::rules::deps::WORKSPACE_RESOLVER_RULE,
            id: "deps/workspace-resolver",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::Consequential,
            summary: "Reporting a workspace that unifies features across dependency kinds",
            documentation: include_str!("rules/docs/deps/workspace-resolver.md"),
        },
        RuleMetadata {
            rule: &crate::rules::deps::DEFAULT_FEATURES_RULE,
            id: "deps/default-features",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::Consequential,
            summary: "Reporting a dependency whose default features enable optional dependencies",
            documentation: include_str!("rules/docs/deps/default-features.md"),
        },
        RuleMetadata {
            rule: &crate::rules::deps::DUPLICATE_VERSIONS_RULE,
            id: "deps/duplicate-versions",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Reporting a crate resolved at several semver-incompatible versions",
            documentation: include_str!("rules/docs/deps/duplicate-versions.md"),
        },
        RuleMetadata {
            rule: &crate::rules::deps::FEATURE_UNIFICATION_RULE,
            id: "deps/feature-unification",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Reporting a default-features opt-out that feature unification defeats",
            documentation: include_str!("rules/docs/deps/feature-unification.md"),
        },
        RuleMetadata {
            rule: &crate::rules::deps::FEATURE_PROVENANCE_RULE,
            id: "deps/feature-provenance",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Small,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Reporting a feature enabled only because another dependency requested it",
            documentation: include_str!("rules/docs/deps/feature-provenance.md"),
        },
        RuleMetadata {
            rule: &crate::rules::deps::FEATURE_TRIM_RULE,
            id: "deps/feature-trim",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::Consequential,
            summary: "Reporting a default feature the knowledge base marks as trimmable",
            documentation: include_str!("rules/docs/deps/feature-trim.md"),
        },
        RuleMetadata {
            rule: &crate::rules::deps::LIKELY_DEV_ONLY_RULE,
            id: "deps/likely-dev-only",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Reporting a development-only crate declared as a normal dependency",
            documentation: include_str!("rules/docs/deps/likely-dev-only.md"),
        },
        RuleMetadata {
            rule: &crate::rules::deps::BUILD_ONLY_NOTE_RULE,
            id: "deps/build-only-note",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::None,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Noting that the heaviest known dependencies compile only for the host",
            documentation: include_str!("rules/docs/deps/build-only-note.md"),
        },
        RuleMetadata {
            rule: &crate::rules::library::HEAVY_DEFAULT_FEATURES_RULE,
            id: "lib/heavy-default-features",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Reporting a library whose default features enable a heavy optional dependency",
            documentation: include_str!("rules/docs/lib/heavy-default-features.md"),
        },
        RuleMetadata {
            rule: &crate::rules::library::NO_FEATURE_GRANULARITY_RULE,
            id: "lib/no-feature-granularity",
            tier: crate::finding::Tier::Dependencies,
            magnitude: crate::finding::Magnitude::Moderate,
            fixability: crate::finding::Fixability::ReportOnly,
            summary: "Reporting a library with many dependencies and no features",
            documentation: include_str!("rules/docs/lib/no-feature-granularity.md"),
        },
    ];

    REGISTRY
}

/// Evaluate every registered rule against `resolution` and return the findings.
///
/// The registry is evaluated once per selected dimension, in dimension order,
/// and the results are deduplicated by the whole finding identity — the rule
/// identifier, the package, the location and the detail text — keeping the
/// first occurrence. A dimension-neutral rule that reports the same package,
/// location and detail under every dimension therefore yields one finding
/// however many dimensions are selected; one that reports several distinct
/// locations, such as one per offending workspace member, yields one finding
/// per distinct location instead of collapsing them into one. Two findings of
/// one rule that differ only in their detail are both kept, because a
/// workspace-level rule may report several distinct subjects with no package
/// or location to tell them apart. A dimensioned rule yields one finding
/// per dimension that produces it, because its identifier itself differs
/// across dimensions.
///
/// The order of the returned findings is the direction order of the dimensions
/// and the registry; the findings are not sorted here, because
/// [`Report::new`](crate::report::Report::new) imposes the report order.
///
/// `cargo_configs` carries the resolved cargo configuration rustflags for
/// every selected package, including the root package of a non-virtual
/// workspace.
pub fn evaluate(
    resolution: &crate::Resolution,
    metadata: &cargo_metadata::Metadata,
    knowledge_base: &crate::knowledge::KnowledgeBase,
    root_manifest: &crate::manifest::ManifestDocument,
    member_manifests: &[(String, crate::manifest::ManifestDocument)],
    cargo_configs: &[(String, crate::cargo_config::ResolvedRustflags)],
) -> Vec<crate::finding::Finding> {
    let mut findings = Vec::new();

    for dimension in &resolution.dimensions {
        let context = RuleContext {
            dimension: *dimension,
            resolution,
            metadata,
            knowledge_base,
            root_manifest,
            member_manifests,
            target_triple: &resolution.target_triple,
            cargo_configs,
        };

        for entry in registry() {
            findings.extend(entry.rule.evaluate(&context));
        }
    }

    deduplicate(findings)
}

/// Keep the first finding of each identity, preserving input order.
fn deduplicate(mut findings: Vec<crate::finding::Finding>) -> Vec<crate::finding::Finding> {
    let mut kept: Vec<(
        RuleId,
        Option<String>,
        Option<crate::finding::Location>,
        String,
    )> = Vec::new();

    findings.retain(|finding| {
        let key = (
            finding.rule.clone(),
            finding.package.clone(),
            finding.location.clone(),
            finding.detail.clone(),
        );

        if kept.contains(&key) {
            return false;
        }

        kept.push(key);

        true
    });

    findings
}

/// The single place every rule's severity is assigned.
///
/// Changing one entry is intended to be a reviewable diff. An entry may be
/// either a complete rule identifier or a category selector standing for a
/// family of rules, which is how the `conflict`, `nightly` and `advisory`
/// families are written.
pub fn severity_assignments() -> &'static [(&'static str, Severity)] {
    const ASSIGNMENTS: &[(&str, Severity)] = &[
        // High
        ("profile/strip", Severity::High),
        ("profile/debug-info", Severity::High),
        ("profile/opt-level:size", Severity::High),
        ("config/link-dead-code", Severity::High),
        ("config/rustflags-shadowed", Severity::High),
        ("workspace/ignored-member-profile", Severity::High),
        ("deps/workspace-resolver", Severity::High),
        ("conflict", Severity::High),
        // Medium
        ("profile/lto", Severity::Medium),
        ("profile/incremental", Severity::Medium),
        ("profile/missing-config", Severity::Medium),
        ("profile/dependency-opt-level", Severity::Medium),
        ("profile/debug-assertions", Severity::Medium),
        ("profile/overflow-checks", Severity::Medium),
        ("config/link-gc-sections", Severity::Medium),
        ("deps/duplicate-versions", Severity::Medium),
        ("deps/default-features", Severity::Medium),
        ("deps/feature-unification", Severity::Medium),
        ("deps/feature-trim", Severity::Medium),
        ("deps/likely-dev-only", Severity::Medium),
        ("lib/heavy-default-features", Severity::Medium),
        ("lib/no-feature-granularity", Severity::Medium),
        // Low
        ("profile/codegen-units", Severity::Low),
        ("profile/panic-abort", Severity::Low),
        ("profile/split-debuginfo", Severity::Low),
        ("deps/feature-provenance", Severity::Low),
        ("nightly", Severity::Low),
        // Info
        ("advisory", Severity::Info),
        ("deps/build-only-note", Severity::Info),
    ];

    ASSIGNMENTS
}

/// The severity assigned to `id`, or [`None`] when no assignment matches.
///
/// When several assignments match, the one whose selector has the longest path
/// wins, so a specific assignment overrides a family. Two matching assignments
/// of equal path length that disagree yield [`None`], a case the table's own
/// test proves unreachable for every registered rule.
pub fn severity_of(id: &RuleId) -> Option<Severity> {
    let mut best: Option<(usize, Severity)> = None;

    for (text, severity) in severity_assignments() {
        let selector = Selector::parse(text).expect("every assignment is a well-formed selector");

        if !selector.matches(id) {
            continue;
        }

        let length = path_length(text);

        match best {
            None => best = Some((length, *severity)),
            Some((best_length, best_severity)) => {
                if length > best_length {
                    best = Some((length, *severity));
                } else if length == best_length && best_severity != *severity {
                    return None;
                }
            }
        }
    }

    best.map(|(_, severity)| severity)
}

/// The number of path segments an assignment selector names.
///
/// The path is the selector text before its dimension qualifier, so the count
/// is the number of non-empty segments that part splits into.
fn path_length(text: &str) -> usize {
    let path = text.split(':').next().unwrap_or(text);

    path.split('/')
        .filter(|segment| !segment.is_empty())
        .count()
}

/// The registered metadata for `id`, or [`None`] when no rule is registered
/// under it.
pub fn metadata_for(id: &RuleId) -> Option<&'static RuleMetadata> {
    registry().iter().find(|entry| entry.rule_id() == *id)
}

/// The directory rule documentation lives in, relative to the package root.
pub const DOCUMENTATION_DIRECTORY: &str = "src/rules/docs";

/// The path of `id`'s documentation file, relative to [`DOCUMENTATION_DIRECTORY`].
///
/// The path is the rule's path with the dimension qualifier dropped, so
/// `profile/opt-level:size` documents to `profile/opt-level.md`; a rule's
/// documentation is authored together with the rule.
pub fn documentation_path(id: &RuleId) -> String {
    format!("{}.md", id.path_text())
}

#[cfg(test)]
mod tests {
    use super::{
        Rule, RuleContext, deduplicate, documentation_path, registry, severity_assignments,
        severity_of,
    };
    use crate::finding::{
        Confidence, Effort, Finding, Fixability, Magnitude, RuleId, RuntimeCost, Selector,
        Severity, Tier,
    };
    use std::collections::BTreeSet;

    /// A test-only rule emitting one finding whose identifier is fixed.
    struct NeutralProbe;

    impl Rule for NeutralProbe {
        fn evaluate(&self, _context: &RuleContext<'_>) -> Vec<Finding> {
            vec![probe_finding("probe/neutral")]
        }
    }

    /// A test-only rule emitting one finding identified per dimension.
    struct DimensionedProbe;

    impl Rule for DimensionedProbe {
        fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
            vec![probe_finding(&format!(
                "probe/qualified:{}",
                context.dimension.as_str()
            ))]
        }
    }

    /// A finding carrying only the fields the evaluation and deduplication read.
    fn probe_finding(rule: &str) -> Finding {
        Finding {
            rule: RuleId::parse(rule).expect("the probe rule parses"),
            package: None,
            tier: Tier::Foundational,
            severity: Severity::High,
            magnitude: Magnitude::Large,
            confidence: Confidence::High,
            effort: Effort::Trivial,
            runtime_cost: RuntimeCost::None,
            fixability: Fixability::Safe,
            change: None,
            location: None,
            title: "t".to_owned(),
            detail: "d".to_owned(),
            tradeoff: "cost".to_owned(),
            references: Vec::new(),
        }
    }

    /// A finding of the neutral probe rule carrying the given `detail`.
    fn detail_probe_finding(detail: &str) -> Finding {
        Finding {
            detail: detail.to_owned(),
            title: "probe".to_owned(),
            tradeoff: "none".to_owned(),
            ..probe_finding("probe/neutral")
        }
    }

    #[test]
    fn deduplication_keeps_findings_that_differ_only_in_detail() {
        let kept = deduplicate(vec![
            detail_probe_finding("first"),
            detail_probe_finding("second"),
        ]);

        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].detail, "first");
        assert_eq!(kept[1].detail, "second");
    }

    #[test]
    fn deduplication_collapses_findings_with_an_identical_identity() {
        let kept = deduplicate(vec![
            detail_probe_finding("same"),
            detail_probe_finding("same"),
            detail_probe_finding("other"),
        ]);

        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].detail, "same");
        assert_eq!(kept[1].detail, "other");
    }

    #[test]
    fn evaluation_deduplicates_a_dimension_neutral_rule() {
        let resolution = probe_resolution();
        let manifest = probe_manifest();
        let metadata = probe_metadata();
        let knowledge_base = crate::knowledge::KnowledgeBase::default();

        // The neutral rule's identifier carries no dimension, so evaluating it
        // once per selected dimension must yield a single finding.
        let findings = {
            let mut findings = Vec::new();

            for dimension in &resolution.dimensions {
                let context = RuleContext {
                    dimension: *dimension,
                    resolution: &resolution,
                    metadata: &metadata,
                    knowledge_base: &knowledge_base,
                    root_manifest: &manifest,
                    member_manifests: &[],
                    target_triple: &resolution.target_triple,
                    cargo_configs: &[],
                };

                findings.extend(NeutralProbe.evaluate(&context));
            }

            let mut seen: BTreeSet<RuleId> = BTreeSet::new();
            findings.retain(|finding| seen.insert(finding.rule.clone()));

            findings
        };

        assert_eq!(findings.len(), 1, "the neutral rule must collapse to one");
        assert_eq!(findings[0].rule.to_string(), "probe/neutral");
    }

    #[test]
    fn evaluation_keeps_one_finding_per_dimensioned_identifier() {
        let resolution = probe_resolution();
        let manifest = probe_manifest();
        let metadata = probe_metadata();
        let knowledge_base = crate::knowledge::KnowledgeBase::default();

        let mut findings = Vec::new();

        for dimension in &resolution.dimensions {
            let context = RuleContext {
                dimension: *dimension,
                resolution: &resolution,
                metadata: &metadata,
                knowledge_base: &knowledge_base,
                root_manifest: &manifest,
                member_manifests: &[],
                target_triple: &resolution.target_triple,
                cargo_configs: &[],
            };

            findings.extend(NeutralProbe.evaluate(&context));
            findings.extend(DimensionedProbe.evaluate(&context));
        }

        let identifiers: BTreeSet<String> = findings
            .iter()
            .map(|finding| finding.rule.to_string())
            .collect();

        // A dimensioned rule and a neutral rule are distinct identifiers even
        // when the dimension repeats, so both survive; deduplicating on the
        // rule path alone would collapse them into one.
        assert_eq!(
            identifiers,
            BTreeSet::from([
                "probe/neutral".to_owned(),
                "probe/qualified:size".to_owned()
            ]),
            "the neutral and dimensioned identifiers must both survive"
        );
    }

    /// A resolution carrying two dimensions for the evaluation loop.
    fn probe_resolution() -> crate::Resolution {
        crate::Resolution {
            workspace_root: camino::Utf8PathBuf::from("/w"),
            member_count: 1,
            selected: Vec::new(),
            profile: crate::profile::builtin_defaults(
                crate::profile::BuiltinProfile::Release,
                "x86_64-unknown-linux-gnu",
            ),
            target_triple: "x86_64-unknown-linux-gnu".to_owned(),
            dimensions: vec![
                crate::dimension::Dimension::Size,
                crate::dimension::Dimension::Size,
            ],
        }
    }

    /// An empty `cargo metadata` document for building an evaluation context.
    fn probe_metadata() -> cargo_metadata::Metadata {
        serde_json::from_str(
            r#"{"packages":[],"workspace_members":[],"resolve":null,"workspace_root":"/probe","target_directory":"/probe/target","version":1}"#,
        )
        .expect("the probe metadata parses")
    }

    /// A parsed manifest for building an evaluation context.
    fn probe_manifest() -> crate::manifest::ManifestDocument {
        crate::manifest::ManifestDocument::parse(
            camino::Utf8Path::new("Cargo.toml"),
            "[package]\nname = \"probe\"\n",
        )
        .expect("the probe manifest parses")
    }

    #[test]
    fn every_severity_assignment_is_a_well_formed_selector() {
        for (selector, _) in severity_assignments() {
            assert!(
                Selector::parse(selector).is_ok(),
                "`{selector}` must be a well-formed selector"
            );
        }
    }

    #[test]
    fn severity_assignments_are_unique() {
        let assignments = severity_assignments();
        let unique: BTreeSet<&str> = assignments.iter().map(|(text, _)| *text).collect();

        assert_eq!(unique.len(), assignments.len(), "no identifier may repeat");
    }

    #[test]
    fn severity_assignments_cover_the_whole_specification_table() {
        let assignments = severity_assignments();

        assert_eq!(assignments.len(), 29, "the table has twenty-nine entries");

        let count = |severity: Severity| {
            assignments
                .iter()
                .filter(|(_, assigned)| *assigned == severity)
                .count()
        };

        assert_eq!(count(Severity::High), 8);
        assert_eq!(count(Severity::Medium), 14);
        assert_eq!(count(Severity::Low), 5);
        assert_eq!(count(Severity::Info), 2);

        assert!(
            !assignments
                .iter()
                .any(|(text, _)| *text == concat!("lib/non-optional-", "dependency")),
            "the dropped rule has no severity"
        );

        for expected in [
            ("profile/opt-level:size", Severity::High),
            ("conflict", Severity::High),
            ("nightly", Severity::Low),
            ("advisory", Severity::Info),
            ("deps/build-only-note", Severity::Info),
        ] {
            assert!(
                assignments.contains(&expected),
                "the table must contain {expected:?}"
            );
        }
    }

    #[test]
    fn severity_lookup_resolves_families_and_prefers_the_specific_entry() {
        let severity = |text: &str| severity_of(&RuleId::parse(text).unwrap());

        assert_eq!(severity("profile/strip"), Some(Severity::High));
        assert_eq!(severity("profile/opt-level:size"), Some(Severity::High));
        assert_eq!(severity("nightly/build-std"), Some(Severity::Low));
        assert_eq!(severity("advisory/upx"), Some(Severity::Info));
        assert_eq!(severity("made-up/rule"), None);
    }

    #[test]
    fn no_rule_identifier_is_assigned_two_severities() {
        let assignments = severity_assignments();

        for (left_text, left_severity) in assignments {
            for (right_text, right_severity) in assignments {
                if left_text == right_text || left_severity == right_severity {
                    continue;
                }

                let right_selector = Selector::parse(right_text).unwrap();
                let left_selector = Selector::parse(left_text).unwrap();

                assert!(
                    !right_selector.matches(&RuleId::parse(left_text).unwrap()),
                    "`{left_text}` is matched by `{right_text}` with a different severity"
                );
                assert!(
                    !left_selector.matches(&RuleId::parse(right_text).unwrap()),
                    "`{right_text}` is matched by `{left_text}` with a different severity"
                );
            }
        }
    }

    #[test]
    fn every_registered_rule_parses_and_has_a_severity() {
        for entry in registry() {
            let id = RuleId::parse(entry.id)
                .unwrap_or_else(|error| panic!("`{}` does not parse: {error}", entry.id));

            assert!(
                severity_of(&id).is_some(),
                "`{}` has no severity assignment",
                entry.id
            );
        }
    }

    #[test]
    fn documentation_path_drops_the_dimension_qualifier() {
        assert_eq!(
            documentation_path(&RuleId::parse("profile/opt-level:size").unwrap()),
            "profile/opt-level.md"
        );
        assert_eq!(
            documentation_path(&RuleId::parse("profile/strip").unwrap()),
            "profile/strip.md"
        );
    }
}
