//! Static analysis of Cargo manifests and cargo configuration for artifact-size
//! recommendations.
//!
//! The crate exposes its analysis as a library so that integration tests and
//! downstream callers can work with typed results rather than rendered text.
//! Rendering and exit-code mapping live in the binary target.

pub mod cli;
pub mod dimension;
pub mod finding;
pub mod manifest;
pub mod profile;
pub mod report;
pub mod workspace;

/// The result type used throughout the crate, carrying [`ZempicError`].
pub type Result<T> = std::result::Result<T, ZempicError>;

/// The error type shared by every fallible operation in the crate.
#[derive(Debug, thiserror::Error)]
pub enum ZempicError {
    /// Reading a manifest file from disk failed.
    #[error("failed to read manifest `{path}`: {source}")]
    ManifestRead {
        /// Path of the manifest that could not be read.
        path: camino::Utf8PathBuf,
        /// Underlying I/O failure.
        source: std::io::Error,
    },

    /// Parsing a manifest file failed.
    #[error("failed to parse manifest `{path}`: {source}")]
    ManifestParse {
        /// Path of the manifest that could not be parsed.
        path: camino::Utf8PathBuf,
        /// Underlying TOML failure.
        source: toml_edit::TomlError,
    },

    /// A profile setting holds a value cargo does not accept.
    #[error("invalid value `{found}` for `{key}` in manifest `{path}`")]
    ProfileValue {
        /// Path of the manifest carrying the setting.
        path: camino::Utf8PathBuf,
        /// Dotted key of the offending setting, for example `profile.release.lto`.
        key: String,
        /// Textual form of the value that was found.
        found: String,
    },

    /// A profile name contains a character cargo does not accept.
    #[error("invalid profile name `{name}` in manifest `{path}`")]
    ProfileName {
        /// Path of the manifest declaring the profile.
        path: camino::Utf8PathBuf,
        /// The offending profile name.
        name: String,
    },

    /// A requested profile is neither a built-in nor declared in the manifest.
    #[error("profile `{name}` is not defined in manifest `{path}`")]
    UnknownProfile {
        /// Path of the manifest that was searched.
        path: camino::Utf8PathBuf,
        /// The requested profile name.
        name: String,
    },

    /// A declared non-built-in profile has no `inherits` key.
    #[error("profile `{name}` in manifest `{path}` must declare `inherits`")]
    CustomProfileMissingInherits {
        /// Path of the manifest declaring the profile.
        path: camino::Utf8PathBuf,
        /// The profile missing its `inherits` key.
        name: String,
    },

    /// A profile's `inherits` names neither a built-in nor a declared profile.
    #[error("profile `{profile}` in manifest `{path}` inherits from unknown profile `{inherits}`")]
    UnknownInheritedProfile {
        /// Path of the manifest declaring the profile.
        path: camino::Utf8PathBuf,
        /// The profile whose `inherits` value is unknown.
        profile: String,
        /// The unknown name the profile inherits from.
        inherits: String,
    },

    /// Following `inherits` revisited a profile, so the chain never terminates.
    #[error("profile `inherits` chain in manifest `{path}` forms a cycle: {}", chain.join(" -> "))]
    ProfileInheritsCycle {
        /// Path of the manifest declaring the profiles.
        path: camino::Utf8PathBuf,
        /// Profile names in traversal order, ending with the name that repeated.
        chain: Vec<String>,
    },

    /// Resolving the package graph through `cargo metadata` failed.
    #[error("failed to resolve cargo metadata: {source}")]
    Metadata {
        /// Underlying `cargo metadata` failure.
        source: cargo_metadata::Error,
    },

    /// The host target triple could not be read from `rustc -vV`.
    #[error("could not determine the host target triple: {detail}")]
    HostTripleUnavailable {
        /// What prevented reading the triple.
        detail: String,
    },

    /// `--exclude` was given without `--workspace`.
    #[error("`--exclude` requires `--workspace`")]
    ExcludeWithoutWorkspace,

    /// A `--package` or `--exclude` spec names no workspace member.
    #[error("package `{spec}` is not a member of the workspace")]
    UnknownPackage {
        /// The spec that matched no workspace member.
        spec: String,
    },

    /// A finding reached a report without the mandatory tradeoff.
    #[error(
        "rule `{rule}` produced a finding with no tradeoff; a finding must state the cost of acting on it"
    )]
    MissingTradeoff {
        /// The rule identifier that produced the finding.
        rule: String,
    },
}

/// The inputs a resolution runs from.
///
/// Each field selects something the resolution honors: which packages of the
/// workspace to analyze, which profile governs their artifacts, which features
/// are active, which target the resolution is for, and which optimization
/// dimensions the run answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisConfig {
    /// The optimization objectives to answer. A run defaults to
    /// [`Dimension::Size`] when this is empty.
    pub dimensions: Vec<dimension::Dimension>,
    /// The manifest to resolve from, or the current directory's when unset.
    pub manifest_path: Option<camino::Utf8PathBuf>,
    /// The workspace members to analyze.
    pub selection: workspace::PackageSelection,
    /// The profile whose settings govern the analyzed artifacts.
    pub profile: String,
    /// The features active during resolution.
    pub features: workspace::FeatureSelection,
    /// The target triple to resolve for, or the host's when unset.
    pub target: Option<String>,
}

