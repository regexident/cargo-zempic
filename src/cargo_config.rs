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
//!
//! The two file-backed ranks each include the environment variable cargo maps
//! onto their configuration key: `CARGO_TARGET_<TRIPLE>_RUSTFLAGS` for the
//! target rank and `CARGO_BUILD_RUSTFLAGS` for the build rank. Cargo appends
//! such a variable's flags to the flags of the rank's file entries instead of
//! letting it shadow them, so these variables are entries of their rank rather
//! than ranks of their own.

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
    /// The `CARGO_BUILD_RUSTFLAGS` variable's value, or [`None`] when it is
    /// unset or not valid UTF-8.
    pub build_rustflags: Option<String>,
    /// Every `CARGO_TARGET_*_RUSTFLAGS` variable whose name and value are
    /// valid UTF-8, keyed by the full variable name.
    pub target_rustflags: std::collections::BTreeMap<String, String>,
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
            build_rustflags: std::env::var("CARGO_BUILD_RUSTFLAGS").ok(),
            target_rustflags: target_rustflags_from_process(),
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
            build_rustflags: None,
            target_rustflags: std::collections::BTreeMap::new(),
            cargo_home: None,
            chain_ceiling: Some(ceiling.to_owned()),
        }
    }
}

/// Collects every `CARGO_TARGET_*_RUSTFLAGS` variable of the process.
///
/// A variable whose name or value is not valid UTF-8 is skipped, because a
/// name this crate cannot spell cannot be matched against a target triple.
fn target_rustflags_from_process() -> std::collections::BTreeMap<String, String> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .filter(|(name, _)| name.starts_with("CARGO_TARGET_") && name.ends_with("_RUSTFLAGS"))
        .collect()
}

/// The name of the environment variable cargo maps onto
/// `target.<target_triple>.rustflags`.
///
/// Cargo spells a configuration key's variable by uppercasing the key and
/// replacing every `-` and `.` with `_`, so `x86_64-unknown-linux-gnu` becomes
/// `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS`.
///
/// # Examples
///
/// ```
/// use cargo_zempic::cargo_config::target_rustflags_variable;
///
/// assert_eq!(
///     target_rustflags_variable("thumbv7em-none-eabihf"),
///     "CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUSTFLAGS",
/// );
/// ```
pub fn target_rustflags_variable(target_triple: &str) -> String {
    let spelled: String = target_triple
        .chars()
        .map(|character| match character {
            '-' | '.' => '_',
            other => other.to_ascii_uppercase(),
        })
        .collect();

    format!("CARGO_TARGET_{spelled}_RUSTFLAGS")
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

/// An environment variable cargo consults for rustflags ahead of any
/// configuration file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentVariable {
    /// `RUSTFLAGS`, whitespace-separated.
    Rustflags,
    /// `CARGO_ENCODED_RUSTFLAGS`, separated by the unit separator `\x1f`.
    EncodedRustflags,
}

impl EnvironmentVariable {
    /// The variable's name as cargo and the shell know it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rustflags => "RUSTFLAGS",
            Self::EncodedRustflags => "CARGO_ENCODED_RUSTFLAGS",
        }
    }
}

/// Which of cargo's three rustflags ranks supplied the flags cargo actually
/// applies, or that none of them did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustflagsSource {
    /// Rank one: a `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` environment
    /// variable, present even when its value is empty.
    Environment {
        /// Which of the two variables supplied the value.
        variable: EnvironmentVariable,
    },
    /// Rank two: the matching `target.<triple>.rustflags` array, concatenated
    /// with every `target.'cfg(...)'.rustflags` array found in the chain,
    /// regardless of whether the predicate inside `cfg(...)` would actually
    /// match the host cargo is building for.
    ///
    /// The rank includes the `CARGO_TARGET_<TRIPLE>_RUSTFLAGS` variable,
    /// which cargo appends to the rank's file entries.
    TargetTables,
    /// Rank three: the `build.rustflags` array.
    ///
    /// The rank includes the `CARGO_BUILD_RUSTFLAGS` variable, which cargo
    /// appends to the rank's file entries.
    BuildTable,
    /// No rank in the chain set any rustflags at all.
    Unset,
}

/// Where a `rustflags` array was declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RustflagsOrigin {
    /// A key in a chain file, carrying the span of the key that declares the
    /// array.
    File(crate::manifest::KeySpan),
    /// An environment variable cargo maps onto a configuration key.
    Environment {
        /// The variable's name.
        variable: String,
    },
}

/// One `rustflags` array, together with where it was declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustflagsEntry {
    /// Where this array was declared.
    pub origin: RustflagsOrigin,
    /// The array's string elements, in source order.
    pub flags: Vec<String>,
}

