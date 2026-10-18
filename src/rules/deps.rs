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

/// The features and optional dependencies a set of requested features enables.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ExpandedFeatures {
    /// Every feature the request enables, directly or through other features.
    pub(crate) features: BTreeSet<String>,
    /// Every optional dependency the request enables.
    pub(crate) optional_dependencies: BTreeSet<String>,
}

/// Expand `requested` through `feature_table` following cargo's feature-value
/// grammar.
///
/// A value naming a key of `feature_table` enables that feature and, in turn,
/// its values. `dep:x` enables the optional dependency `x`, and `x/f` enables
/// `x` when it is optional. `x?/f` enables nothing by itself, because it only
/// forwards `f` to `x` when `x` is enabled by other means. A bare name that is
/// not a key of the table but names an optional dependency is that
/// dependency's implicit feature and enables it. A requested name that is
/// neither is ignored.
///
/// Each feature is expanded once, so a cyclic table terminates.
pub(crate) fn expand_features(
    feature_table: &BTreeMap<String, Vec<String>>,
    optional_dependencies: &BTreeSet<String>,
    requested: &[&str],
) -> ExpandedFeatures {
    let mut expanded = ExpandedFeatures::default();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut pending: Vec<String> = requested.iter().map(|name| (*name).to_owned()).collect();

    while let Some(name) = pending.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }

        let Some(values) = feature_table.get(&name) else {
            if optional_dependencies.contains(&name) {
                expanded.optional_dependencies.insert(name);
            }

            continue;
        };

        expanded.features.insert(name);

        for value in values {
            if let Some(dependency) = value.strip_prefix("dep:") {
                if optional_dependencies.contains(dependency) {
                    expanded.optional_dependencies.insert(dependency.to_owned());
                }
            } else if let Some((dependency, _feature)) = value.split_once('/') {
                if dependency.ends_with('?') {
                    continue;
                }

                if optional_dependencies.contains(dependency) {
                    expanded.optional_dependencies.insert(dependency.to_owned());
                }
            } else {
                pending.push(value.clone());
            }
        }
    }

    expanded
}

/// The name `package` declares each of its optional dependencies under.
///
/// This is the dependency's `rename` when present, else its package name,
/// because that is the key its implicit feature and `dep:` references use.
pub(crate) fn optional_dependency_names(package: &cargo_metadata::Package) -> BTreeSet<String> {
    package
        .dependencies
        .iter()
        .filter(|dependency| dependency.optional)
        .map(declared_key)
        .collect()
}

/// The key `dependency` is declared under in its manifest.
pub(crate) fn declared_key(dependency: &cargo_metadata::Dependency) -> String {
    dependency
        .rename
        .clone()
        .unwrap_or_else(|| dependency.name.clone())
}

/// The metadata packages the run selected, in selection order.
pub(crate) fn selected_packages<'a>(context: &RuleContext<'a>) -> Vec<&'a cargo_metadata::Package> {
    let metadata: &'a cargo_metadata::Metadata = context.metadata;

    context
        .resolution
        .selected
        .iter()
        .filter_map(|selected| {
            metadata
                .packages
                .iter()
                .find(|package| package.manifest_path == selected.manifest_path)
        })
        .collect()
}

/// The parsed manifest of `package`, when the run holds one.
///
/// The root package's manifest is kept apart from the members' manifests, so
/// the root is matched by path and every other package by name.
pub(crate) fn manifest_for<'a>(
    context: &RuleContext<'a>,
    package: &cargo_metadata::Package,
) -> Option<&'a ManifestDocument> {
    if package.manifest_path == context.root_manifest.path() {
        return Some(context.root_manifest);
    }

    let member_manifests: &'a [(String, ManifestDocument)] = context.member_manifests;

    member_manifests
        .iter()
        .find(|(name, _)| *name == package.name.as_str())
        .map(|(_, manifest)| manifest)
}

