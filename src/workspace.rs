//! Resolving a cargo workspace into its package graph.
//!
//! Artifact analysis needs the resolved dependency graph, so this module
//! queries `cargo metadata` rather than parsing manifests by hand. Resolution
//! is always narrowed to a single target platform, because metadata otherwise
//! activates every target's dependencies and inflates the graph with crates a
//! build would never compile.

use cargo_metadata::{CargoOpt, Metadata, MetadataCommand, TargetKind};

/// The feature flags a `cargo metadata` invocation should activate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeatureSelection {
    /// Features to enable by name, passed as a single comma-separated value.
    pub features: Vec<String>,
    /// Whether to enable every feature of every selected package.
    pub all_features: bool,
    /// Whether to disable the default feature set.
    pub no_default_features: bool,
}

/// A `cargo metadata` request narrowed to one target platform.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetadataQuery {
    /// Manifest to resolve, or the current directory's manifest when absent.
    pub manifest_path: Option<camino::Utf8PathBuf>,
    /// Target triple to resolve for, or the host triple when absent.
    pub target: Option<String>,
    /// Feature flags to activate.
    pub features: FeatureSelection,
}

/// Reads the host target triple from the `host:` line of `rustc -vV`.
pub fn host_target_triple() -> crate::Result<String> {
    let output = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|source| crate::ZempicError::HostTripleUnavailable {
            detail: source.to_string(),
        })?;

    let stdout = String::from_utf8(output.stdout).map_err(|source| {
        crate::ZempicError::HostTripleUnavailable {
            detail: source.to_string(),
        }
    })?;

    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(|host| host.trim().to_owned())
        .ok_or_else(|| crate::ZempicError::HostTripleUnavailable {
            detail: "`rustc -vV` output carries no `host:` line".to_owned(),
        })
}

/// Builds the `cargo metadata` invocation a query describes.
///
/// The platform filter is always the query's target triple, falling back to
/// `host_triple`, so resolution never activates other targets' dependencies.
pub(crate) fn metadata_command(query: &MetadataQuery, host_triple: &str) -> MetadataCommand {
    let mut command = MetadataCommand::new();

    if let Some(manifest_path) = &query.manifest_path {
        command.manifest_path(manifest_path);
    }

    if !query.features.features.is_empty() {
        command.features(CargoOpt::SomeFeatures(query.features.features.clone()));
    }
    if query.features.all_features {
        command.features(CargoOpt::AllFeatures);
    }
    if query.features.no_default_features {
        command.features(CargoOpt::NoDefaultFeatures);
    }

    let triple = query.target.as_deref().unwrap_or(host_triple);
    command.other_options(["--filter-platform".to_owned(), triple.to_owned()]);

    command
}

/// Resolves the package graph a query describes.
pub fn load_metadata(query: &MetadataQuery) -> crate::Result<Metadata> {
    let host_triple = match &query.target {
        Some(target) => target.clone(),
        None => host_target_triple()?,
    };

    metadata_command(query, &host_triple)
        .exec()
        .map_err(|source| crate::ZempicError::Metadata { source })
}

/// The workspace members a run should analyze.
///
/// Mirrors cargo's `-p/--package`, `--workspace` and `--exclude` surface: an
/// empty `packages` list with `workspace` unset falls back to the workspace's
/// default members.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageSelection {
    /// Package specs named by `-p/--package`, matched against package names.
    pub packages: Vec<String>,
    /// Whether `--workspace`/`--all` selects every workspace member.
    pub workspace: bool,
    /// Package specs named by `--exclude`.
    pub exclude: Vec<String>,
}

/// Resolves a selection to workspace packages, ordered by name.
///
/// When `selection.workspace` is set, every workspace member is a candidate;
/// otherwise the named `selection.packages` are, or the workspace default
/// members when no names are given. `selection.exclude` is only meaningful
/// alongside `--workspace`, matching cargo's own argument contract. Every spec
/// must name a workspace member: cargo's richer package-spec grammar is not
/// interpreted, so a spec that carries a version, source or path qualifier is
/// reported as unknown rather than partially matched.
pub fn select_packages<'a>(
    metadata: &'a cargo_metadata::Metadata,
    selection: &PackageSelection,
) -> crate::Result<Vec<&'a cargo_metadata::Package>> {
    if !selection.exclude.is_empty() && !selection.workspace {
        return Err(crate::ZempicError::ExcludeWithoutWorkspace);
    }

    let members: Vec<&cargo_metadata::Package> = metadata
        .packages
        .iter()
        .filter(|package| metadata.workspace_members.contains(&package.id))
        .collect();

    let named: Vec<&cargo_metadata::Package> = selection
        .packages
        .iter()
        .map(|spec| resolve_member(&members, spec))
        .collect::<crate::Result<_>>()?;

    let candidates: Vec<&cargo_metadata::Package> = if selection.workspace {
        members.clone()
    } else if named.is_empty() {
        members
            .iter()
            .filter(|package| metadata.workspace_default_members.contains(&package.id))
            .copied()
            .collect()
    } else {
        named
    };

    let mut selected = Vec::new();
    for package in candidates {
        let excluded = selection
            .exclude
            .iter()
            .any(|spec| package.name.as_ref() == spec);
        if !excluded {
            selected.push(package);
        }
    }

    for spec in &selection.exclude {
        resolve_member(&members, spec)?;
    }

    selected.sort_by(|left, right| left.name.as_ref().cmp(right.name.as_ref()));

    Ok(selected)
}

