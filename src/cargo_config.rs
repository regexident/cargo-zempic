//! Resolving which rustflags source cargo actually uses.
//!
//! Cargo takes rustflags from exactly one of three ranks: the `RUSTFLAGS`
//! environment variable, the `CARGO_ENCODED_RUSTFLAGS` environment variable, or
//! a `build.rustflags` key in a `config.toml` found by walking from the
//! manifest directory up to the filesystem root (or a `CARGO_HOME` override).
//! Cargo never merges flags across these ranks: the first one present wins and
//! the rest are ignored outright. An analysis that read a lower-ranked source
//! while a higher one was set would report rustflags cargo never applies, so
//! the rank that wins must be resolved before any rank's content is read.

/// The process state the cargo configuration chain is resolved against.
///
/// Carries the two rustflags environment variables cargo consults ahead of any
/// configuration file, and the inputs needed to locate `CARGO_HOME`'s
/// `config.toml`, the chain's final link. [`from_process`](Self::from_process)
/// reads the process environment alone, so resolution over this record remains
/// a pure function of its fields and is testable without mutating process-wide
/// state from parallel tests.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CargoEnvironment {
    /// The `RUSTFLAGS` variable's value, or [`None`] when it is unset or not
    /// valid UTF-8.
    pub rustflags: Option<String>,
    /// The `CARGO_ENCODED_RUSTFLAGS` variable's value, or [`None`] when it is
    /// unset or not valid UTF-8.
    pub encoded_rustflags: Option<String>,
    /// The directory whose `config.toml` closes the chain, or [`None`] when
    /// neither `CARGO_HOME` nor the home directory resolves to a UTF-8 path.
    pub cargo_home: Option<camino::Utf8PathBuf>,
    /// The highest directory the ancestor walk toward `cargo_home` may visit,
    /// or [`None`] for the filesystem root.
    pub chain_ceiling: Option<camino::Utf8PathBuf>,
}

impl CargoEnvironment {
    /// Reads the cargo environment from the running process.
    ///
    /// `cargo_home` resolves to the `CARGO_HOME` variable when set, else to the
    /// home directory's `.cargo` subdirectory, else to [`None`] when neither
    /// path is valid UTF-8 or no home directory can be determined.
    /// `chain_ceiling` is always [`None`]: a process-wide resolution walks
    /// ancestors all the way to the filesystem root, the same as cargo itself.
    pub fn from_process() -> Self {
        Self {
            rustflags: std::env::var("RUSTFLAGS").ok(),
            encoded_rustflags: std::env::var("CARGO_ENCODED_RUSTFLAGS").ok(),
            cargo_home: cargo_home_from_process(),
            chain_ceiling: None,
        }
    }

    /// Returns an environment carrying no ambient variable, bounded to
    /// `ceiling`.
    ///
    /// An unbounded ancestor walk started from a temporary test directory would
    /// keep climbing past it and read whatever `config.toml` files happen to
    /// exist above it on the host running the test, files whose content the
    /// test does not control and the test's outcome must not depend on.
    /// Sealing the environment at `ceiling` keeps the walk inside the directory
    /// under analysis.
    pub fn sealed(ceiling: &camino::Utf8Path) -> Self {
        Self {
            rustflags: None,
            encoded_rustflags: None,
            cargo_home: None,
            chain_ceiling: Some(ceiling.to_owned()),
        }
    }
}

/// Resolves `CARGO_HOME`, falling back to the home directory's `.cargo`
/// subdirectory.
///
/// Returns [`None`] when neither variable is set to a UTF-8 path and no home
/// directory can be determined, because a directory this crate cannot name is
/// a directory it cannot report on.
fn cargo_home_from_process() -> Option<camino::Utf8PathBuf> {
    if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
        return camino::Utf8PathBuf::from_path_buf(std::path::PathBuf::from(cargo_home)).ok();
    }

    let home = std::env::home_dir()?;

    camino::Utf8PathBuf::from_path_buf(home.join(".cargo")).ok()
}

#[cfg(test)]
mod tests {
    use super::CargoEnvironment;
    use camino::Utf8Path;

    #[test]
    fn sealed_environment_carries_a_ceiling_and_no_variables() {
        let sealed = CargoEnvironment::sealed(Utf8Path::new("/probe"));

        assert_eq!(
            CargoEnvironment::sealed(Utf8Path::new("/probe"))
                .chain_ceiling
                .as_deref(),
            Some(Utf8Path::new("/probe"))
        );
        assert_eq!(sealed.rustflags, None);
        assert_eq!(sealed.encoded_rustflags, None);
        assert_eq!(sealed.cargo_home, None);
    }

    #[test]
    fn process_environment_carries_no_ceiling() {
        assert_eq!(CargoEnvironment::from_process().chain_ceiling, None);
    }
}