/// The package `package` reaches through `dependency` as a normal dependency.
///
/// The resolve graph names each edge by the dependency's extern crate name,
/// which is its declared key with `-` replaced by `_`. Returns `None` when the
/// graph holds no such normal edge, for example when the dependency does not
/// apply to the resolved target platform.
pub(crate) fn resolved_dependency<'a>(
    metadata: &'a cargo_metadata::Metadata,
    package: &cargo_metadata::Package,
    dependency: &cargo_metadata::Dependency,
) -> Option<&'a cargo_metadata::Package> {
    let node = metadata
        .resolve
        .as_ref()?
        .nodes
        .iter()
        .find(|node| node.id == package.id)?;

    let extern_name = declared_key(dependency).replace('-', "_");

    let edge = node.deps.iter().find(|edge| {
        edge.name == extern_name
            && edge
                .dep_kinds
                .iter()
                .any(|info| info.kind == cargo_metadata::DependencyKind::Normal)
    })?;

    metadata
        .packages
        .iter()
        .find(|candidate| candidate.id == edge.pkg)
}

/// The position of `dependency`'s key in the `[dependencies]` table of
/// `manifest`.
///
/// Returns `None` for a target-specific dependency, which lives under a
/// `[target.*]` table, and when the manifest is absent or lacks the key.
pub(crate) fn dependency_location(
    manifest: Option<&ManifestDocument>,
    dependency: &cargo_metadata::Dependency,
) -> Option<KeySpan> {
    if dependency.target.is_some() {
        return None;
    }

    manifest?.key_span(&format!("dependencies.{}", declared_key(dependency)))
}

/// The sentence appended to every verdict that depends on resolver-v2's
/// separation of dependency kinds.
pub(crate) const KIND_SPLITTING_NOTE: &str = "Resolver-v2 kind splitting is approximated: cargo metadata unifies features across normal, build and dev dependencies, so this verdict is not certain.";

/// Reports a dependency declared with default features that enable optional
/// dependencies of that dependency.
///
/// The default set is expanded through the dependency's feature table. A
/// default set that enables only plain features is not reported, because the
/// rule has no evidence that those features cost anything.
pub struct DefaultFeaturesRule;

/// The instance registered for [`DefaultFeaturesRule`].
pub const DEFAULT_FEATURES_RULE: DefaultFeaturesRule = DefaultFeaturesRule;

impl Rule for DefaultFeaturesRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("deps/default-features");
        let mut findings = Vec::new();

        for package in selected_packages(context) {
            let manifest = manifest_for(context, package);

            for dependency in &package.dependencies {
                if dependency.kind != cargo_metadata::DependencyKind::Normal
                    || !dependency.uses_default_features
                {
                    continue;
                }

                let Some(resolved) = resolved_dependency(context.metadata, package, dependency)
                else {
                    continue;
                };

                if !resolved.features.contains_key("default") {
                    continue;
                }

                let expanded = expand_features(
                    &resolved.features,
                    &optional_dependency_names(resolved),
                    &["default"],
                );

                if expanded.optional_dependencies.is_empty() {
                    continue;
                }

                let key = declared_key(dependency);

                let listed = expanded
                    .optional_dependencies
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ");

                let detail = format!(
                    "The default features of `{key}` enable the optional dependencies {listed}. \
                     {KIND_SPLITTING_NOTE}"
                );

                findings.push(build(
                    metadata,
                    Some(package.name.to_string()),
                    Confidence::Medium,
                    Effort::Moderate,
                    Fixability::Consequential,
                    Some(Change {
                        current: "true".to_owned(),
                        recommended: "false".to_owned(),
                    }),
                    dependency_location(manifest, dependency),
                    &format!("`{key}` is declared with default features that enable optional dependencies"),
                    detail,
                    "Disabling defaults removes every default feature, including ones the code \
                     may use; each one still needed must be re-enabled by name.",
                ));
            }
        }

        findings
    }
}

/// Reports a `default-features = false` opt-out that cargo's feature
/// unification defeats.
///
/// A resolved node lists the features enabled for it across the whole graph,
/// so `default` appears there whenever any requester asked for the defaults,
/// including the requesters outside the workspace members.
pub struct FeatureUnificationRule;

/// The instance registered for [`FeatureUnificationRule`].
pub const FEATURE_UNIFICATION_RULE: FeatureUnificationRule = FeatureUnificationRule;