impl RustflagsEntry {
    /// The dotted configuration key declaring this array for a
    /// [`RustflagsOrigin::File`], or the variable name for a
    /// [`RustflagsOrigin::Environment`].
    pub fn name(&self) -> &str {
        match &self.origin {
            RustflagsOrigin::File(span) => span.key.as_str(),
            RustflagsOrigin::Environment { variable } => variable.as_str(),
        }
    }

    /// The span of the key declaring this array, or [`None`] for an
    /// environment variable, which has no position in any file.
    pub fn key_span(&self) -> Option<&crate::manifest::KeySpan> {
        match &self.origin {
            RustflagsOrigin::File(span) => Some(span),
            RustflagsOrigin::Environment { .. } => None,
        }
    }
}

/// The rustflags cargo applies after resolving its three-rank precedence over
/// a configuration chain, together with every entry that rank shadowed.
///
/// Only flag membership in [`flags`](Self::flags) is relied upon anywhere
/// downstream; the concatenation order of [`live`](Self::live)'s entries is a
/// readability choice, not a contract any caller may depend on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedRustflags {
    /// Which rank supplied [`flags`](Self::flags).
    pub source: RustflagsSource,
    /// The rustflags cargo actually applies, as the concatenation of every
    /// array it reads.
    ///
    /// Adjacent arrays share no token pairing in cargo, yet a flat scan of
    /// this concatenation would pair a trailing `-C` with the first token of
    /// the next array. Readers must therefore use
    /// [`flag_enabled`](Self::flag_enabled),
    /// [`unstable_flag`](Self::unstable_flag) and
    /// [`link_arguments`](Self::link_arguments), which scan each array on its
    /// own, rather than scanning this field.
    pub flags: Vec<String>,
    /// The entries contributing to [`flags`](Self::flags); empty when
    /// [`source`](Self::source) is [`RustflagsSource::Environment`] or
    /// [`RustflagsSource::Unset`], since neither reads a chain file.
    pub live: Vec<RustflagsEntry>,
    /// Every present entry of a rank strictly below the live source, in
    /// chain order.
    pub shadowed: Vec<RustflagsEntry>,
    /// Whether a `target.'cfg(...)'.rustflags` table was found while
    /// collecting rank two, regardless of which rank ultimately won.
    ///
    /// This crate has no cfg evaluator, so such a table counts as live
    /// whether or not its predicate would actually match; a rule relying on
    /// a resolution carrying this flag must cap its confidence accordingly.
    pub cfg_tables_unevaluated: bool,
}

impl ResolvedRustflags {
    /// The arrays cargo reads independently: the single environment array for
    /// [`RustflagsSource::Environment`], each live entry's array in order for
    /// the two chain-file ranks, and none for [`RustflagsSource::Unset`].
    fn flag_arrays(&self) -> Vec<&[String]> {
        match self.source {
            RustflagsSource::Environment { .. } => vec![self.flags.as_slice()],
            RustflagsSource::TargetTables | RustflagsSource::BuildTable => self
                .live
                .iter()
                .map(|entry| entry.flags.as_slice())
                .collect(),
            RustflagsSource::Unset => Vec::new(),
        }
    }

    /// Whether the rustc codegen flag `name` is enabled, reading each array on
    /// its own.
    ///
    /// The first array naming `name` decides; a `-C` ending one array never
    /// pairs with the first token of the next.
    pub fn flag_enabled(&self, name: &str) -> bool {
        self.flag_arrays()
            .into_iter()
            .find_map(|flags| codegen_flag_setting(flags, name))
            .unwrap_or(false)
    }

    /// The `-Z` flag named `name` as it last appears, reading each array on
    /// its own.
    ///
    /// The value comes from the last array that names `name` at all; see
    /// [`unstable_flag`] for the per-array rules.
    pub fn unstable_flag(&self, name: &str) -> Option<Option<String>> {
        self.flag_arrays()
            .into_iter()
            .filter_map(|flags| unstable_flag(flags, name))
            .next_back()
    }

    /// The values of every `-C link-arg`/`link-args` flag, reading each array
    /// on its own and concatenating the results in array order.
    pub fn link_arguments(&self) -> Vec<String> {
        self.flag_arrays()
            .into_iter()
            .flat_map(link_arguments)
            .collect()
    }
}

/// One `rustflags` array found while collecting a single rank from the
/// configuration chain.
struct CollectedEntry {
    entry: RustflagsEntry,
}

/// Read the `rustflags` array under `table`, if any, as a list of its string
/// elements.
fn rustflags_array(table: &toml_edit::Table, key: &str) -> Option<Vec<String>> {
    let item = table.get(key)?;
    let array = item.as_array()?;

    Some(
        array
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
    )
}

