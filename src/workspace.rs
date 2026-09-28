//! Resolving a cargo workspace into its package graph.
//!
//! Artifact analysis needs the resolved dependency graph, so this module
//! queries `cargo metadata` rather than parsing manifests by hand. Resolution
//! is always narrowed to a single target platform, because metadata otherwise
//! activates every target's dependencies and inflates the graph with crates a
//! build would never compile.

use cargo_metadata::{CargoOpt, Metadata, MetadataCommand};

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

#[cfg(test)]
mod tests {
    use super::{FeatureSelection, MetadataQuery, metadata_command};

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
}