/// Finds the workspace member a spec names exactly.
///
/// Only the package name is matched; a spec that carries any qualifier (`@`,
/// `:` or `/`) names cargo's broader package-spec grammar, which this crate
/// deliberately does not interpret.
fn resolve_member<'a>(
    members: &[&'a cargo_metadata::Package],
    spec: &str,
) -> crate::Result<&'a cargo_metadata::Package> {
    if spec.contains(['@', ':', '/']) {
        return Err(crate::ZempicError::UnknownPackage {
            spec: spec.to_owned(),
        });
    }

    members
        .iter()
        .copied()
        .find(|package| package.name.as_ref() == spec)
        .ok_or_else(|| crate::ZempicError::UnknownPackage {
            spec: spec.to_owned(),
        })
}

/// Whether the analyzed profile governs the size of a package's artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactKind {
    /// The profile being analyzed governs this artifact's size, because the
    /// package emits a linked artifact — a binary or a `cdylib`/`dylib`/
    /// `staticlib` — that the profile controls directly.
    Linked,
    /// The profile being analyzed does not govern this artifact's size: a Rust
    /// library or `rlib` is shaped by the consumer's profile, and a procedural
    /// macro is a host artifact built for the compiling machine.
    Unlinked,
}

/// The classification of one target within a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetClassification {
    /// The target's name as cargo reports it.
    pub name: String,
    /// The target's cargo kinds, in the order cargo reports them.
    pub kinds: Vec<TargetKind>,
    /// Whether the analyzed profile governs this target's artifact.
    pub artifact: ArtifactKind,
}

/// A package's targets classified by the artifact each produces.
///
/// Targets whose artifact the analyzed profile never governs — tests, benches,
/// examples and custom build scripts — are omitted rather than classified, so
/// the retained targets are exactly those the profile can shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageClassification {
    /// The classified package's id.
    pub id: cargo_metadata::PackageId,
    /// The classified package's name.
    pub name: String,
    /// The package's profile-governed targets, in the order cargo reports them.
    pub targets: Vec<TargetClassification>,
    /// Target kinds cargo reported that this crate does not recognize,
    /// deduplicated and sorted.
    pub unrecognized_kinds: Vec<String>,
}

impl PackageClassification {
    /// Whether any retained target emits an artifact the profile governs.
    pub fn produces_linked_artifact(&self) -> bool {
        self.targets
            .iter()
            .any(|target| target.artifact == ArtifactKind::Linked)
    }

    /// The retained targets whose artifact the profile governs.
    pub fn linked_targets(&self) -> impl Iterator<Item = &TargetClassification> {
        self.targets
            .iter()
            .filter(|target| target.artifact == ArtifactKind::Linked)
    }
}

/// Classifies a package's targets by whether the analyzed profile governs them.
///
/// The profile governs a linked artifact — a binary, `cdylib`, `dylib` or
/// `staticlib` — but not a Rust library, `rlib` or procedural macro, whose
/// size the consumer's profile decides. A kind this crate does not recognize
/// is treated as unlinked and recorded, so an unrecognized target is visible
/// rather than silently dropped.
pub fn classify_package(package: &cargo_metadata::Package) -> PackageClassification {
    let mut targets = Vec::new();
    let mut unrecognized = Vec::new();

    for target in &package.targets {
        if target.kind.iter().any(is_unclassified_kind) {
            continue;
        }

        for kind in &target.kind {
            if let TargetKind::Unknown(name) = kind {
                unrecognized.push(name.clone());
            }
        }

        let artifact = if target.kind.iter().any(is_linked_kind) {
            ArtifactKind::Linked
        } else {
            ArtifactKind::Unlinked
        };

        targets.push(TargetClassification {
            name: target.name.clone(),
            kinds: target.kind.clone(),
            artifact,
        });
    }

    unrecognized.sort();
    unrecognized.dedup();

    PackageClassification {
        id: package.id.clone(),
        name: package.name.to_string(),
        targets,
        unrecognized_kinds: unrecognized,
    }
}

