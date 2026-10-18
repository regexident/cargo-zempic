//! Detecting which release channel the active Rust toolchain tracks.
//!
//! Advice differs between channels: a nightly-only lint or flag is useless
//! advice on beta and stable, and vice versa, so rules need the channel before
//! they recommend anything. Cargo picks the toolchain from a
//! `rust-toolchain.toml` file found in the directory tree, which overrides
//! whatever `rustup` would select, so that file wins over the host compiler's
//! own version banner.

use camino::Utf8Path;

/// The release channel of the toolchain a build will use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolchainChannel {
    /// A stable release.
    Stable,
    /// A beta release.
    Beta,
    /// A nightly build, whether current or pinned to a date.
    Nightly,
}

impl ToolchainChannel {
    /// Reports whether this channel is [`ToolchainChannel::Nightly`].
    pub fn is_nightly(self) -> bool {
        matches!(self, Self::Nightly)
    }

    /// The channel's lowercase name, as printed in a `rust-toolchain.toml`
    /// channel value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Nightly => "nightly",
        }
    }
}

/// Reads the channel a `rust-toolchain.toml` document declares.
///
/// The channel is the `channel` key of the `[toolchain]` table. A value
/// beginning with `nightly`, such as the dated `nightly-2025-01-01`, is
/// [`ToolchainChannel::Nightly`]; one beginning with `beta` is
/// [`ToolchainChannel::Beta`]; any other string, including an explicit version,
/// is [`ToolchainChannel::Stable`]. Returns `None` when the document declares no
/// `[toolchain]` table, no `channel` key there, a `channel` that is not a
/// string, or is not valid TOML.
pub fn from_toolchain_manifest(text: &str) -> Option<ToolchainChannel> {
    let document = toml_edit::Document::<String>::parse(text.to_owned()).ok()?;

    let channel = document
        .get("toolchain")?
        .as_table()?
        .get("channel")?
        .as_str()?;

    Some(channel_for_prefix(channel))
}

/// Reads the channel from the verbose version banner `rustc -vV` prints.
///
/// A `channel:` line naming `nightly`, `beta` or `stable` decides on its own.
/// Otherwise the `release:` line decides: a `-nightly` suffix is
/// [`ToolchainChannel::Nightly`], a `-beta` suffix is [`ToolchainChannel::Beta`]
/// and any other release is [`ToolchainChannel::Stable`]. Lines other than
/// these are ignored. Returns `None` when the banner carries neither a usable
/// `channel:` line nor a `release:` line.
pub fn from_rustc_version(text: &str) -> Option<ToolchainChannel> {
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(name))
            .map(str::trim)
    };

    if let Some(channel) = field("channel:") {
        match channel_for_prefix(channel) {
            ToolchainChannel::Stable if !channel.starts_with("stable") => {}
            identified => return Some(identified),
        }
    }

    let release = field("release:")?;

    Some(channel_for_suffix(release))
}

/// Detects the channel of the toolchain a build in `workspace_root` will use.
///
/// The document at `<workspace_root>/rust-toolchain.toml` is consulted first,
/// because it overrides toolchain selection for the whole tree below the
/// directory holding it. When the file is absent, unreadable, unparseable or
/// names no channel, the banner of the `rustc` on `PATH` decides. Detection
/// never fails an analysis: an unavailable or unrecognizable banner is reported
/// as [`ToolchainChannel::Stable`], the channel whose advice holds for the
/// widest audience.
pub fn active(workspace_root: &Utf8Path) -> ToolchainChannel {
    std::fs::read_to_string(workspace_root.join("rust-toolchain.toml"))
        .ok()
        .and_then(|text| from_toolchain_manifest(&text))
        .or_else(host_channel)
        .unwrap_or(ToolchainChannel::Stable)
}

/// Reads the channel from the `rustc` on `PATH`.
///
/// Returns `None` when the process cannot be launched, its output is not UTF-8
/// or the banner names no channel.
fn host_channel() -> Option<ToolchainChannel> {
    let output = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .ok()?;
    let banner = String::from_utf8(output.stdout).ok()?;

    from_rustc_version(&banner)
}

/// Classifies a channel name or a release version by its leading word.
///
/// Names and versions share the shape of a channel followed by an optional
/// suffix, as in `nightly-2025-01-01` or `1.86.0-beta.1`, so only the prefix
/// matters. Anything naming neither pre-release channel tracks stable.
fn channel_for_prefix(value: &str) -> ToolchainChannel {
    if value.starts_with("nightly") {
        ToolchainChannel::Nightly
    } else if value.starts_with("beta") {
        ToolchainChannel::Beta
    } else {
        ToolchainChannel::Stable
    }
}

