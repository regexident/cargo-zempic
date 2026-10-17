//! Reading cargo configuration files and resolving which rustflags source
//! cargo actually uses.
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

/// A cargo configuration file parsed for analysis, retaining the text its
/// spans refer to.
///
/// A key's recorded location in a [`toml_edit::Document`] is a byte range into
/// the source text, while findings report one-based line and column
/// positions. Converting between the two requires the original text, the same
/// requirement [`manifest::ManifestDocument`](crate::manifest::ManifestDocument)
/// carries for a `Cargo.toml`. The two types duplicate that conversion rather
/// than share it, to keep a cargo configuration file's read and parse errors
/// distinct from a manifest's.
#[derive(Debug)]
pub struct CargoConfigDocument {
    path: camino::Utf8PathBuf,
    text: String,
    document: toml_edit::Document<String>,
}

impl CargoConfigDocument {
    /// Read and parse the cargo configuration file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::CargoConfigRead`](crate::ZempicError::CargoConfigRead)
    /// if the file cannot be read and
    /// [`ZempicError::CargoConfigParse`](crate::ZempicError::CargoConfigParse)
    /// if its contents are not valid TOML.
    pub fn load(path: &camino::Utf8Path) -> crate::Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|source| {
            crate::ZempicError::CargoConfigRead {
                path: path.to_owned(),
                source,
            }
        })?;

        Self::parse(path, &text)
    }

    /// Parse `text` as the cargo configuration file at `path` without
    /// touching the filesystem.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::CargoConfigParse`](crate::ZempicError::CargoConfigParse)
    /// if `text` is not valid TOML.
    pub fn parse(path: &camino::Utf8Path, text: &str) -> crate::Result<Self> {
        let document = toml_edit::Document::<String>::parse(text.to_owned()).map_err(|source| {
            crate::ZempicError::CargoConfigParse {
                path: path.to_owned(),
                source,
            }
        })?;

        Ok(Self {
            path: path.to_owned(),
            text: text.to_owned(),
            document,
        })
    }

    /// The cargo configuration file's path.
    pub fn path(&self) -> &camino::Utf8Path {
        &self.path
    }

    /// The parsed TOML document, retaining the spans of the text it was
    /// parsed from.
    pub fn document(&self) -> &toml_edit::Document<String> {
        &self.document
    }

    /// Resolve `dotted_key`, such as `target.'cfg(windows)'.rustflags`, to its
    /// source position.
    ///
    /// A segment may be a single- or double-quoted TOML key, as cargo accepts
    /// for a platform-specific `[target.'cfg(...)']` table; a `.` inside an
    /// open quote is not a segment boundary. The surrounding quote characters
    /// are stripped before a segment is used as a table lookup key, since
    /// `toml_edit` records a quoted header's key without them. Returns `None`
    /// when any segment is absent or when the parsed document carries no span
    /// for the key.
    pub fn key_span(&self, dotted_key: &str) -> Option<crate::manifest::KeySpan> {
        let mut segments = split_dotted_key(dotted_key).into_iter().peekable();
        let mut table = self.document.as_table();
        let key = loop {
            let segment = segments.next()?;
            let (key, value) = table.get_key_value(segment.as_str())?;

            if segments.peek().is_none() {
                break key;
            }

            table = value.as_table()?;
        };

        let span = key.span()?;
        let (line, column) = self.position_at(span.start);

        Some(crate::manifest::KeySpan {
            file: self.path.clone(),
            key: dotted_key.to_owned(),
            line,
            column,
        })
    }

    /// Convert a byte offset in the original text to a one-based line and
    /// column.
    ///
    /// Duplicated here rather than shared with
    /// [`manifest::ManifestDocument`](crate::manifest::ManifestDocument), to
    /// keep this type's errors distinct from a manifest's.
    fn position_at(&self, offset: usize) -> (usize, usize) {
        let prefix = &self.text[..offset];
        let line = prefix.matches('\n').count() + 1;
        let column = match prefix.rfind('\n') {
            Some(newline) => prefix.len() - newline,
            None => prefix.len() + 1,
        };

        (line, column)
    }
}

/// Split a dotted key into its segments, honoring single- and double-quoted
/// segments that may themselves contain a literal `.`.
///
/// A surrounding matching quote pair is stripped from each segment, since a
/// quoted TOML key such as `'cfg(windows)'` is looked up in a parsed
/// [`toml_edit::Table`] by its unquoted content.
fn split_dotted_key(dotted_key: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut open_quote: Option<char> = None;

    for character in dotted_key.chars() {
        match open_quote {
            Some(quote) if character == quote => {
                open_quote = None;
                current.push(character);
            }
            Some(_) => {
                current.push(character);
            }
            None if character == '\'' || character == '"' => {
                open_quote = Some(character);
                current.push(character);
            }
            None if character == '.' => {
                segments.push(std::mem::take(&mut current));
            }
            None => {
                current.push(character);
            }
        }
    }

    segments.push(current);

    segments
        .into_iter()
        .map(|segment| strip_matching_quotes(&segment))
        .collect()
}