/// Collect the `build.rustflags` entry from `document`, if present.
fn collect_build_entry(document: &CargoConfigDocument) -> Option<CollectedEntry> {
    let build_table = document.document().as_table().get("build")?.as_table()?;
    let flags = rustflags_array(build_table, "rustflags")?;
    let key = document.key_span("build.rustflags")?;

    Some(CollectedEntry {
        entry: RustflagsEntry {
            origin: RustflagsOrigin::File(key),
            flags,
        },
    })
}

/// Collect every rank-two entry from `document`: the `target.<triple>`
/// table matching `target_triple` exactly, and every `target.'cfg(...)'`
/// table, in the order the `[target]` table lists them.
///
/// Returns the entries alongside whether any cfg table among them carried a
/// `rustflags` key, since that fact must be recorded even when the table
/// ultimately contributes nothing to the winning rank.
fn collect_target_entries(
    document: &CargoConfigDocument,
    target_triple: &str,
) -> (Vec<CollectedEntry>, bool) {
    let mut entries = Vec::new();
    let mut cfg_table_seen = false;

    let Some(target_table) = document
        .document()
        .as_table()
        .get("target")
        .and_then(|item| item.as_table())
    else {
        return (entries, cfg_table_seen);
    };

    for (key_name, item) in target_table.iter() {
        let Some(nested_table) = item.as_table() else {
            continue;
        };

        let is_matching_triple = key_name == target_triple;
        let is_cfg_table = key_name.starts_with("cfg(");

        if !is_matching_triple && !is_cfg_table {
            continue;
        }

        let Some(flags) = rustflags_array(nested_table, "rustflags") else {
            continue;
        };

        if is_cfg_table {
            cfg_table_seen = true;
        }

        let dotted_key = format!("target.'{key_name}'.rustflags");
        let Some(key) = document.key_span(&dotted_key) else {
            continue;
        };

        entries.push(CollectedEntry {
            entry: RustflagsEntry {
                origin: RustflagsOrigin::File(key),
                flags,
            },
        });
    }

    (entries, cfg_table_seen)
}

/// The entry for an environment variable cargo maps onto a rustflags key,
/// split on ASCII whitespace the way cargo splits such a list value.
fn environment_entry(variable: &str, value: &str) -> CollectedEntry {
    CollectedEntry {
        entry: RustflagsEntry {
            origin: RustflagsOrigin::Environment {
                variable: variable.to_owned(),
            },
            flags: value.split_ascii_whitespace().map(str::to_owned).collect(),
        },
    }
}

/// Resolve which rustflags source cargo uses from the `documents` chain, the
/// process `environment`, and the `target_triple` cargo is building for.
///
/// Evaluates cargo's precedence in strict rank order — the environment
/// variables first, then every matching `target` table in the chain, then
/// every `build.rustflags` table — stopping at the first rank that is
/// present anywhere in the chain. The target and build ranks each append
/// their mapped environment variable after the rank's file entries, and a
/// rank holding only such a variable is present. A rank below the one that wins is recorded
/// in [`ResolvedRustflags::shadowed`] purely so a rule can point at the
/// configuration cargo silently ignores; it never contributes to
/// [`ResolvedRustflags::flags`].
pub fn resolve(
    documents: &[CargoConfigDocument],
    environment: &CargoEnvironment,
    target_triple: &str,
) -> ResolvedRustflags {
    let mut target_entries = Vec::new();
    let mut cfg_tables_unevaluated = false;

    for document in documents {
        let (entries, cfg_table_seen) = collect_target_entries(document, target_triple);
        target_entries.extend(entries);
        cfg_tables_unevaluated |= cfg_table_seen;
    }

    let mut build_entries: Vec<CollectedEntry> =
        documents.iter().filter_map(collect_build_entry).collect();

    if let Some(value) = &environment.build_rustflags {
        build_entries.push(environment_entry("CARGO_BUILD_RUSTFLAGS", value));
    }

    if let Some(value) = environment
        .target_rustflags
        .get(&target_rustflags_variable(target_triple))
    {
        target_entries.push(environment_entry(
            &target_rustflags_variable(target_triple),
            value,
        ));
    }

    let shadowed_configuration = || {
        target_entries
            .iter()
            .chain(build_entries.iter())
            .map(|collected| collected.entry.clone())
            .collect::<Vec<_>>()
    };

    if let Some(value) = &environment.encoded_rustflags {
        return ResolvedRustflags {
            source: RustflagsSource::Environment {
                variable: EnvironmentVariable::EncodedRustflags,
            },
            flags: value
                .split('\u{1f}')
                .filter(|segment| !segment.is_empty())
                .map(str::to_owned)
                .collect(),
            live: Vec::new(),
            shadowed: shadowed_configuration(),
            cfg_tables_unevaluated,
        };
    }

    if let Some(value) = &environment.rustflags {
        return ResolvedRustflags {
            source: RustflagsSource::Environment {
                variable: EnvironmentVariable::Rustflags,
            },
            flags: value.split_ascii_whitespace().map(str::to_owned).collect(),
            live: Vec::new(),
            shadowed: shadowed_configuration(),
            cfg_tables_unevaluated,
        };
    }

    if !target_entries.is_empty() {
        let live: Vec<RustflagsEntry> = target_entries
            .into_iter()
            .map(|collected| collected.entry)
            .collect();
        let flags = live
            .iter()
            .flat_map(|entry| entry.flags.iter().cloned())
            .collect();
        let shadowed = build_entries
            .into_iter()
            .map(|collected| collected.entry)
            .collect();

        return ResolvedRustflags {
            source: RustflagsSource::TargetTables,
            flags,
            live,
            shadowed,
            cfg_tables_unevaluated,
        };
    }

    if !build_entries.is_empty() {
        let live: Vec<RustflagsEntry> = build_entries
            .into_iter()
            .map(|collected| collected.entry)
            .collect();
        let flags = live
            .iter()
            .flat_map(|entry| entry.flags.iter().cloned())
            .collect();

        return ResolvedRustflags {
            source: RustflagsSource::BuildTable,
            flags,
            live,
            shadowed: Vec::new(),
            cfg_tables_unevaluated,
        };
    }

    ResolvedRustflags {
        source: RustflagsSource::Unset,
        flags: Vec::new(),
        live: Vec::new(),
        shadowed: Vec::new(),
        cfg_tables_unevaluated,
    }
}