/// Whether a kind marks a target the analyzed profile never governs.
fn is_unclassified_kind(kind: &TargetKind) -> bool {
    matches!(
        kind,
        TargetKind::Test | TargetKind::Bench | TargetKind::Example | TargetKind::CustomBuild
    )
}

/// Whether a kind marks a target that emits a linked artifact.
fn is_linked_kind(kind: &TargetKind) -> bool {
    matches!(
        kind,
        TargetKind::Bin | TargetKind::CDyLib | TargetKind::DyLib | TargetKind::StaticLib
    )
}

#[cfg(test)]
mod tests {
    use super::{FeatureSelection, MetadataQuery, metadata_command};
    use crate::workspace::{
        ArtifactKind, PackageClassification, classify_package, is_linked_kind, is_unclassified_kind,
    };
    use cargo_metadata::{PackageId, Target, TargetKind};

    const HOST: &str = "x86_64-unknown-linux-gnu";

    fn args_of(query: &MetadataQuery) -> Vec<String> {
        metadata_command(query, HOST)
            .cargo_command()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn filter_platform_uses_the_host_triple_when_no_target_is_given() {
        let args = args_of(&MetadataQuery::default());

        assert_eq!(args.first().map(String::as_str), Some("metadata"));
        assert_eq!(args[1..3], ["--format-version", "1"]);
        assert_eq!(
            args[args.len() - 2..],
            ["--filter-platform", "x86_64-unknown-linux-gnu"]
        );
    }

    #[test]
    fn filter_platform_uses_the_explicit_target_when_given() {
        let query = MetadataQuery {
            target: Some("aarch64-apple-darwin".to_owned()),
            ..MetadataQuery::default()
        };

        let args = args_of(&query);

        assert_eq!(
            args[args.len() - 2..],
            ["--filter-platform", "aarch64-apple-darwin"]
        );
    }

    #[test]
    fn feature_list_is_passed_as_a_comma_separated_value() {
        let query = MetadataQuery {
            features: FeatureSelection {
                features: vec!["alpha".to_owned(), "beta".to_owned()],
                ..FeatureSelection::default()
            },
            ..MetadataQuery::default()
        };

        let args = args_of(&query);

        assert_eq!(
            args,
            [
                "metadata",
                "--format-version",
                "1",
                "--features",
                "alpha,beta",
                "--filter-platform",
                HOST,
            ]
        );
    }

    #[test]
    fn all_features_and_no_default_features_are_passed_once_each() {
        let query = MetadataQuery {
            features: FeatureSelection {
                all_features: true,
                no_default_features: true,
                ..FeatureSelection::default()
            },
            ..MetadataQuery::default()
        };

        let args = args_of(&query);

        assert_eq!(
            args.iter().filter(|arg| *arg == "--all-features").count(),
            1
        );
        assert_eq!(
            args.iter()
                .filter(|arg| *arg == "--no-default-features")
                .count(),
            1
        );
    }

    #[test]
    fn manifest_path_is_passed_when_given() {
        let query = MetadataQuery {
            manifest_path: Some(camino::Utf8PathBuf::from("/tmp/demo/Cargo.toml")),
            ..MetadataQuery::default()
        };

        let args = args_of(&query);

        assert!(
            args.windows(2)
                .any(|pair| pair == ["--manifest-path", "/tmp/demo/Cargo.toml"])
        );
    }

    #[test]
    fn no_deps_is_never_passed() {
        let configurations = [
            MetadataQuery::default(),
            MetadataQuery {
                target: Some("aarch64-apple-darwin".to_owned()),
                ..MetadataQuery::default()
            },
            MetadataQuery {
                manifest_path: Some(camino::Utf8PathBuf::from("/tmp/demo/Cargo.toml")),
                ..MetadataQuery::default()
            },
            MetadataQuery {
                features: FeatureSelection {
                    features: vec!["alpha".to_owned()],
                    all_features: true,
                    no_default_features: true,
                },
                ..MetadataQuery::default()
            },
        ];

        for query in &configurations {
            assert!(!args_of(query).iter().any(|arg| arg == "--no-deps"));
        }
    }

    /// A package carrying the two targets required to observe target
    /// classification through the public entry point.
    fn package_with_targets(name: &str, targets: [Target; 2]) -> cargo_metadata::Package {
        cargo_metadata::PackageBuilder::new(
            cargo_metadata::PackageName::new(name.to_owned()),
            cargo_metadata::semver::Version::new(0, 1, 0),
            PackageId {
                repr: format!("path+file:///probe/{name}#0.1.0"),
            },
            camino::Utf8PathBuf::from(format!("/probe/{name}/Cargo.toml")),
        )
        .targets(targets.to_vec())
        .build()
        .expect("both targets are supplied")
    }

    /// A target whose literal name says where it belongs in the fixture's
    /// `targets` vector, so a preserved order is observable.
    fn named_target(name: &str, kind: TargetKind) -> Target {
        cargo_metadata::TargetBuilder::default()
            .name(name.to_owned())
            .kind(vec![kind])
            .src_path(camino::Utf8PathBuf::from(format!("/probe/{name}.rs")))
            .build()
            .expect("name, kind and source path are supplied")
    }

    #[test]
    fn unclassified_kinds_are_auxiliary_targets_only() {
        for kind in [
            TargetKind::Test,
            TargetKind::Bench,
            TargetKind::Example,
            TargetKind::CustomBuild,
        ] {
            assert!(is_unclassified_kind(&kind), "{kind:?} is auxiliary");
        }

        for kind in [
            TargetKind::Bin,
            TargetKind::Lib,
            TargetKind::RLib,
            TargetKind::DyLib,
            TargetKind::CDyLib,
            TargetKind::StaticLib,
            TargetKind::ProcMacro,
            TargetKind::Unknown("mystery".to_owned()),
        ] {
            assert!(!is_unclassified_kind(&kind), "{kind:?} is classified");
        }
    }

    #[test]
    fn linked_kinds_are_linked_artifacts_only() {
        for kind in [
            TargetKind::Bin,
            TargetKind::CDyLib,
            TargetKind::DyLib,
            TargetKind::StaticLib,
        ] {
            assert!(is_linked_kind(&kind), "{kind:?} is linked");
        }

        for kind in [
            TargetKind::Lib,
            TargetKind::RLib,
            TargetKind::ProcMacro,
            TargetKind::Test,
            TargetKind::Bench,
            TargetKind::Example,
            TargetKind::CustomBuild,
            TargetKind::Unknown("mystery".to_owned()),
        ] {
            assert!(!is_linked_kind(&kind), "{kind:?} is unlinked");
        }
    }

    #[test]
    fn unknown_kinds_are_recorded_once_and_sorted() {
        let package = package_with_targets(
            "probe",
            [
                named_target("first", TargetKind::Unknown("zebra-kind".to_owned())),
                named_target("second", TargetKind::Unknown("alpaca-kind".to_owned())),
            ],
        );

        let classification = classify_package(&package);

        assert_eq!(
            classification.unrecognized_kinds,
            ["alpaca-kind", "zebra-kind"]
        );
        assert_eq!(classification.targets.len(), 2);
        assert_eq!(classification.targets[0].name, "first");
        assert_eq!(classification.targets[1].name, "second");
        assert_eq!(classification.targets[0].artifact, ArtifactKind::Unlinked);
        assert_eq!(classification.targets[1].artifact, ArtifactKind::Unlinked);
        assert!(!classification.produces_linked_artifact());
    }

    #[test]
    fn repeated_unknown_kinds_are_deduplicated() {
        let package = package_with_targets(
            "probe",
            [
                named_target("first", TargetKind::Unknown("mystery-kind".to_owned())),
                named_target("second", TargetKind::Unknown("mystery-kind".to_owned())),
            ],
        );

        let classification = classify_package(&package);

        assert_eq!(classification.unrecognized_kinds, ["mystery-kind"]);
    }

    #[test]
    fn recognized_targets_are_retained_and_ordered() {
        let package = package_with_targets(
            "probe",
            [
                named_target("first", TargetKind::Bin),
                named_target("second", TargetKind::Lib),
            ],
        );

        let classification = classify_package(&package);

        assert!(classification.unrecognized_kinds.is_empty());

        let names: Vec<&str> = classification
            .targets
            .iter()
            .map(|target| target.name.as_str())
            .collect();

        assert_eq!(names, ["first", "second"]);
        assert_eq!(classification.targets[0].artifact, ArtifactKind::Linked);
        assert_eq!(classification.targets[1].artifact, ArtifactKind::Unlinked);
        assert!(classification.produces_linked_artifact());
    }

    #[test]
    fn classification_preserves_package_identity() {
        let package = package_with_targets(
            "probe",
            [
                named_target("first", TargetKind::Bin),
                named_target("second", TargetKind::Lib),
            ],
        );

        let expected: PackageClassification = PackageClassification {
            id: package.id.clone(),
            name: "probe".to_owned(),
            targets: Vec::new(),
            unrecognized_kinds: Vec::new(),
        };

        let classification = classify_package(&package);

        assert_eq!(classification.id, expected.id);
        assert_eq!(classification.name, expected.name);
    }
}