/// Strip a surrounding matching pair of single or double quotes from
/// `segment`, leaving it unchanged if it carries none.
fn strip_matching_quotes(segment: &str) -> String {
    let bytes = segment.as_bytes();

    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];

        if (first == b'\'' || first == b'"') && first == last {
            return segment[1..segment.len() - 1].to_owned();
        }
    }

    segment.to_owned()
}

/// Canonicalize `path`, returning it unchanged when canonicalization fails.
///
/// A symlinked directory can be written one way and reported another: on
/// macOS a temporary directory under `/var/folders/...` is itself a symlink
/// to `/private/var/folders/...`, so a path built from one of those forms and
/// a path built from the other name the same directory without being equal
/// as strings. Comparing canonical forms turns that spelling difference into
/// the identity it represents. Falling back to the path as written keeps a
/// directory that cannot be canonicalized — most commonly because it does
/// not exist — comparable rather than making the comparison itself fail.
fn canonicalized(path: &camino::Utf8Path) -> camino::Utf8PathBuf {
    std::fs::canonicalize(path)
        .ok()
        .and_then(|canonical| camino::Utf8PathBuf::from_path_buf(canonical).ok())
        .unwrap_or_else(|| path.to_owned())
}

/// Walk the cargo configuration chain above `package_directory`, loading
/// every `.cargo/config.toml` found between it and the chain's ceiling.
///
/// Cargo locates `build.rustflags` and its other configuration keys by
/// walking from the package directory up through its ancestors, reading
/// `<directory>/.cargo/config.toml` wherever it exists, and finally
/// consulting `$CARGO_HOME/config.toml`; a value set nearer the package
/// overrides the same value set further up the chain, so the returned
/// [`Vec`] is ordered nearest-first with the `$CARGO_HOME` file last
/// regardless of whether `$CARGO_HOME` also happens to be an ancestor
/// visited earlier in the walk. A directory missing `.cargo/config.toml` is
/// skipped, not an error; only a file that exists but cannot be read or
/// parsed propagates as an error.
///
/// When `environment.chain_ceiling` is [`Some`], the walk visits only
/// directories at or under it (compared by their canonical form, so a
/// symlinked path does not escape the bound) and stops the moment
/// `package_directory` itself lies outside it, yielding an empty chain
/// rather than continuing past the ceiling under any circumstance. When it
/// is [`None`], the walk continues to the filesystem root, cargo's own
/// production behavior.
///
/// # Errors
///
/// Returns whatever [`CargoConfigDocument::load`] returns for a
/// `config.toml` that exists but cannot be read or parsed.
pub fn discover(
    package_directory: &camino::Utf8Path,
    environment: &CargoEnvironment,
) -> crate::Result<Vec<CargoConfigDocument>> {
    let ceiling = environment.chain_ceiling.as_deref().map(canonicalized);

    let mut chain = Vec::new();
    let mut visited_configuration_files = Vec::new();
    let mut directory = Some(package_directory);

    while let Some(current_directory) = directory {
        let is_within_ceiling = match &ceiling {
            Some(ceiling) => canonicalized(current_directory).starts_with(ceiling),
            None => true,
        };

        if !is_within_ceiling {
            break;
        }

        let configuration_path = current_directory.join(".cargo").join("config.toml");

        if configuration_path.is_file() {
            let canonical_configuration_path = canonicalized(&configuration_path);

            if !visited_configuration_files.contains(&canonical_configuration_path) {
                chain.push(CargoConfigDocument::load(&configuration_path)?);
                visited_configuration_files.push(canonical_configuration_path);
            }
        }

        directory = current_directory.parent();
    }

    if let Some(cargo_home) = &environment.cargo_home {
        let configuration_path = cargo_home.join("config.toml");

        if configuration_path.is_file() {
            let canonical_configuration_path = canonicalized(&configuration_path);

            if !visited_configuration_files.contains(&canonical_configuration_path) {
                chain.push(CargoConfigDocument::load(&configuration_path)?);
            }
        }
    }

    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::CargoEnvironment;
    use camino::{Utf8Path, Utf8PathBuf};

    fn utf8_path(path: &std::path::Path) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(path.to_owned())
            .expect("temporary directory path must be valid UTF-8")
    }

    /// Canonicalize `path` for use in a test fixture, panicking if `path`
    /// does not exist or is not valid UTF-8.
    ///
    /// A `TempDir`'s reported path and its canonical form name the same
    /// directory under two different spellings (on macOS the former sits
    /// under `/var/folders/...`, a symlink to the latter's
    /// `/private/var/folders/...`). Building the ceiling of a
    /// [`CargoEnvironment`] from this canonical spelling while leaving the
    /// visited package directory in its raw, as-reported form guarantees the
    /// two are compared under mismatched spellings, so a regression that
    /// drops `discover`'s own canonicalization would wrongly conclude the
    /// directory lies outside the ceiling.
    fn canonical_path(path: &Utf8Path) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(
            std::fs::canonicalize(path.as_std_path())
                .expect("fixture directory must exist before it can be canonicalized"),
        )
        .expect("canonicalized fixture path must be valid UTF-8")
    }

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

    #[test]
    fn key_span_points_at_the_last_segment() {
        use super::CargoConfigDocument;

        let path = Utf8Path::new("/probe/config.toml");
        let text = "[build]\nrustflags = [\"-C\", \"target-cpu=native\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();

        let span = document.key_span("build.rustflags").unwrap();

        assert_eq!(span.file, path);
        assert_eq!(span.key, "build.rustflags");
        assert_eq!(span.line, 2);
        assert_eq!(span.column, 1);
    }

    #[test]
    fn key_span_resolves_a_quoted_cfg_segment() {
        use super::CargoConfigDocument;

        let path = Utf8Path::new("/probe/config.toml");
        let text = "[target.'cfg(windows)']\nrustflags = []\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();

        let span = document
            .key_span("target.'cfg(windows)'.rustflags")
            .unwrap();

        assert_eq!(span.line, 2);
    }

    #[test]
    fn key_span_is_absent_for_an_unknown_key() {
        use super::CargoConfigDocument;

        let path = Utf8Path::new("/probe/config.toml");
        let text = "[build]\nrustflags = []\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();

        assert_eq!(document.key_span("build.absent"), None);
    }

    #[test]
    fn malformed_text_names_the_cargo_configuration() {
        use super::CargoConfigDocument;

        let path = Utf8Path::new("/probe/config.toml");
        let error = CargoConfigDocument::parse(path, "[build").unwrap_err();

        assert!(matches!(error, crate::ZempicError::CargoConfigParse { .. }));

        let message = error.to_string();

        assert!(message.contains("cargo configuration"));
        assert!(message.contains("/probe/config.toml"));
    }

    #[test]
    fn discovery_orders_the_chain_nearest_first() {
        use super::discover;

        let temporary_directory = tempfile::TempDir::new().unwrap();
        let root = utf8_path(temporary_directory.path()).join("root");
        let nested = root.join("nested");
        let deeper = nested.join("deeper");

        std::fs::create_dir_all(root.join(".cargo")).unwrap();
        std::fs::write(root.join(".cargo").join("config.toml"), "[build]\n").unwrap();
        std::fs::create_dir_all(nested.join(".cargo")).unwrap();
        std::fs::write(nested.join(".cargo").join("config.toml"), "[build]\n").unwrap();
        std::fs::create_dir_all(&deeper).unwrap();

        // The ceiling is given in its canonical spelling while `deeper` (and
        // every directory `discover` walks up from it) stays in its raw,
        // as-reported `TempDir` spelling, so a regression that compares the
        // two without canonicalizing would wrongly conclude the directory
        // lies outside the ceiling.
        let environment = CargoEnvironment {
            rustflags: None,
            encoded_rustflags: None,
            cargo_home: None,
            chain_ceiling: Some(canonical_path(&root)),
        };

        let chain = discover(&deeper, &environment).unwrap();

        assert_eq!(chain.len(), 2);
        assert!(
            chain[0]
                .path()
                .as_str()
                .ends_with("nested/.cargo/config.toml")
        );
        assert!(
            !chain[1]
                .path()
                .as_str()
                .ends_with("nested/.cargo/config.toml")
        );
        assert!(chain[1].path().as_str().ends_with(".cargo/config.toml"));
    }

    #[test]
    fn discovery_stops_at_the_ceiling() {
        use super::discover;

        let temporary_directory = tempfile::TempDir::new().unwrap();
        let root = utf8_path(temporary_directory.path()).join("root");
        let nested = root.join("nested");
        let deeper = nested.join("deeper");

        std::fs::create_dir_all(root.join(".cargo")).unwrap();
        std::fs::write(root.join(".cargo").join("config.toml"), "[build]\n").unwrap();
        std::fs::create_dir_all(nested.join(".cargo")).unwrap();
        std::fs::write(nested.join(".cargo").join("config.toml"), "[build]\n").unwrap();
        std::fs::create_dir_all(&deeper).unwrap();

        // The ceiling and `deeper` are deliberately given different
        // spellings of the same directory tree (canonical versus raw), so a
        // regression that compares them without canonicalizing would wrongly
        // conclude `deeper` lies outside the ceiling from the first step of
        // the walk, rather than stopping exactly at `nested`.
        let environment = CargoEnvironment {
            rustflags: None,
            encoded_rustflags: None,
            cargo_home: None,
            chain_ceiling: Some(canonical_path(&nested)),
        };

        let chain = discover(&deeper, &environment).unwrap();

        assert_eq!(chain.len(), 1);
        assert!(
            chain[0]
                .path()
                .as_str()
                .ends_with("nested/.cargo/config.toml")
        );
    }

    #[test]
    fn discovery_appends_the_cargo_home_file_last() {
        use super::discover;

        let temporary_directory = tempfile::TempDir::new().unwrap();
        let root = utf8_path(temporary_directory.path()).join("root");
        let nested = root.join("nested");
        let deeper = nested.join("deeper");
        let cargo_home = utf8_path(temporary_directory.path()).join("cargo_home");

        std::fs::create_dir_all(root.join(".cargo")).unwrap();
        std::fs::write(root.join(".cargo").join("config.toml"), "[build]\n").unwrap();
        std::fs::create_dir_all(nested.join(".cargo")).unwrap();
        std::fs::write(nested.join(".cargo").join("config.toml"), "[build]\n").unwrap();
        std::fs::create_dir_all(&deeper).unwrap();
        std::fs::create_dir_all(&cargo_home).unwrap();
        std::fs::write(cargo_home.join("config.toml"), "[build]\n").unwrap();

        let environment = CargoEnvironment {
            rustflags: None,
            encoded_rustflags: None,
            cargo_home: Some(cargo_home.clone()),
            chain_ceiling: Some(root.clone()),
        };

        let chain = discover(&deeper, &environment).unwrap();

        assert_eq!(chain.len(), 3);
        assert_eq!(chain.last().unwrap().path(), cargo_home.join("config.toml"));
    }

    #[test]
    fn discovery_is_empty_outside_the_ceiling() {
        use super::discover;

        let temporary_directory = tempfile::TempDir::new().unwrap();
        let base = utf8_path(temporary_directory.path());

        let ceiling_directory = base.join("ceiling");
        let outside_directory = base.join("outside");
        let escape = ceiling_directory.join("escape");

        std::fs::create_dir_all(ceiling_directory.join(".cargo")).unwrap();
        std::fs::write(
            ceiling_directory.join(".cargo").join("config.toml"),
            "[build]\n",
        )
        .unwrap();
        std::fs::create_dir_all(&outside_directory).unwrap();

        // `escape` is a symlink planted inside the ceiling that points to a
        // directory outside of it, so a directory reached through it is
        // textually nested under the ceiling (its raw spelling starts with
        // the ceiling's raw spelling) while canonically living outside the
        // ceiling's tree entirely. A regression that walks the raw spelling
        // without canonicalizing would treat `package_directory` as inside
        // the ceiling and keep walking back up through `escape` to the
        // ceiling's own `.cargo/config.toml`, wrongly returning it; only
        // canonicalizing before comparing resolves the symlink and detects
        // that the walk has left the ceiling's tree.
        symlink_directory(&outside_directory, &escape);

        let package_directory = escape.join("inner");
        std::fs::create_dir_all(&package_directory).unwrap();

        let environment = CargoEnvironment {
            rustflags: None,
            encoded_rustflags: None,
            cargo_home: None,
            chain_ceiling: Some(ceiling_directory),
        };

        let chain = discover(&package_directory, &environment).unwrap();

        assert!(chain.is_empty());
    }

    #[cfg(unix)]
    fn symlink_directory(target: &Utf8Path, link: &Utf8Path) {
        std::os::unix::fs::symlink(target.as_std_path(), link.as_std_path())
            .expect("symlink fixture must be creatable");
    }

    #[cfg(windows)]
    fn symlink_directory(target: &Utf8Path, link: &Utf8Path) {
        std::os::windows::fs::symlink_dir(target.as_std_path(), link.as_std_path())
            .expect("symlink fixture must be creatable");
    }
}
