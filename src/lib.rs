//! Static analysis of Cargo manifests and cargo configuration for artifact-size
//! recommendations.
//!
//! The crate exposes its analysis as a library so that integration tests and
//! downstream callers can work with typed results rather than rendered text.
//! Rendering and exit-code mapping live in the binary target.

pub mod manifest;
pub mod profile;
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
}