/// Classifies a release version such as `1.86.0-nightly` by its suffix.
fn channel_for_suffix(release: &str) -> ToolchainChannel {
    if release.ends_with("-nightly") {
        ToolchainChannel::Nightly
    } else if release.ends_with("-beta") {
        ToolchainChannel::Beta
    } else {
        ToolchainChannel::Stable
    }
}

#[cfg(test)]
mod tests {
    use super::{ToolchainChannel, from_rustc_version, from_toolchain_manifest};

    const NIGHTLY_BANNER: &str = "rustc 1.86.0-nightly (012345678 2025-01-01)\n\
         binary: rustc\n\
         commit-hash: 0123456789abcdef\n\
         commit-date: 2025-01-01\n\
         host: aarch64-apple-darwin\n\
         release: 1.86.0-nightly\n\
         LLVM version: 20.1.0";

    #[test]
    fn toolchain_manifest_maps_nightly() {
        let channel = from_toolchain_manifest("[toolchain]\nchannel = \"nightly\"\n");

        assert_eq!(channel, Some(ToolchainChannel::Nightly));
    }

    #[test]
    fn toolchain_manifest_maps_dated_nightly() {
        let channel = from_toolchain_manifest("[toolchain]\nchannel = \"nightly-2025-01-01\"\n");

        assert_eq!(channel, Some(ToolchainChannel::Nightly));
    }

    #[test]
    fn toolchain_manifest_maps_beta() {
        let channel = from_toolchain_manifest("[toolchain]\nchannel = \"beta\"\n");

        assert_eq!(channel, Some(ToolchainChannel::Beta));
    }

    #[test]
    fn toolchain_manifest_maps_stable() {
        assert_eq!(
            from_toolchain_manifest("[toolchain]\nchannel = \"stable\"\n"),
            Some(ToolchainChannel::Stable)
        );
        assert_eq!(
            from_toolchain_manifest("[toolchain]\nchannel = \"1.86.0\"\n"),
            Some(ToolchainChannel::Stable)
        );
    }

    #[test]
    fn toolchain_manifest_rejects_missing_channel() {
        assert_eq!(
            from_toolchain_manifest("[toolchain]\ncomponents = [\"rustfmt\"]\n"),
            None
        );
        assert_eq!(from_toolchain_manifest("[profile]\ndebug = true\n"), None);
    }

    #[test]
    fn toolchain_manifest_rejects_malformed_toml() {
        assert_eq!(from_toolchain_manifest("[toolchain\nchannel = "), None);
        assert_eq!(
            from_toolchain_manifest("[toolchain]\nchannel = 1\n"),
            None,
            "a non-string channel names no channel"
        );
    }

    #[test]
    fn rustc_version_maps_nightly_release() {
        let channel = from_rustc_version(NIGHTLY_BANNER);

        assert_eq!(channel, Some(ToolchainChannel::Nightly));
    }

    #[test]
    fn rustc_version_maps_beta_release() {
        let banner = "rustc 1.87.0-beta.2 (012345678 2025-03-01)\n\
             binary: rustc\n\
             commit-hash: 0123456789abcdef\n\
             commit-date: 2025-03-01\n\
             host: aarch64-apple-darwin\n\
             release: 1.87.0-beta\n\
             LLVM version: 20.1.0";

        let channel = from_rustc_version(banner);

        assert_eq!(channel, Some(ToolchainChannel::Beta));
    }

    #[test]
    fn rustc_version_maps_stable_release() {
        let banner = "rustc 1.99.0 (b940084d7 2026-09-28)\n\
             binary: rustc\n\
             commit-hash: b940084d7eb6a299eb4bfeb8e34901bc051e7ac4\n\
             commit-date: 2026-09-28\n\
             host: aarch64-apple-darwin\n\
             release: 1.99.0\n\
             LLVM version: 23.1.1";

        let channel = from_rustc_version(banner);

        assert_eq!(channel, Some(ToolchainChannel::Stable));
    }

    #[test]
    fn rustc_version_rejects_missing_release() {
        let banner = "rustc 1.86.0-nightly (012345678 2025-01-01)\n\
             binary: rustc\n\
             host: aarch64-apple-darwin";

        assert_eq!(from_rustc_version(banner), None);
        assert_eq!(from_rustc_version(""), None);
    }

    #[test]
    fn rustc_version_prefers_the_channel_line() {
        let banner = "rustc 1.87.0-nightly (012345678 2025-03-01)\n\
             binary: rustc\n\
             release: 1.86.0\n\
             channel: nightly\n\
             host: aarch64-apple-darwin\n\
             LLVM version: 20.1.0";

        let channel = from_rustc_version(banner);

        assert_eq!(channel, Some(ToolchainChannel::Nightly));
    }
}