/// Discover the configuration chain above `package_directory` and resolve
/// which rustflags source cargo uses for `target_triple`.
///
/// # Errors
///
/// Returns whatever [`discover`] returns for a `config.toml` that exists but
/// cannot be read or parsed.
pub fn resolve_for_package(
    package_directory: &camino::Utf8Path,
    environment: &CargoEnvironment,
    target_triple: &str,
) -> crate::Result<ResolvedRustflags> {
    let documents = discover(package_directory, environment)?;

    Ok(resolve(&documents, environment, target_triple))
}

/// The name and optional `=value` suffix of one `-C`/`--codegen` flag,
/// split at the first `=`.
fn split_codegen_value(value: &str) -> (&str, Option<&str>) {
    match value.split_once('=') {
        Some((name, suffix)) => (name, Some(suffix)),
        None => (value, None),
    }
}

/// Resolve whether a `-C`/`--codegen` flag named `name` is enabled, given the
/// flag's own name and optional boolean suffix, or [`None`] when `value`
/// does not name `name` at all.
fn codegen_flag_state(value: &str, name: &str) -> Option<bool> {
    let (value_name, suffix) = split_codegen_value(value);

    if value_name != name {
        return None;
    }

    match suffix {
        None => Some(true),
        Some("yes" | "y" | "on" | "true") => Some(true),
        Some("no" | "n" | "off" | "false") => Some(false),
        Some(_) => None,
    }
}

/// The state of the first occurrence in `flags` that names the rustc codegen
/// flag `name`, or [`None`] when no occurrence names it.
///
/// Recognizes every form rustc and cargo accept: the two-element
/// `-C`/`--codegen` forms, their one-element `-C<name>`/`--codegen=<name>`
/// equivalents, and a `=yes`/`=y`/`=on`/`=true` or `=no`/`=n`/`=off`/`=false`
/// suffix on the name in any of them. A bare flag with no suffix at all
/// counts as enabled, matching cargo's own boolean-flag convention.
fn codegen_flag_setting(flags: &[String], name: &str) -> Option<bool> {
    let mut index = 0;

    while index < flags.len() {
        let current = flags[index].as_str();

        if current == "-C" || current == "--codegen" {
            if let Some(next) = flags.get(index + 1) {
                if let Some(state) = codegen_flag_state(next, name) {
                    return Some(state);
                }
            }

            index += 2;
            continue;
        }

        let matched = current
            .strip_prefix("-C")
            .or_else(|| current.strip_prefix("--codegen="))
            .and_then(|value| codegen_flag_state(value, name));

        if let Some(state) = matched {
            return Some(state);
        }

        index += 1;
    }

    None
}

/// Whether the rustc codegen flag `name` is enabled anywhere in `flags`.
///
/// `flags` must be a single `rustflags` array: a trailing `-C` pairs with no
/// element of any other array. See [`codegen_flag_setting`]'s recognition
/// rules, which this shares.
pub fn flag_enabled(flags: &[String], name: &str) -> bool {
    codegen_flag_setting(flags, name).unwrap_or(false)
}

/// The name and optional `=value` suffix of one `-Z` flag, split at the first
/// `=`.
fn split_unstable_value(value: &str) -> (&str, Option<&str>) {
    match value.split_once('=') {
        Some((name, suffix)) => (name, Some(suffix)),
        None => (value, None),
    }
}