/// The names of the workspace members whose own normal dependencies request
/// the default features of `target`, in ascending order.
fn members_requesting_defaults(
    metadata: &cargo_metadata::Metadata,
    target: &cargo_metadata::PackageId,
) -> BTreeSet<String> {
    metadata
        .packages
        .iter()
        .filter(|member| metadata.workspace_members.contains(&member.id))
        .filter(|member| {
            member.dependencies.iter().any(|dependency| {
                dependency.kind == cargo_metadata::DependencyKind::Normal
                    && dependency.uses_default_features
                    && resolved_dependency(metadata, member, dependency)
                        .is_some_and(|resolved| resolved.id == *target)
            })
        })
        .map(|member| member.name.to_string())
        .collect()
}

impl Rule for FeatureUnificationRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("deps/feature-unification");
        let mut findings = Vec::new();

        let Some(resolve) = context.metadata.resolve.as_ref() else {
            return findings;
        };

        for package in selected_packages(context) {
            let manifest = manifest_for(context, package);

            for dependency in &package.dependencies {
                if dependency.kind != cargo_metadata::DependencyKind::Normal
                    || dependency.uses_default_features
                {
                    continue;
                }

                let Some(resolved) = resolved_dependency(context.metadata, package, dependency)
                else {
                    continue;
                };

                let defaults_enabled = resolve
                    .nodes
                    .iter()
                    .find(|node| node.id == resolved.id)
                    .is_some_and(|node| {
                        node.features
                            .iter()
                            .any(|feature| feature.as_str() == "default")
                    });

                if !defaults_enabled {
                    continue;
                }

                let key = declared_key(dependency);
                let requesters = members_requesting_defaults(context.metadata, &resolved.id);

                let reason = if requesters.is_empty() {
                    format!(
                        "A dependency outside the workspace members' own declarations requests \
                         the default features of `{key}`, and cargo unifies features across the \
                         graph."
                    )
                } else {
                    let members = requesters
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ");

                    format!(
                        "Workspace member {members} requests the default features of `{key}`, \
                         and cargo unifies features across the workspace."
                    )
                };

                findings.push(build(
                    metadata,
                    Some(package.name.to_string()),
                    Confidence::Medium,
                    Effort::Moderate,
                    Fixability::ReportOnly,
                    None,
                    dependency_location(manifest, dependency),
                    &format!(
                        "`{key}` keeps its default features despite `default-features = false`"
                    ),
                    format!("{reason} {KIND_SPLITTING_NOTE}"),
                    "Restoring the opt-out means changing the other declaration that requests the \
                     defaults, which may remove functionality its crate relies on.",
                ));
            }
        }

        findings
    }
}

/// Reports features of a dependency that only other dependencies request.
///
/// A feature is enabled when any requester in the graph asks for it. The
/// features the workspace members ask for themselves, through their
/// declarations and through their own features that forward to a dependency,
/// are subtracted from the features the resolved graph enables. What remains
/// is present only because some other dependency requested it.
pub struct FeatureProvenanceRule;

/// The instance registered for [`FeatureProvenanceRule`].
pub const FEATURE_PROVENANCE_RULE: FeatureProvenanceRule = FeatureProvenanceRule;

