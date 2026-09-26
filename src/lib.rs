//! Static analysis of Cargo manifests and cargo configuration for artifact-size
//! recommendations.
//!
//! The crate exposes its analysis as a library so that integration tests and
//! downstream callers can work with typed results rather than rendered text.
//! Rendering and exit-code mapping live in the binary target.

pub mod manifest;
pub mod profile;

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
}
