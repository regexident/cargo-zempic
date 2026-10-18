//! Rules about the dependency graph and dependency declarations.
//!
//! The rules in this module read the resolved dependency graph and the
//! dependency declarations in manifests, never source.

use crate::finding::{
    Change, Confidence, Effort, Finding, Fixability, Location, RuleId, RuntimeCost,
};
use crate::manifest::{KeySpan, ManifestDocument};
use crate::rules::{Rule, RuleContext, RuleMetadata};
use std::collections::{BTreeMap, BTreeSet};

/// Where cargo takes the effective feature resolver from.
enum ResolverSource {
    /// An explicit `workspace.resolver` key holding this value.
    WorkspaceKey(String),
    /// An explicit `package.resolver` key holding this value.
    PackageKey(String),
    /// The root package's edition, `None` when the package declares none.
    Edition(Option<String>),
    /// A virtual manifest, which has no edition to derive a resolver from.
    VirtualManifest,
}

/// Find the source of the feature resolver for the workspace rooted at
/// `root_manifest`.
fn resolver_source(root_manifest: &ManifestDocument) -> ResolverSource {
    let document = root_manifest.document();
    let explicit = |table: &str| {
        document
            .get(table)
            .and_then(|item| item.get("resolver"))
            .and_then(|item| item.as_str())
            .map(str::to_owned)
    };

    if let Some(value) = explicit("workspace") {
        return ResolverSource::WorkspaceKey(value);
    }

    if let Some(value) = explicit("package") {
        return ResolverSource::PackageKey(value);
    }

    let Some(package) = document.get("package") else {
        return ResolverSource::VirtualManifest;
    };

    let inherits_edition = package
        .get("edition")
        .and_then(|item| item.get("workspace"))
        .and_then(|item| item.as_bool())
        == Some(true);

    // A package without an `edition` key inherits nothing and defaults to
    // edition 2015; only an explicit `edition.workspace = true` reads the
    // workspace's value.
    let edition = if inherits_edition {
        document
            .get("workspace")
            .and_then(|item| item.get("package"))
            .and_then(|item| item.get("edition"))
            .and_then(|item| item.as_str())
    } else {
        package.get("edition").and_then(|item| item.as_str())
    }
    .map(str::to_owned);

    ResolverSource::Edition(edition)
}

/// Map a resolver value or edition to the resolver cargo applies for it.
///
/// An unrecognized explicit value maps to `"2"`: cargo rejects it before this
/// crate runs, so the exact answer never reaches a report, and `"2"` keeps the
/// resolver-1 rule quiet rather than reporting a manifest cargo would not load.
fn resolver_for_value(value: &str) -> &'static str {
    match value {
        "1" => "1",
        "3" => "3",
        _ => "2",
    }
}

/// The feature resolver cargo applies to the workspace rooted at `root_manifest`.
///
/// This is `workspace.resolver` when present, else `package.resolver`, else the
/// resolver implied by the root package's edition: an absent edition, `2015` or
/// `2018` give `"1"`, `2021` gives `"2"` and `2024` gives `"3"`. A virtual
/// manifest has no edition, so without `workspace.resolver` it resolves to
/// `"1"`.
pub(crate) fn effective_resolver(root_manifest: &ManifestDocument) -> &'static str {
    match resolver_source(root_manifest) {
        ResolverSource::WorkspaceKey(value) | ResolverSource::PackageKey(value) => {
            resolver_for_value(&value)
        }
        ResolverSource::Edition(None) | ResolverSource::VirtualManifest => "1",
        ResolverSource::Edition(Some(edition)) => match edition.as_str() {
            "2015" | "2018" => "1",
            "2024" => "3",
            _ => "2",
        },
    }
}

/// Reports a workspace whose effective feature resolver is `"1"`.
///
/// Resolver 1 unifies the features of a dependency across its normal, build
/// and dev uses, so a feature requested only by a build script or a test
/// reaches the shipped artifact.
pub struct WorkspaceResolverRule;

/// The instance registered for [`WorkspaceResolverRule`].
pub const WORKSPACE_RESOLVER_RULE: WorkspaceResolverRule = WorkspaceResolverRule;