/// The `-Z` flag named `name` as it last appears in `flags`.
///
/// Recognizes every form cargo accepts: the two-element `-Z name` and
/// `-Z name=value` spellings and the one-element `-Zname` and `-Zname=value`
/// ones. Returns [`Some`] with [`None`] for a bare flag and [`Some`] with the
/// value for a valued flag, or [`None`] when no occurrence names `name` at all.
///
/// Unstable flags accumulate across the array, so the last occurrence of a name
/// is the one the compiler acts on; an earlier occurrence does not survive.
/// A trailing `-Z` with no element after it is dropped by cargo's own
/// argument handling and is ignored here as well.
pub fn unstable_flag(flags: &[String], name: &str) -> Option<Option<String>> {
    let mut found = None;
    let mut index = 0;

    while index < flags.len() {
        let current = flags[index].as_str();

        if current == "-Z" {
            if let Some(next) = flags.get(index + 1) {
                let (value_name, suffix) = split_unstable_value(next);

                if value_name == name {
                    found = Some(suffix.map(str::to_owned));
                }
            }

            index += 2;
            continue;
        }

        if let Some(value) = current.strip_prefix("-Z") {
            let (value_name, suffix) = split_unstable_value(value);

            if value_name == name {
                found = Some(suffix.map(str::to_owned));
            }
        }

        index += 1;
    }

    found
}

/// Push the value of a `-C link-arg=<value>` or `-C link-args=<value>` flag
/// onto `results`, splitting a plural `link-args` value on ASCII whitespace
/// into multiple values while leaving a singular `link-arg` value whole.
fn push_link_argument(results: &mut Vec<String>, name: &str, value: &str) {
    match name {
        "link-arg" => results.push(value.to_owned()),
        "link-args" => results.extend(value.split_ascii_whitespace().map(str::to_owned)),
        _ => {}
    }
}

