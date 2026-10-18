//! Static analysis of Cargo manifests and cargo configuration for artifact-size
//! recommendations.
//!
//! The crate exposes its analysis as a library so that integration tests and
//! downstream callers can work with typed results rather than rendered text.
//! Rendering and exit-code mapping live in the binary target.

pub mod cargo_config;
pub mod cli;
pub mod config;
pub mod dimension;
pub mod finding;
pub mod knowledge;
pub mod manifest;
pub mod profile;
pub mod report;
pub mod rules;
pub mod toolchain;
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

    /// Serializing a report to JSON failed.
    #[error("failed to serialize the report to JSON: {source}")]
    ReportSerialize {
        /// Underlying serialization failure.
        source: serde_json::Error,
    },

    /// A configuration file could not be read.
    #[error("failed to read configuration `{path}`: {source}")]
    ConfigRead {
        /// Path of the configuration file that could not be read.
        path: camino::Utf8PathBuf,
        /// Underlying I/O failure.
        source: std::io::Error,
    },

    /// A configuration file could not be parsed or carried an unknown key.
    #[error("failed to parse configuration `{path}`: {source}")]
    ConfigParse {
        /// Path of the configuration file that could not be parsed.
        path: camino::Utf8PathBuf,
        /// Underlying TOML failure.
        source: toml_edit::de::Error,
    },

    /// A configuration file declares an unsupported schema version.
    #[error(
        "configuration `{path}` declares schema version {found}, but only {expected} is supported"
    )]
    ConfigSchemaVersion {
        /// Path of the configuration file.
        path: camino::Utf8PathBuf,
        /// The version the file declares.
        found: u32,
        /// The version this crate supports.
        expected: u32,
    },

    /// A configuration ignore selector is malformed.
    #[error("configuration `{path}` has an invalid selector `{text}`: {source}")]
    ConfigSelector {
        /// Path of the configuration file.
        path: camino::Utf8PathBuf,
        /// The selector as written.
        text: String,
        /// Why the selector is invalid.
        source: crate::finding::SelectorError,
    },

    /// A configuration date is malformed.
    #[error("configuration `{path}` has an invalid value `{found}` for `{key}`")]
    ConfigDate {
        /// Path of the configuration file.
        path: camino::Utf8PathBuf,
        /// The key holding the invalid value.
        key: String,
        /// The value as written.
        found: String,
    },

    /// A configuration names an optimization dimension the crate does not implement.
    #[error("configuration `{path}` names unknown dimension `{found}`")]
    ConfigDimension {
        /// Path of the configuration file.
        path: camino::Utf8PathBuf,
        /// The dimension name that was not recognized.
        found: String,
    },

    /// A configuration names an unknown report format.
    #[error("configuration `{path}` names unknown report format `{found}`")]
    ConfigFormat {
        /// Path of the configuration file.
        path: camino::Utf8PathBuf,
        /// The format name that was not recognized.
        found: String,
    },

    /// A configuration names an unknown fail-on threshold.
    #[error("configuration `{path}` names unknown fail-on threshold `{found}`")]
    ConfigFailOn {
        /// Path of the configuration file.
        path: camino::Utf8PathBuf,
        /// The threshold name that was not recognized.
        found: String,
    },

    /// Reading a cargo configuration file from disk failed.
    #[error("failed to read cargo configuration `{path}`: {source}")]
    CargoConfigRead {
        /// Path of the cargo configuration file that could not be read.
        path: camino::Utf8PathBuf,
        /// Underlying I/O failure.
        source: std::io::Error,
    },

    /// Parsing a cargo configuration file failed.
    #[error("failed to parse cargo configuration `{path}`: {source}")]
    CargoConfigParse {
        /// Path of the cargo configuration file that could not be parsed.
        path: camino::Utf8PathBuf,
        /// Underlying TOML failure.
        source: toml_edit::TomlError,
    },

    /// A knowledge-base document could not be parsed or carried an unknown key.
    #[error("failed to parse knowledge base `{origin}`: {source}")]
    KnowledgeBaseParse {
        /// Where the knowledge base came from, a file path or `<embedded knowledge base>`.
        origin: String,
        /// Underlying TOML failure.
        source: toml_edit::de::Error,
    },

    /// A knowledge-base document declares a `kb_version` this crate does not accept.
    #[error("knowledge base declares `kb_version = {found}`, expected {expected}")]
    KnowledgeBaseVersion {
        /// The `kb_version` the document declares.
        found: u32,
        /// The `kb_version` this crate accepts.
        expected: u32,
    },

    /// A knowledge-base entry holds a value that is not valid.
    #[error("invalid knowledge-base entry `{name}` in `{origin}`: {detail}")]
    KnowledgeBaseEntry {
        /// Where the knowledge base came from, a file path or `<embedded knowledge base>`.
        origin: String,
        /// The crate name of the invalid entry.
        name: String,
        /// What makes the entry invalid.
        detail: String,
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
    /// The profile whose settings govern the analyzed artifacts, or [`None`]
    /// when neither the command line nor a configuration file names one and
    /// [`DEFAULT_PROFILE`] applies.
    pub profile: Option<String>,
    /// The features active during resolution.
    pub features: workspace::FeatureSelection,
    /// The target triple to resolve for, or the host's when unset.
    pub target: Option<String>,
    /// The output format the command line selected, or [`None`] when it did not
    /// and the configuration file may supply one.
    pub format: Option<report::Format>,
    /// The gate threshold the command line selected, or [`None`] when it did not
    /// and the configuration file may supply one.
    pub fail_on: Option<report::FailOn>,
    /// Whether findings render one per line.
    pub quiet: bool,
    /// The gates deciding which findings a run reports.
    pub gates: config::Gates,
    /// The current date, supplied by the caller so ignore expiry is
    /// deterministic.
    pub today: config::CivilDate,
    /// The process state the cargo configuration chain is resolved against.
    pub environment: cargo_config::CargoEnvironment,
    /// The toolchain channel the analysis reasons about.
    ///
    /// [`None`] detects the channel from the workspace root, so a run answers
    /// for the toolchain the project actually builds with. [`Some`] injects a
    /// channel, which lets tests and callers pin the answer instead of
    /// inheriting the machine's.
    pub toolchain: Option<crate::toolchain::ToolchainChannel>,
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

/// The profile a run analyzes when neither the command line nor a configuration
/// file names one.
pub const DEFAULT_PROFILE: &str = "release";

/// The workspace scoped before a profile is resolved.
///
/// Scoping reads `cargo metadata` and selects packages, which requires no
/// configuration; resolving a profile additionally requires the effective
/// profile name, which the configuration can supply. The two are separated so
/// that [`analyze`] can read the configuration between them without loading
/// metadata twice.
struct WorkspaceScope {
    /// The root directory of the analyzed workspace.
    workspace_root: camino::Utf8PathBuf,
    /// The number of packages the workspace contains, selected or not.
    member_count: usize,
    /// The selected packages, in the order the selection returned them.
    selected: Vec<ResolvedPackage>,
    /// The target triple resolution runs for.
    target_triple: String,
}

/// Resolves the workspace an [`AnalysisConfig`] describes into the packages,
/// profile and target an analysis runs against.
///
/// The profile is resolved from the workspace root manifest alone: cargo honors
/// a `[profile.*]` table only there, so a member's own profile table is inert
/// and never consulted. The workspace is read through `cargo metadata`, whose
/// package graph supplies both the selected packages and the member count.
///
/// This entry point honors the arguments alone; the profile is
/// [`DEFAULT_PROFILE`] when the configuration names none. [`analyze`] is the
/// entry point that also reads `zempic.toml` and lets it select the profile and
/// dimensions.
///
/// # Errors
///
/// Returns a [`ZempicError`] when the host triple cannot be read, `cargo
/// metadata` fails, the selection names an unknown member, or the workspace
/// root manifest cannot be read or parsed.
pub fn resolve(config: &AnalysisConfig) -> Result<Resolution> {
    let (scope, _metadata) = scope(config)?;
    let root_manifest = manifest::ManifestDocument::load(&scope.workspace_root.join("Cargo.toml"))?;

    finish(
        scope,
        config.profile.as_deref().unwrap_or(DEFAULT_PROFILE),
        config.dimensions.clone(),
        &root_manifest,
    )
}

/// Reads the workspace's packages and target triple.
///
/// Performs the host-triple resolution, metadata load and package selection
/// that precede profile resolution, so a caller may read the configuration
/// before naming the profile.
///
/// Returns the metadata it read beside the scope so the rules can evaluate the
/// graph without loading it twice.
fn scope(config: &AnalysisConfig) -> Result<(WorkspaceScope, cargo_metadata::Metadata)> {
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

    let workspace_scope = WorkspaceScope {
        workspace_root,
        member_count,
        selected,
        target_triple,
    };

    Ok((workspace_scope, metadata))
}

/// Resolves `profile_name` over `root_manifest` into a [`Resolution`].
///
/// The dimensions default to [`dimension::Dimension::Size`] when `dimensions`
/// is empty and are deduplicated while preserving the given order otherwise.
fn finish(
    scope: WorkspaceScope,
    profile_name: &str,
    dimensions: Vec<dimension::Dimension>,
    root_manifest: &manifest::ManifestDocument,
) -> Result<Resolution> {
    let profile = profile::resolve_profile(root_manifest, profile_name, &scope.target_triple)?;

    let mut seen = std::collections::BTreeSet::new();
    let mut effective = Vec::new();

    for dimension in dimensions {
        if seen.insert(dimension) {
            effective.push(dimension);
        }
    }

    if effective.is_empty() {
        effective.push(dimension::Dimension::Size);
    }

    Ok(Resolution {
        workspace_root: scope.workspace_root,
        member_count: scope.member_count,
        selected: scope.selected,
        profile,
        target_triple: scope.target_triple,
        dimensions: effective,
    })
}

/// Analyzes the workspace an [`AnalysisConfig`] describes and returns a report.
///
/// The analysis resolves the workspace, reads the `zempic.toml` files that
/// configure it, applies the gates to the findings the registry produces, and
/// assembles a report. It never builds, and it never opens a source file: every
/// input is a manifest, a cargo configuration file or the metadata that
/// describes them, so the analysis is read-only and completes without a
/// compiler.
///
/// The analyzed profile is the command line's, else the configuration's, else
/// [`DEFAULT_PROFILE`]; the dimensions are the command line's when non-empty,
/// else the configuration's, else [`dimension::Dimension::Size`].
///
/// The gates are the command line's, else the configuration's `[analysis]`
/// table, else the gate's own default; the ignores are the configuration's
/// `[[ignore]]` entries followed by the command line's.
///
/// # Errors
///
/// Returns a [`ZempicError`] when resolution fails, when a configuration file
/// cannot be read or does not validate, when the embedded knowledge base does
/// not parse (see [`knowledge::KnowledgeBase::embedded`]), or when a produced
/// finding carries no tradeoff.
pub fn analyze(config: &AnalysisConfig) -> Result<report::Report> {
    let (workspace_scope, metadata) = scope(config)?;

    let package_directories: Vec<camino::Utf8PathBuf> = workspace_scope
        .selected
        .iter()
        .filter_map(|package| {
            package
                .manifest_path
                .parent()
                .map(camino::Utf8Path::to_owned)
        })
        .collect();

    let file_config = config::discover(&workspace_scope.workspace_root, &package_directories)?;

    let mut knowledge_base = knowledge::KnowledgeBase::embedded()?;
    knowledge_base.extend(file_config.knowledge_base);

    let profile_name = config
        .profile
        .as_deref()
        .or(file_config.analysis.profile.as_deref())
        .unwrap_or(DEFAULT_PROFILE)
        .to_owned();

    let dimensions = if config.dimensions.is_empty() {
        file_config
            .analysis
            .optimize_for
            .clone()
            .unwrap_or_else(|| vec![dimension::Dimension::Size])
    } else {
        config.dimensions.clone()
    };

    // The configured format and threshold are the base the command line merges
    // over; `None` from the command line means the file, or the default, wins.
    let settings = report::ReportSettings {
        format: config
            .format
            .or(file_config.report.format)
            .unwrap_or_default(),
        fail_on: config
            .fail_on
            .or(file_config.report.fail_on)
            .unwrap_or_default(),
        quiet: config.quiet,
    };

    let effective_gates = config::Gates {
        ignore: file_config
            .ignore
            .into_iter()
            .chain(config.gates.ignore.iter().cloned())
            .collect(),
        ..config::resolve_gates(&config.gates, &file_config.analysis)
    };

    let toolchain = config
        .toolchain
        .unwrap_or_else(|| crate::toolchain::active(&workspace_scope.workspace_root));
    let allow_nightly = effective_gates.allow_nightly.unwrap_or(false);

    let root_manifest =
        manifest::ManifestDocument::load(&workspace_scope.workspace_root.join("Cargo.toml"))?;
    let resolution = finish(workspace_scope, &profile_name, dimensions, &root_manifest)?;

    let member_manifests = member_manifests(&resolution)?;
    let cargo_configs = package_rustflags(&resolution, &config.environment)?;
    let findings = rules::evaluate(
        &resolution,
        &metadata,
        &knowledge_base,
        &root_manifest,
        &member_manifests,
        &cargo_configs,
        toolchain,
        allow_nightly,
    );
    let filtered = config::filter(findings, &effective_gates, config.today);

    let mut notices = filtered.notices;

    // The rule cannot emit a notice itself, so the run reports the skip from the
    // same predicate the rule uses, so the notice and the skip cannot disagree.
    if rules::profile::panic_strategy_is_inert(&resolution.profile) {
        notices.push(report::Notice::SkippedPanicStrategy {
            profile: inert_profile_name(&resolution.profile),
        });
    }

    report::Report::new(resolution, filtered.findings, Vec::new(), notices, settings)
}

/// The profile name the panic-strategy skip is reported under.
///
/// Names the analyzed profile's own name when it is `test` or `bench`, else the
/// inherited profile in its chain that named one.
fn inert_profile_name(profile: &profile::ResolvedProfile) -> String {
    if matches!(profile.name.as_str(), "test" | "bench") {
        return profile.name.clone();
    }

    profile
        .inherits_chain
        .iter()
        .find(|name| matches!(name.as_str(), "test" | "bench"))
        .cloned()
        .unwrap_or_else(|| profile.name.clone())
}

/// Parses each selected member's manifest, excluding the workspace root's.
///
/// A member whose manifest path is the workspace root's `Cargo.toml` is the root
/// package of a non-virtual workspace, and its `[profile.*]` table is the one
/// cargo honors, so including it would invert the meaning of the rule that reads
/// this list. An unreadable or unparseable member manifest fails the analysis
/// the same way the root's does.
///
/// # Errors
///
/// Returns [`ZempicError::ManifestRead`](crate::ZempicError::ManifestRead) or
/// [`ZempicError::ManifestParse`](crate::ZempicError::ManifestParse) naming a
/// member manifest that cannot be read or parsed.
pub fn member_manifests(
    resolution: &Resolution,
) -> Result<Vec<(String, manifest::ManifestDocument)>> {
    let root_manifest_path = resolution.workspace_root.join("Cargo.toml");
    let mut manifests = Vec::new();

    for package in &resolution.selected {
        if package.manifest_path == root_manifest_path {
            continue;
        }

        manifests.push((
            package.name.clone(),
            manifest::ManifestDocument::load(&package.manifest_path)?,
        ));
    }

    Ok(manifests)
}

/// Resolves the cargo configuration rustflags for every selected package.
///
/// A cargo configuration chain is resolved per package directory, and the
/// root package of a non-virtual workspace has a package directory like any
/// other, so every entry of `resolution.selected` is resolved here, including
/// the root's — unlike [`member_manifests`], which excludes it because that
/// list concerns a `[profile.*]` table cargo only honors in the root.
///
/// # Errors
///
/// Returns whatever [`cargo_config::resolve_for_package`] returns for a
/// `config.toml` found in a package's chain that exists but cannot be read or
/// parsed.
pub fn package_rustflags(
    resolution: &Resolution,
    environment: &cargo_config::CargoEnvironment,
) -> Result<Vec<(String, cargo_config::ResolvedRustflags)>> {
    resolution
        .selected
        .iter()
        .map(|package| {
            let package_directory = package
                .manifest_path
                .parent()
                .map(camino::Utf8Path::to_owned)
                .unwrap_or_else(|| resolution.workspace_root.clone());

            let resolved = cargo_config::resolve_for_package(
                &package_directory,
                environment,
                &resolution.target_triple,
            )?;

            Ok((package.name.clone(), resolved))
        })
        .collect()
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
    use super::{
        AnalysisConfig, cargo_config::CargoEnvironment, dimension::Dimension, metadata_query,
        workspace,
    };

    #[test]
    fn metadata_query_filters_by_the_resolved_target_triple() {
        let config = AnalysisConfig {
            dimensions: vec![Dimension::Size],
            manifest_path: Some(camino::Utf8PathBuf::from("/probe/Cargo.toml")),
            selection: workspace::PackageSelection::default(),
            profile: None,
            features: workspace::FeatureSelection::default(),
            target: Some("aarch64-unknown-linux-gnu".to_owned()),
            format: None,
            fail_on: None,
            quiet: false,
            gates: crate::config::Gates::default(),
            today: crate::config::CivilDate::today(),
            environment: CargoEnvironment::default(),
            toolchain: None,
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