/// The features of `target` that the workspace members request themselves.
///
/// Each direct normal declaration of `target` in a member requests `default`
/// when it uses default features, plus its listed features. A member feature
/// whose value forwards a feature to the declaration, as `key/feature` or
/// `key?/feature`, requests that feature once the member feature is enabled.
/// Every request is expanded through the feature table of `target`, and both
/// the features and the optional dependencies it enables count, because an
/// optional dependency is also a feature of its own.
fn features_requested_by_members(
    metadata: &cargo_metadata::Metadata,
    target: &cargo_metadata::Package,
) -> BTreeSet<String> {
    let optional_dependencies = optional_dependency_names(target);
    let mut requested = BTreeSet::new();

    let Some(resolve) = metadata.resolve.as_ref() else {
        return requested;
    };

    for member in metadata
        .packages
        .iter()
        .filter(|member| metadata.workspace_members.contains(&member.id))
    {
        let enabled_on_member: BTreeSet<&str> = resolve
            .nodes
            .iter()
            .find(|node| node.id == member.id)
            .map(|node| {
                node.features
                    .iter()
                    .map(|feature| feature.as_str())
                    .collect()
            })
            .unwrap_or_default();

        for dependency in &member.dependencies {
            if dependency.kind != cargo_metadata::DependencyKind::Normal
                || resolved_dependency(metadata, member, dependency)
                    .is_none_or(|resolved| resolved.id != target.id)
            {
                continue;
            }

            let key = declared_key(dependency);
            let mut names: Vec<&str> = dependency.features.iter().map(String::as_str).collect();

            if dependency.uses_default_features {
                names.push("default");
            }

            for (feature, values) in &member.features {
                if !enabled_on_member.contains(feature.as_str()) {
                    continue;
                }

                for value in values {
                    let Some((forwarded_to, forwarded)) = value.split_once('/') else {
                        continue;
                    };

                    if forwarded_to.trim_end_matches('?') == key {
                        names.push(forwarded);
                    }
                }
            }

            let expanded = expand_features(&target.features, &optional_dependencies, &names);

            requested.extend(expanded.features);
            requested.extend(expanded.optional_dependencies);
        }
    }

    requested
}

impl Rule for FeatureProvenanceRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("deps/feature-provenance");
        let mut findings = Vec::new();

        let Some(resolve) = context.metadata.resolve.as_ref() else {
            return findings;
        };

        for package in selected_packages(context) {
            let manifest = manifest_for(context, package);

            for dependency in &package.dependencies {
                if dependency.kind != cargo_metadata::DependencyKind::Normal {
                    continue;
                }

                let Some(resolved) = resolved_dependency(context.metadata, package, dependency)
                else {
                    continue;
                };

                let Some(node) = resolve.nodes.iter().find(|node| node.id == resolved.id) else {
                    continue;
                };

                let requested = features_requested_by_members(context.metadata, resolved);

                let only_transitive: BTreeSet<&str> = node
                    .features
                    .iter()
                    .map(|feature| feature.as_str())
                    .filter(|feature| !requested.contains(*feature))
                    .collect();

                if only_transitive.is_empty() {
                    continue;
                }

                let key = declared_key(dependency);

                let listed = only_transitive
                    .iter()
                    .map(|feature| format!("`{feature}`"))
                    .collect::<Vec<_>>()
                    .join(", ");

                let detail = format!(
                    "No workspace member requests the enabled features {listed} of `{key}`; \
                     other dependencies in the graph do. {KIND_SPLITTING_NOTE}"
                );

                findings.push(build(
                    metadata,
                    Some(package.name.to_string()),
                    Confidence::Medium,
                    Effort::Moderate,
                    Fixability::ReportOnly,
                    None,
                    dependency_location(manifest, dependency),
                    &format!("Features of `{key}` are enabled only by other dependencies"),
                    detail,
                    "Removing these features means dropping or replacing the dependency that \
                     requests them, which may lose functionality it provides.",
                ));
            }
        }

        findings
    }
}

/// Reports a default feature of a dependency that the knowledge base marks as
/// trimmable.
///
/// The entry matching the resolved version decides which default features are
/// reported; a feature the declaration requests by name, directly or through
/// another requested feature, is wanted and is not reported.
pub struct FeatureTrimRule;

/// The instance registered for [`FeatureTrimRule`].
pub const FEATURE_TRIM_RULE: FeatureTrimRule = FeatureTrimRule;