impl Rule for WorkspaceResolverRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("deps/workspace-resolver");
        let root_manifest = context.root_manifest;

        if effective_resolver(root_manifest) != "1" {
            return Vec::new();
        }

        let source = resolver_source(root_manifest);
        let has_workspace_table = root_manifest.document().get("workspace").is_some();

        let (explicit_key, derivation) = match &source {
            ResolverSource::WorkspaceKey(_) => (
                Some("workspace.resolver"),
                "`workspace.resolver` is set to \"1\"".to_owned(),
            ),
            ResolverSource::PackageKey(_) => (
                Some("package.resolver"),
                "`package.resolver` is set to \"1\"".to_owned(),
            ),
            ResolverSource::Edition(Some(edition)) => (
                None,
                format!(
                    "no resolver is set and the root package's edition is {edition}, which \
                     implies resolver 1"
                ),
            ),
            ResolverSource::Edition(None) => (
                None,
                "no resolver is set and the root package declares no edition, which implies \
                 resolver 1"
                    .to_owned(),
            ),
            ResolverSource::VirtualManifest => (
                None,
                "no resolver is set and a virtual manifest has no edition to derive one from, \
                 so resolver 1 applies"
                    .to_owned(),
            ),
        };

        let location = explicit_key
            .and_then(|key| root_manifest.key_span(key))
            .unwrap_or_else(|| {
                let key = if has_workspace_table {
                    "workspace.resolver"
                } else {
                    "package.resolver"
                };

                KeySpan {
                    file: root_manifest.path().to_owned(),
                    key: key.to_owned(),
                    line: 1,
                    column: 1,
                }
            });

        let current = if explicit_key.is_some() { "1" } else { "unset" };

        let detail = format!(
            "The effective resolver is 1: {derivation}. Resolver 1 unifies features across \
             normal, build and dev dependencies, so features requested only by build or dev \
             dependencies reach the artifact."
        );

        vec![build(
            metadata,
            None,
            Confidence::High,
            Effort::Trivial,
            metadata.fixability,
            Some(Change {
                current: current.to_owned(),
                recommended: "2".to_owned(),
            }),
            Some(location),
            "The workspace resolves features with resolver 1",
            detail,
            "Feature sets of dependencies shared between normal, build and dev kinds stop \
             unifying; a build that relied on a feature leaking across kinds can fail to \
             compile until that feature is requested where it is used.",
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

/// Build a finding from a registry entry's metadata and the rule-specific
/// fields.
///
/// Takes `confidence`, `effort` and `fixability` as parameters because the
/// rules in this module differ in them.
#[expect(
    clippy::too_many_arguments,
    reason = "a flat constructor with one call site per rule reads more plainly than a \
              parameter struct"
)]
fn build(
    metadata: &'static RuleMetadata,
    package: Option<String>,
    confidence: Confidence,
    effort: Effort,
    fixability: Fixability,
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
        confidence,
        effort,
        runtime_cost: RuntimeCost::None,
        fixability,
        change,
        location: location.map(Location::new),
        title: title.to_owned(),
        detail,
        tradeoff: tradeoff.to_owned(),
        references: Vec::new(),
    }
}

/// The semver compatibility class of `version`.
///
/// Two versions are interchangeable under cargo's resolution exactly when their
/// classes are equal: the major version when it is non-zero, else the minor
/// version of a `0.x` release, else the patch version of a `0.0.x` release.
fn compatibility_class(version: &cargo_metadata::semver::Version) -> (u64, u64, u64) {
    if version.major > 0 {
        return (version.major, 0, 0);
    }

    if version.minor > 0 {
        return (0, version.minor, 0);
    }

    (0, 0, version.patch)
}

/// Reports a crate that is resolved at several semver-incompatible versions.
///
/// Each incompatible version is compiled and linked separately, so the crate's
/// code appears in the artifact once per version.
pub struct DuplicateVersionsRule;

/// The instance registered for [`DuplicateVersionsRule`].
pub const DUPLICATE_VERSIONS_RULE: DuplicateVersionsRule = DuplicateVersionsRule;