/// A selected package and the classification of its targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPackage {
    /// The package's name.
    pub name: String,
    /// The package's manifest path.
    pub manifest_path: camino::Utf8PathBuf,
    /// The package's targets classified by the artifact each produces.
    pub classification: workspace::PackageClassification,
}

/// A workspace resolved against an [`AnalysisConfig`].
///
/// Carries the workspace root and member count so a caller can report the
/// scope of the analysis, the selected packages with their classifications,
/// the resolved profile, the target triple resolution ran for, and the
/// dimensions the run answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// The root directory of the analyzed workspace.
    pub workspace_root: camino::Utf8PathBuf,
    /// The number of packages the workspace contains, selected or not.
    pub member_count: usize,
    /// The selected packages, in the order the selection returned them.
    pub selected: Vec<ResolvedPackage>,
    /// The profile resolved from the workspace root manifest.
    pub profile: profile::ResolvedProfile,
    /// The target triple the resolution was performed for.
    pub target_triple: String,
    /// The optimization objectives the run answers.
    pub dimensions: Vec<dimension::Dimension>,
}

/// Resolves the workspace an [`AnalysisConfig`] describes into the packages,
/// profile and target an analysis runs against.
///
/// The profile is resolved from the workspace root manifest alone: cargo honors
/// a `[profile.*]` table only there, so a member's own profile table is inert
/// and never consulted. The workspace is read through `cargo metadata`, whose
/// package graph supplies both the selected packages and the member count.
///
/// # Errors
///
/// Returns a [`ZempicError`] when the host triple cannot be read, `cargo
/// metadata` fails, the selection names an unknown member, or the workspace
/// root manifest cannot be read or parsed.
pub fn resolve(config: &AnalysisConfig) -> Result<Resolution> {
    let target_triple = match &config.target {
        Some(target) => target.clone(),
        None => workspace::host_target_triple()?,
    };

    let metadata = workspace::load_metadata(&metadata_query(config, &target_triple))?;

    let workspace_root = metadata.workspace_root.clone();
    let member_count = metadata.workspace_members.len();

    let selected = workspace::select_packages(&metadata, &config.selection)?
        .into_iter()
        .map(|package| ResolvedPackage {
            name: package.name.to_string(),
            manifest_path: package.manifest_path.clone(),
            classification: workspace::classify_package(package),
        })
        .collect();

    let root_manifest = manifest::ManifestDocument::load(&workspace_root.join("Cargo.toml"))?;
    let profile = profile::resolve_profile(&root_manifest, &config.profile)?;

    let dimensions = if config.dimensions.is_empty() {
        vec![dimension::Dimension::Size]
    } else {
        config.dimensions.clone()
    };

    Ok(Resolution {
        workspace_root,
        member_count,
        selected,
        profile,
        target_triple,
        dimensions,
    })
}

/// Builds the [`workspace::MetadataQuery`] a configuration resolves through.
///
/// The query's `target` is always `Some`, carrying the resolved triple rather
/// than the raw `config.target`, so the metadata platform filter is the triple
/// resolution reports: a query left with `target: None` would fall back to the
/// host triple and silently filter for the wrong platform when a non-host
/// target is requested.
pub(crate) fn metadata_query(
    config: &AnalysisConfig,
    target_triple: &str,
) -> workspace::MetadataQuery {
    workspace::MetadataQuery {
        manifest_path: config.manifest_path.clone(),
        target: Some(target_triple.to_owned()),
        features: config.features.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{AnalysisConfig, dimension::Dimension, metadata_query, workspace};

    #[test]
    fn metadata_query_filters_by_the_resolved_target_triple() {
        let config = AnalysisConfig {
            dimensions: vec![Dimension::Size],
            manifest_path: Some(camino::Utf8PathBuf::from("/probe/Cargo.toml")),
            selection: workspace::PackageSelection::default(),
            profile: "release".to_owned(),
            features: workspace::FeatureSelection::default(),
            target: Some("aarch64-unknown-linux-gnu".to_owned()),
        };

        let query = metadata_query(&config, "aarch64-unknown-linux-gnu");

        // A query whose `target` is `None` would filter `cargo metadata` by the
        // host triple instead of the requested one, silently analyzing the wrong
        // platform; the resolved triple must be the query's platform filter.
        assert_eq!(query.target.as_deref(), Some("aarch64-unknown-linux-gnu"));
        assert_eq!(query.manifest_path, config.manifest_path);
        assert_eq!(query.features, config.features);
    }
}