impl Rule for FeatureTrimRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("deps/feature-trim");
        let mut findings = Vec::new();

        for package in selected_packages(context) {
            let manifest = manifest_for(context, package);

            for dependency in &package.dependencies {
                if dependency.kind != cargo_metadata::DependencyKind::Normal
                    || !dependency.uses_default_features
                {
                    continue;
                }

                let Some(resolved) = resolved_dependency(context.metadata, package, dependency)
                else {
                    continue;
                };

                let Some(entry) = context
                    .knowledge_base
                    .entry_for(&resolved.name, &resolved.version)
                else {
                    continue;
                };

                let optional_dependencies = optional_dependency_names(resolved);
                let default_features =
                    expand_features(&resolved.features, &optional_dependencies, &["default"])
                        .features;
                let requested: Vec<&str> = dependency.features.iter().map(String::as_str).collect();
                let requested_features =
                    expand_features(&resolved.features, &optional_dependencies, &requested)
                        .features;

                let stale = entry.is_stale(&resolved.version);
                let key = declared_key(dependency);

                for feature in trimmable_features(entry, &default_features, &requested_features) {
                    let mut detail = format!(
                        "`{}` {} enables `{}` through its default features. Advice: {}",
                        resolved.name, resolved.version, feature.name, feature.advice
                    );

                    if stale {
                        detail.push(' ');
                        detail.push_str(&crate::knowledge::stale_note(
                            &resolved.name,
                            &entry.verified_through,
                            &resolved.version,
                        ));
                    }

                    let mut finding = build(
                        metadata,
                        Some(package.name.to_string()),
                        Confidence::High,
                        Effort::Moderate,
                        crate::knowledge::effective_fixability(
                            Fixability::Consequential,
                            feature.fixable,
                            stale,
                        ),
                        Some(Change {
                            current: "true".to_owned(),
                            recommended: "false".to_owned(),
                        }),
                        dependency_location(manifest, dependency),
                        &format!(
                            "Default feature `{}` of `{key}` can be trimmed",
                            feature.name
                        ),
                        detail,
                        "Disabling defaults removes every default feature; each one the code \
                         still needs must be re-enabled by name.",
                    );

                    finding.magnitude = feature.magnitude;
                    findings.push(finding);
                }
            }
        }

        findings
    }
}

/// The feature entries of `entry` that the default set enables and the
/// declaration does not request, in entry order.
///
/// Only entries flagged `default` qualify, so an entry advising on a
/// non-default feature never produces a trim.
fn trimmable_features<'a>(
    entry: &'a crate::knowledge::CrateEntry,
    default_features: &BTreeSet<String>,
    requested_features: &BTreeSet<String>,
) -> Vec<&'a crate::knowledge::FeatureEntry> {
    entry
        .features
        .iter()
        .filter(|feature| {
            feature.default
                && default_features.contains(&feature.name)
                && !requested_features.contains(&feature.name)
        })
        .collect()
}

/// Reports a crate the knowledge base marks as development-only that a package
/// declares under `[dependencies]`.
///
/// Test, benchmark and example harnesses are almost never needed by shipped
/// code, yet declaring one as a normal dependency compiles it into every
/// build. The verdict is a likelihood, not a fact, because the declaring
/// package may use the crate outside tests.
pub struct LikelyDevOnlyRule;

/// The instance registered for [`LikelyDevOnlyRule`].
pub const LIKELY_DEV_ONLY_RULE: LikelyDevOnlyRule = LikelyDevOnlyRule;

impl Rule for LikelyDevOnlyRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("deps/likely-dev-only");
        let mut findings = Vec::new();

        for package in selected_packages(context) {
            let manifest = manifest_for(context, package);

            for dependency in &package.dependencies {
                if dependency.kind != cargo_metadata::DependencyKind::Normal {
                    continue;
                }

                let Some(resolved) = resolved_dependency(context.metadata, package, dependency)
                else {
                    continue;
                };

                let Some(entry) = context
                    .knowledge_base
                    .entry_for(&resolved.name, &resolved.version)
                else {
                    continue;
                };

                if !entry.dev_only {
                    continue;
                }

                let key = declared_key(dependency);

                let mut detail = format!(
                    "`{}` {} is listed in the knowledge base as almost always used only by \
                     tests, benchmarks or examples, yet `{}` declares it under \
                     `[dependencies]`, so every build compiles it.",
                    resolved.name, resolved.version, package.name
                );

                if entry.is_stale(&resolved.version) {
                    detail.push(' ');
                    detail.push_str(&crate::knowledge::stale_note(
                        &resolved.name,
                        &entry.verified_through,
                        &resolved.version,
                    ));
                }

                findings.push(build(
                    metadata,
                    Some(package.name.to_string()),
                    Confidence::Medium,
                    Effort::Moderate,
                    Fixability::ReportOnly,
                    None,
                    dependency_location(manifest, dependency),
                    &format!("`{key}` is usually a development-only dependency"),
                    detail,
                    "Moving it to `[dev-dependencies]` breaks any non-test code that uses it; \
                     that code must be gated or moved first.",
                ));
            }
        }

        findings
    }
}