/// Collect the values of every `-C link-arg=<value>` and `-C
/// link-args=<value>` flag in `flags`, in every form rustc and cargo accept.
pub fn link_arguments(flags: &[String]) -> Vec<String> {
    let mut results = Vec::new();
    let mut index = 0;

    while index < flags.len() {
        let current = flags[index].as_str();

        if current == "-C" || current == "--codegen" {
            if let Some(next) = flags.get(index + 1) {
                if let Some((name, value)) = next.split_once('=') {
                    push_link_argument(&mut results, name, value);
                }
            }

            index += 2;
            continue;
        }

        let rest = current
            .strip_prefix("-C")
            .or_else(|| current.strip_prefix("--codegen="));

        if let Some(rest) = rest {
            if let Some((name, value)) = rest.split_once('=') {
                push_link_argument(&mut results, name, value);
            }
        }

        index += 1;
    }

    results
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
            ..CargoEnvironment::default()
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
            ..CargoEnvironment::default()
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
            ..CargoEnvironment::default()
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
            ..CargoEnvironment::default()
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

    #[test]
    fn build_table_alone_is_live() {
        use super::{CargoConfigDocument, RustflagsSource, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text = "[build]\nrustflags = [\"-C\", \"opt-level=z\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();
        let environment = CargoEnvironment::default();

        let resolved = resolve(&[document], &environment, "x86_64-unknown-linux-gnu");

        assert_eq!(resolved.source, RustflagsSource::BuildTable);
        assert_eq!(
            resolved.flags,
            vec!["-C".to_owned(), "opt-level=z".to_owned()]
        );
        assert_eq!(resolved.live.len(), 1);
        assert!(resolved.shadowed.is_empty());
        assert!(!resolved.cfg_tables_unevaluated);
    }

    #[test]
    fn target_table_alone_is_live() {
        use super::{CargoConfigDocument, RustflagsSource, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text =
            "[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-C\", \"target-cpu=native\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();
        let environment = CargoEnvironment::default();

        let resolved = resolve(&[document], &environment, "x86_64-unknown-linux-gnu");

        assert_eq!(resolved.source, RustflagsSource::TargetTables);
        assert_eq!(
            resolved.flags,
            vec!["-C".to_owned(), "target-cpu=native".to_owned()]
        );
    }

    #[test]
    fn environment_alone_is_live() {
        use super::{EnvironmentVariable, RustflagsSource, resolve};

        let environment = CargoEnvironment {
            rustflags: Some("-C a".to_owned()),
            encoded_rustflags: None,
            cargo_home: None,
            chain_ceiling: None,
            ..CargoEnvironment::default()
        };

        let resolved = resolve(&[], &environment, "x86_64-unknown-linux-gnu");

        assert_eq!(
            resolved.source,
            RustflagsSource::Environment {
                variable: EnvironmentVariable::Rustflags
            }
        );
        assert_eq!(resolved.flags, vec!["-C".to_owned(), "a".to_owned()]);
        assert!(resolved.shadowed.is_empty());
    }

    #[test]
    fn no_source_resolves_to_unset() {
        use super::{RustflagsSource, resolve};

        let environment = CargoEnvironment::default();

        let resolved = resolve(&[], &environment, "x86_64-unknown-linux-gnu");

        assert_eq!(resolved.source, RustflagsSource::Unset);
        assert!(resolved.flags.is_empty());
        assert!(resolved.live.is_empty());
        assert!(resolved.shadowed.is_empty());
        assert!(!resolved.cfg_tables_unevaluated);
    }

    #[test]
    fn target_table_shadows_the_build_table() {
        use super::{CargoConfigDocument, RustflagsSource, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text = "[build]\nrustflags = [\"-C\", \"a\"]\n\n[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-C\", \"b\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();
        let environment = CargoEnvironment::default();

        let resolved = resolve(
            &[CargoConfigDocument::parse(path, text).unwrap()],
            &environment,
            "x86_64-unknown-linux-gnu",
        );

        assert_eq!(resolved.source, RustflagsSource::TargetTables);
        assert_eq!(resolved.flags, vec!["-C".to_owned(), "b".to_owned()]);
        assert_eq!(resolved.shadowed.len(), 1);
        assert_eq!(resolved.shadowed[0].name(), "build.rustflags");

        let resolved_foreign_triple = resolve(&[document], &environment, "aarch64-apple-darwin");

        assert_eq!(resolved_foreign_triple.source, RustflagsSource::BuildTable);
        assert!(resolved_foreign_triple.shadowed.is_empty());
    }

    #[test]
    fn triple_and_cfg_tables_concatenate() {
        use super::{CargoConfigDocument, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text = "[build]\nrustflags = [\"-C\", \"a\"]\n\n[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-C\", \"b\"]\n\n[target.'cfg(windows)']\nrustflags = [\"-C\", \"c\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();
        let environment = CargoEnvironment::default();

        let resolved = resolve(&[document], &environment, "x86_64-unknown-linux-gnu");

        assert!(resolved.flags.contains(&"b".to_owned()));
        assert!(resolved.flags.contains(&"c".to_owned()));
        assert!(resolved.cfg_tables_unevaluated);
    }

    #[test]
    fn environment_shadows_both_configuration_ranks() {
        use super::{CargoConfigDocument, EnvironmentVariable, RustflagsSource, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text = "[build]\nrustflags = [\"-C\", \"a\"]\n\n[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-C\", \"b\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();
        let environment = CargoEnvironment {
            rustflags: Some("-C d  -C e".to_owned()),
            encoded_rustflags: None,
            cargo_home: None,
            chain_ceiling: None,
            ..CargoEnvironment::default()
        };

        let resolved = resolve(&[document], &environment, "x86_64-unknown-linux-gnu");

        assert_eq!(
            resolved.source,
            RustflagsSource::Environment {
                variable: EnvironmentVariable::Rustflags
            }
        );
        assert_eq!(
            resolved.flags,
            vec![
                "-C".to_owned(),
                "d".to_owned(),
                "-C".to_owned(),
                "e".to_owned()
            ]
        );
        assert_eq!(resolved.shadowed.len(), 2);
    }

    #[test]
    fn encoded_rustflags_wins_over_rustflags() {
        use super::{EnvironmentVariable, RustflagsSource, resolve};

        let environment = CargoEnvironment {
            rustflags: Some("-C a".to_owned()),
            encoded_rustflags: Some("-C\u{1f}b".to_owned()),
            cargo_home: None,
            chain_ceiling: None,
            ..CargoEnvironment::default()
        };

        let resolved = resolve(&[], &environment, "x86_64-unknown-linux-gnu");

        assert_eq!(
            resolved.source,
            RustflagsSource::Environment {
                variable: EnvironmentVariable::EncodedRustflags
            }
        );
    }

    #[test]
    fn a_foreign_triple_table_is_not_live() {
        use super::{CargoConfigDocument, RustflagsSource, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text = "[target.aarch64-apple-darwin]\nrustflags = [\"-C\", \"a\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();
        let environment = CargoEnvironment::default();

        let resolved = resolve(&[document], &environment, "x86_64-unknown-linux-gnu");

        assert_eq!(resolved.source, RustflagsSource::Unset);
        assert!(resolved.live.is_empty());
        assert!(resolved.shadowed.is_empty());
        assert!(resolved.flags.is_empty());
        assert!(!resolved.cfg_tables_unevaluated);
    }

    #[test]
    fn unstable_flag_reads_the_separate_bare_form() {
        use super::unstable_flag;

        assert_eq!(
            unstable_flag(
                &["-Z".to_owned(), "panic-immediate-abort".to_owned()],
                "panic-immediate-abort"
            ),
            Some(None)
        );
    }

    #[test]
    fn unstable_flag_reads_the_joined_bare_form() {
        use super::unstable_flag;

        assert_eq!(
            unstable_flag(
                &["-Zpanic-immediate-abort".to_owned()],
                "panic-immediate-abort"
            ),
            Some(None)
        );
    }

    #[test]
    fn unstable_flag_reads_the_separate_valued_form() {
        use super::unstable_flag;

        assert_eq!(
            unstable_flag(
                &["-Z".to_owned(), "location-detail=none".to_owned()],
                "location-detail"
            ),
            Some(Some("none".to_owned()))
        );
    }

    #[test]
    fn unstable_flag_reads_the_joined_valued_form() {
        use super::unstable_flag;

        assert_eq!(
            unstable_flag(&["-Zlocation-detail=none".to_owned()], "location-detail"),
            Some(Some("none".to_owned()))
        );
    }

    #[test]
    fn unstable_flag_returns_none_when_absent() {
        use super::unstable_flag;

        assert_eq!(unstable_flag(&[], "panic-immediate-abort"), None);
        assert_eq!(
            unstable_flag(
                &["-C".to_owned(), "panic-immediate-abort".to_owned()],
                "panic-immediate-abort"
            ),
            None
        );
        assert_eq!(
            unstable_flag(&["-Zpanic-immediate-abort".to_owned()], "location-detail"),
            None
        );
    }

    #[test]
    fn unstable_flag_keeps_the_last_occurrence() {
        use super::unstable_flag;

        assert_eq!(
            unstable_flag(
                &[
                    "-Zlocation-detail=none".to_owned(),
                    "-Z".to_owned(),
                    "location-detail=full".to_owned(),
                ],
                "location-detail"
            ),
            Some(Some("full".to_owned()))
        );
        assert_eq!(
            unstable_flag(
                &[
                    "-Z".to_owned(),
                    "location-detail=none".to_owned(),
                    "-Zlocation-detail".to_owned(),
                ],
                "location-detail"
            ),
            Some(None)
        );
    }

    #[test]
    fn unstable_flag_ignores_a_trailing_switch() {
        use super::unstable_flag;

        assert_eq!(unstable_flag(&["-Z".to_owned()], "location-detail"), None);
        assert_eq!(
            unstable_flag(
                &["-Zlocation-detail=none".to_owned(), "-Z".to_owned(),],
                "location-detail"
            ),
            Some(Some("none".to_owned()))
        );
    }

    #[test]
    fn flag_enabled_recognizes_every_codegen_form() {
        use super::flag_enabled;

        let name = "link-dead-code";

        assert!(flag_enabled(
            &["-C".to_owned(), "link-dead-code".to_owned()],
            name
        ));
        assert!(flag_enabled(&["-Clink-dead-code".to_owned()], name));
        assert!(flag_enabled(
            &["--codegen".to_owned(), "link-dead-code".to_owned()],
            name
        ));
        assert!(flag_enabled(
            &["-C".to_owned(), "link-dead-code=yes".to_owned()],
            name
        ));
        assert!(!flag_enabled(
            &["-C".to_owned(), "link-dead-code=no".to_owned()],
            name
        ));
        assert!(!flag_enabled(
            &["-C".to_owned(), "link-dead-codex".to_owned()],
            name
        ));
    }

    fn resolve_text(text: &str) -> super::ResolvedRustflags {
        use super::{CargoConfigDocument, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let document = CargoConfigDocument::parse(path, text).unwrap();

        resolve(
            &[document],
            &CargoEnvironment::default(),
            "x86_64-unknown-linux-gnu",
        )
    }

    #[test]
    fn resolved_readers_do_not_pair_flags_across_arrays() {
        let triple_first = resolve_text(
            "[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-C\"]\n\n[target.'cfg(unix)']\nrustflags = [\"link-dead-code\"]\n",
        );
        let cfg_first = resolve_text(
            "[target.'cfg(unix)']\nrustflags = [\"-C\"]\n\n[target.x86_64-unknown-linux-gnu]\nrustflags = [\"link-dead-code\"]\n",
        );
        let unstable = resolve_text(
            "[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-Z\"]\n\n[target.'cfg(unix)']\nrustflags = [\"location-detail=none\"]\n",
        );
        let link = resolve_text(
            "[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-C\"]\n\n[target.'cfg(unix)']\nrustflags = [\"link-arg=-Wl,--gc-sections\"]\n",
        );

        assert!(!triple_first.flag_enabled("link-dead-code"));
        assert!(!cfg_first.flag_enabled("link-dead-code"));
        assert_eq!(unstable.unstable_flag("location-detail"), None);
        assert!(link.link_arguments().is_empty());
    }

    #[test]
    fn resolved_readers_scan_every_array() {
        use super::{RustflagsSource, resolve};

        let resolved = resolve_text(
            "[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-C\", \"target-cpu=native\"]\n\n[target.'cfg(unix)']\nrustflags = [\"-C\", \"link-dead-code\", \"-Z\", \"location-detail=none\", \"-C\", \"link-arg=/OPT:REF\"]\n",
        );

        assert!(resolved.flag_enabled("link-dead-code"));
        assert_eq!(
            resolved.unstable_flag("location-detail"),
            Some(Some("none".to_owned()))
        );
        assert_eq!(resolved.link_arguments(), vec!["/OPT:REF".to_owned()]);

        let environment = CargoEnvironment {
            rustflags: Some("-C link-dead-code".to_owned()),
            ..CargoEnvironment::default()
        };
        let from_environment = resolve(&[], &environment, "x86_64-unknown-linux-gnu");

        assert!(matches!(
            from_environment.source,
            RustflagsSource::Environment { .. }
        ));
        assert!(from_environment.flag_enabled("link-dead-code"));
    }

    const TRIPLE: &str = "x86_64-unknown-linux-gnu";
    const TRIPLE_VARIABLE: &str = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS";

    fn flags(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn target_environment(value: &str) -> CargoEnvironment {
        CargoEnvironment {
            target_rustflags: [(TRIPLE_VARIABLE.to_owned(), value.to_owned())].into(),
            ..CargoEnvironment::default()
        }
    }

    #[test]
    fn target_rustflags_variable_follows_cargo_naming() {
        use super::target_rustflags_variable;

        assert_eq!(
            target_rustflags_variable("x86_64-unknown-linux-gnu"),
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS"
        );
        assert_eq!(
            target_rustflags_variable("thumbv7em-none-eabihf"),
            "CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUSTFLAGS"
        );
    }

    #[test]
    fn cargo_build_rustflags_joins_the_build_rank() {
        use super::{CargoConfigDocument, RustflagsSource, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text = "[build]\nrustflags = [\"-C\", \"a\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();
        let environment = CargoEnvironment {
            build_rustflags: Some("-C b".to_owned()),
            ..CargoEnvironment::default()
        };

        let resolved = resolve(&[document], &environment, TRIPLE);

        assert_eq!(resolved.source, RustflagsSource::BuildTable);
        assert_eq!(resolved.live.len(), 2);
        assert_eq!(resolved.live[1].name(), "CARGO_BUILD_RUSTFLAGS");
        assert_eq!(resolved.live[1].key_span(), None);
        assert_eq!(resolved.flags, flags(&["-C", "a", "-C", "b"]));
        assert!(resolved.shadowed.is_empty());
    }

    #[test]
    fn cargo_target_rustflags_joins_the_target_rank() {
        use super::{CargoConfigDocument, RustflagsSource, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text = "[target.x86_64-unknown-linux-gnu]\nrustflags = [\"-C\", \"a\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();

        let resolved = resolve(&[document], &target_environment("-C b"), TRIPLE);

        assert_eq!(resolved.source, RustflagsSource::TargetTables);
        assert_eq!(resolved.live.len(), 2);
        assert_eq!(resolved.live[1].name(), TRIPLE_VARIABLE);
        assert_eq!(resolved.flags, flags(&["-C", "a", "-C", "b"]));
    }

    #[test]
    fn cargo_target_rustflags_alone_shadows_the_build_table() {
        use super::{CargoConfigDocument, RustflagsSource, resolve};

        let path = Utf8Path::new("/probe/.cargo/config.toml");
        let text = "[build]\nrustflags = [\"-C\", \"a\"]\n";
        let document = CargoConfigDocument::parse(path, text).unwrap();

        let resolved = resolve(&[document], &target_environment("-C b"), TRIPLE);

        assert_eq!(resolved.source, RustflagsSource::TargetTables);
        assert_eq!(resolved.flags, flags(&["-C", "b"]));
        assert_eq!(resolved.shadowed.len(), 1);
        assert_eq!(resolved.shadowed[0].name(), "build.rustflags");
    }

    #[test]
    fn rustflags_shadows_environment_mapped_entries() {
        use super::{EnvironmentVariable, RustflagsSource, resolve};

        let environment = CargoEnvironment {
            rustflags: Some("-C c".to_owned()),
            build_rustflags: Some("-C a".to_owned()),
            ..target_environment("-C b")
        };

        let resolved = resolve(&[], &environment, TRIPLE);

        assert_eq!(
            resolved.source,
            RustflagsSource::Environment {
                variable: EnvironmentVariable::Rustflags
            }
        );

        let names: Vec<&str> = resolved.shadowed.iter().map(|entry| entry.name()).collect();

        assert_eq!(names, vec![TRIPLE_VARIABLE, "CARGO_BUILD_RUSTFLAGS"]);
    }
}