impl Rule for DuplicateVersionsRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("deps/duplicate-versions");

        let Some(resolve) = &context.metadata.resolve else {
            return Vec::new();
        };

        let resolved: BTreeSet<&cargo_metadata::PackageId> =
            resolve.nodes.iter().map(|node| &node.id).collect();

        let mut versions_by_name: BTreeMap<&str, BTreeSet<&cargo_metadata::semver::Version>> =
            BTreeMap::new();

        for package in &context.metadata.packages {
            if resolved.contains(&package.id) {
                versions_by_name
                    .entry(package.name.as_str())
                    .or_default()
                    .insert(&package.version);
            }
        }

        let mut findings = Vec::new();

        for (name, versions) in versions_by_name {
            let classes: BTreeSet<(u64, u64, u64)> = versions
                .iter()
                .map(|version| compatibility_class(version))
                .collect();

            if classes.len() < 2 {
                continue;
            }

            let listed = versions
                .iter()
                .map(|version| format!("`{version}`"))
                .collect::<Vec<_>>()
                .join(", ");

            let detail = format!(
                "`{name}` is resolved at {listed}. Each incompatible version is compiled and \
                 linked separately, so the crate's code is present in the artifact once per \
                 version."
            );

            findings.push(build(
                metadata,
                None,
                Confidence::High,
                Effort::Moderate,
                Fixability::ReportOnly,
                None,
                None,
                &format!("`{name}` is resolved at several incompatible versions"),
                detail,
                "Unifying on one version can require upgrading or downgrading the crates that \
                 depend on it, which can change their behavior.",
            ));
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::{compatibility_class, effective_resolver};
    use crate::manifest::ManifestDocument;
    use camino::Utf8Path;
    use cargo_metadata::semver::Version;

    #[test]
    fn compatibility_class_uses_the_leftmost_non_zero_component() {
        let class = |text: &str| compatibility_class(&Version::parse(text).unwrap());

        assert_eq!(class("1.2.3"), (1, 0, 0));
        assert_eq!(class("1.9.0"), (1, 0, 0));
        assert_eq!(class("0.4.7"), (0, 4, 0));
        assert_eq!(class("0.0.3"), (0, 0, 3));
        assert_eq!(class("2.0.0"), (2, 0, 0));
    }

    fn resolver_of(text: &str) -> &'static str {
        let manifest =
            ManifestDocument::parse(Utf8Path::new("Cargo.toml"), text).expect("the text parses");

        effective_resolver(&manifest)
    }

    #[test]
    fn effective_resolver_follows_the_explicit_key_then_the_edition() {
        assert_eq!(resolver_of("[workspace]\nmembers = []\n"), "1");
        assert_eq!(resolver_of("[workspace]\nresolver = \"2\"\n"), "2");
        assert_eq!(
            resolver_of(
                "[workspace]\nresolver = \"1\"\n[package]\nname = \"p\"\nedition = \"2021\"\n"
            ),
            "1"
        );
        assert_eq!(
            resolver_of("[package]\nname = \"p\"\nedition = \"2018\"\n"),
            "1"
        );
        assert_eq!(resolver_of("[package]\nname = \"p\"\n"), "1");
        assert_eq!(
            resolver_of("[package]\nname = \"p\"\nedition = \"2021\"\n"),
            "2"
        );
        assert_eq!(
            resolver_of("[package]\nname = \"p\"\nedition = \"2024\"\n"),
            "3"
        );
        assert_eq!(
            resolver_of("[package]\nname = \"p\"\nedition = \"2021\"\nresolver = \"1\"\n"),
            "1"
        );
        assert_eq!(
            resolver_of(
                "[workspace.package]\nedition = \"2021\"\n[package]\nname = \"p\"\nedition.workspace = true\n"
            ),
            "2"
        );
        assert_eq!(
            resolver_of(
                "[workspace.package]\nedition = \"2018\"\n[package]\nname = \"p\"\nedition.workspace = true\n"
            ),
            "1"
        );
        assert_eq!(
            resolver_of("[workspace.package]\nedition = \"2021\"\n[package]\nname = \"p\"\n"),
            "1"
        );
    }
}