/// Notes that the heaviest known dependencies of a package compile only for
/// the host.
///
/// Build scripts and procedural macros run during compilation, so their
/// dependency trees are compiled for the host and never reach the artifact.
/// When every heavy crate in a package's graph sits behind such an edge, the
/// note points optimization effort away from them. A heavy crate that is also
/// reached through a normal edge from the artifact keeps the note silent.
pub struct BuildOnlyNoteRule;

/// The instance registered for [`BuildOnlyNoteRule`].
pub const BUILD_ONLY_NOTE_RULE: BuildOnlyNoteRule = BuildOnlyNoteRule;

impl Rule for BuildOnlyNoteRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        use cargo_metadata::{DependencyKind, PackageId, TargetKind};
        use std::collections::{HashMap, HashSet};

        let metadata = registered("deps/build-only-note");
        let mut findings = Vec::new();

        let Some(resolve) = context.metadata.resolve.as_ref() else {
            return findings;
        };

        let nodes: HashMap<&PackageId, &cargo_metadata::Node> =
            resolve.nodes.iter().map(|node| (&node.id, node)).collect();
        let packages: HashMap<&PackageId, &cargo_metadata::Package> = context
            .metadata
            .packages
            .iter()
            .map(|package| (&package.id, package))
            .collect();

        for package in selected_packages(context) {
            let produces_artifact = context
                .resolution
                .selected
                .iter()
                .find(|selected| selected.manifest_path == package.manifest_path)
                .is_some_and(|selected| selected.classification.produces_linked_artifact());

            if !produces_artifact {
                continue;
            }

            // Each entry pairs a package with whether it is reached only
            // through host-compiled edges. A package reached both ways is
            // visited once per flag, so a target path is never masked by a
            // host path found first.
            let mut visited: HashSet<(&PackageId, bool)> = HashSet::new();
            let mut worklist: Vec<(&PackageId, bool)> = vec![(&package.id, false)];
            let mut host_only: BTreeMap<(&str, &PackageId), &cargo_metadata::Package> =
                BTreeMap::new();
            let mut reaches_target = false;

            while let Some((id, host)) = worklist.pop() {
                if !visited.insert((id, host)) {
                    continue;
                }

                if id != &package.id {
                    let Some(reached) = packages.get(id) else {
                        continue;
                    };

                    let heavy = context
                        .knowledge_base
                        .entry_for(&reached.name, &reached.version)
                        .is_some_and(|entry| entry.is_heavy());

                    if heavy {
                        if host {
                            host_only.insert((reached.name.as_str(), id), reached);
                        } else {
                            reaches_target = true;
                        }
                    }
                }

                let Some(node) = nodes.get(id) else {
                    continue;
                };

                for dependency in &node.deps {
                    let Some(child) = packages.get(&dependency.pkg) else {
                        continue;
                    };

                    let compiles_for_host = child
                        .targets
                        .iter()
                        .any(|target| target.kind.contains(&TargetKind::ProcMacro));

                    for kind in &dependency.dep_kinds {
                        match kind.kind {
                            DependencyKind::Normal => {
                                worklist.push((&dependency.pkg, host || compiles_for_host));
                            }
                            DependencyKind::Build => worklist.push((&dependency.pkg, true)),
                            _ => {}
                        }
                    }
                }
            }

            if reaches_target || host_only.is_empty() {
                continue;
            }

            let crates: Vec<&cargo_metadata::Package> = host_only.into_values().collect();

            let names = crates
                .iter()
                .map(|reached| format!("`{}`", reached.name))
                .collect::<Vec<_>>()
                .join(", ");
            let mut detail = format!(
                "{names} are marked heavy in the knowledge base and are reached only through \
                 build dependencies or procedural macros, so they compile for the host and add \
                 nothing to the artifact."
            );

            for reached in &crates {
                let Some(entry) = context
                    .knowledge_base
                    .entry_for(&reached.name, &reached.version)
                else {
                    continue;
                };

                if entry.is_stale(&reached.version) {
                    detail.push(' ');
                    detail.push_str(&crate::knowledge::stale_note(
                        &reached.name,
                        &entry.verified_through,
                        &reached.version,
                    ));
                }
            }

            findings.push(build(
                metadata,
                Some(package.name.to_string()),
                Confidence::High,
                Effort::Trivial,
                Fixability::ReportOnly,
                None,
                None,
                &format!(
                    "The heaviest known dependencies of `{}` compile only for the host",
                    package.name
                ),
                detail,
                "None; the note points optimization effort away from dependencies that do not \
                 reach the artifact.",
            ));
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::{compatibility_class, effective_resolver, expand_features, trimmable_features};
    use crate::manifest::ManifestDocument;
    use camino::Utf8Path;
    use cargo_metadata::semver::Version;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn compatibility_class_uses_the_leftmost_non_zero_component() {
        let class = |text: &str| compatibility_class(&Version::parse(text).unwrap());

        assert_eq!(class("1.2.3"), (1, 0, 0));
        assert_eq!(class("1.9.0"), (1, 0, 0));
        assert_eq!(class("0.4.7"), (0, 4, 0));
        assert_eq!(class("0.0.3"), (0, 0, 3));
        assert_eq!(class("2.0.0"), (2, 0, 0));
    }

    #[test]
    fn expand_features_follows_the_cargo_feature_grammar() {
        let table: BTreeMap<String, Vec<String>> = [
            ("default", vec!["fast"]),
            ("fast", vec!["dep:inner", "extra"]),
            ("extra", vec![]),
            ("forward", vec!["inner/f"]),
            ("weak", vec!["other?/f"]),
        ]
        .into_iter()
        .map(|(name, values)| {
            (
                name.to_owned(),
                values.into_iter().map(str::to_owned).collect(),
            )
        })
        .collect();
        let optional: BTreeSet<String> = ["inner", "other", "renamed"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let set = |names: &[&str]| -> BTreeSet<String> {
            names.iter().map(|name| (*name).to_owned()).collect()
        };

        let defaults = expand_features(&table, &optional, &["default"]);
        assert_eq!(defaults.features, set(&["default", "fast", "extra"]));
        assert_eq!(defaults.optional_dependencies, set(&["inner"]));

        let weak = expand_features(&table, &optional, &["weak"]);
        assert_eq!(weak.optional_dependencies, set(&[]));

        let forward = expand_features(&table, &optional, &["forward"]);
        assert_eq!(forward.optional_dependencies, set(&["inner"]));

        let implicit = expand_features(&table, &optional, &["renamed"]);
        assert_eq!(implicit.optional_dependencies, set(&["renamed"]));

        let missing = expand_features(&table, &optional, &["missing"]);
        assert_eq!(missing.features, set(&[]));
        assert_eq!(missing.optional_dependencies, set(&[]));
    }

    #[test]
    fn trimmable_features_exclude_requested_and_non_default_entries() {
        use crate::finding::Magnitude;
        use crate::knowledge::{CrateEntry, FeatureEntry};

        let feature = |name: &str, default: bool| FeatureEntry {
            name: name.to_owned(),
            default,
            magnitude: Magnitude::Moderate,
            advice: "advice".to_owned(),
            fixable: true,
        };
        let entry = CrateEntry {
            versions: semver::VersionReq::STAR,
            verified_through: Version::new(1, 0, 0),
            magnitude: None,
            dev_only: false,
            features: vec![
                feature("alpha", true),
                feature("beta", true),
                feature("gamma", false),
            ],
        };
        let set = |names: &[&str]| -> BTreeSet<String> {
            names.iter().map(|name| (*name).to_owned()).collect()
        };

        let trimmable =
            trimmable_features(&entry, &set(&["alpha", "beta", "gamma"]), &set(&["beta"]));
        let names: Vec<&str> = trimmable
            .iter()
            .map(|feature| feature.name.as_str())
            .collect();

        assert_eq!(names, ["alpha"]);
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
