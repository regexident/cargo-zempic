//! Cargo profile settings resolved to concrete values.
//!
//! Every compiled artifact is built under exactly one profile, and the settings
//! of that profile determine its size and runtime behavior. Cargo fills in any
//! setting a manifest leaves unset from a built-in default, so a key that is
//! absent from `Cargo.toml` still has an effective value.
//!
//! A [`ResolvedProfile`] records that effective value together with a
//! [`Provenance`] describing where the value came from. Provenance and value are
//! kept separate from any notion of a recommended value: whether a setting is
//! already well tuned is a per-rule judgement made later, so the resolved form
//! carries only what cargo itself would use.

/// The optimization level cargo passes to the compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptLevel {
    /// No optimizations (`0`).
    Zero,
    /// Basic optimizations (`1`).
    One,
    /// Some optimizations (`2`).
    Two,
    /// All optimizations (`3`).
    Three,
    /// Optimize for binary size (`"s"`).
    Size,
    /// Optimize for binary size without loop vectorization (`"z"`).
    SizeNoVectorize,
}

/// The link-time optimization mode cargo selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lto {
    /// LTO disabled; covers cargo's `false` and `"off"`.
    Off,
    /// Thin LTO across the local crate and its codegen units (`"thin"`).
    Thin,
    /// Fat LTO across the whole dependency graph; covers `true` and `"fat"`.
    Fat,
}

/// How much cargo strips from the emitted binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strip {
    /// Strip nothing; covers cargo's `"none"` and `false`.
    None,
    /// Strip debug information (`"debuginfo"`).
    DebugInfo,
    /// Strip symbols and debug information; covers `"symbols"` and `true`.
    Symbols,
}

/// The panic strategy the emitted binary uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanicStrategy {
    /// Unwind the stack on panic (`"unwind"`).
    Unwind,
    /// Abort the process on panic (`"abort"`).
    Abort,
}

/// The amount of debug information cargo requests from the compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugInfo {
    /// No debug information; covers cargo's `0`, `false` and `"none"`.
    None,
    /// Line directives only (`"line-directives-only"`).
    LineDirectivesOnly,
    /// Line tables only (`"line-tables-only"`).
    LineTablesOnly,
    /// Debug information without type or variable-level detail; covers `1` and `"limited"`.
    Limited,
    /// Full debug information; covers `2`, `true` and `"full"`.
    Full,
}

/// How much debug information cargo emits in a separate file.
///
/// The setting decides whether debug information is left inside the linked
/// artifact, which is what makes it relevant to artifact size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDebuginfo {
    /// Debug information stays in the artifact (`"off"`).
    Off,
    /// Debug information is emitted into a separate directory of files
    /// (`"packed"`).
    Packed,
    /// Debug information is emitted into a separate file per object
    /// (`"unpacked"`).
    Unpacked,
}

impl SplitDebuginfo {
    /// The name this variant is spelled under in a manifest.
    pub fn as_str(&self) -> &'static str {
        match self {
            SplitDebuginfo::Off => "off",
            SplitDebuginfo::Packed => "packed",
            SplitDebuginfo::Unpacked => "unpacked",
        }
    }

    /// The variant a manifest value names, or [`None`] for any other value.
    pub fn from_toml_value(value: &toml_edit::Value) -> Option<Self> {
        match value.as_str()? {
            "off" => Some(SplitDebuginfo::Off),
            "packed" => Some(SplitDebuginfo::Packed),
            "unpacked" => Some(SplitDebuginfo::Unpacked),
            _ => None,
        }
    }
}

/// Whether `triple` targets an Apple platform.
///
/// An Apple triple carries `apple` as its vendor field, the second of the
/// `-`-separated fields; comparing that field rather than searching the whole
/// string keeps a path or crate name containing `apple` from producing a false
/// positive.
pub fn is_apple_target(triple: &str) -> bool {
    triple.split('-').nth(1) == Some("apple")
}

/// Where a resolved setting's value came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// The value was written directly in this profile's manifest table.
    Explicit {
        /// Name of the profile whose table carries the key.
        profile: String,
        /// Location of the key that set the value.
        key: crate::manifest::KeySpan,
    },
    /// The value came from the profile named in `inherits`.
    Inherited {
        /// Name of the profile the value was inherited from.
        from: String,
        /// Location of the key in the inherited profile.
        key: crate::manifest::KeySpan,
    },
    /// No manifest set the value; cargo's built-in default applies.
    CargoDefault,
}

/// A setting value together with where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved<T> {
    /// The effective value cargo would use.
    pub value: T,
    /// Where the value came from.
    pub provenance: Provenance,
}

impl<T> Resolved<T> {
    /// Builds a resolved value with an explicit provenance.
    pub fn new(value: T, provenance: Provenance) -> Self {
        Self { value, provenance }
    }

    /// Builds a resolved value carrying cargo's built-in default.
    fn new_default(value: T) -> Self {
        Self {
            value,
            provenance: Provenance::CargoDefault,
        }
    }

    /// Returns true when a manifest explicitly set the value in this profile.
    pub fn is_explicit(&self) -> bool {
        matches!(self.provenance, Provenance::Explicit { .. })
    }

    /// Returns true when no manifest set the value and cargo's default applies.
    pub fn is_cargo_default(&self) -> bool {
        matches!(self.provenance, Provenance::CargoDefault)
    }
}

/// A profile with every setting resolved to an effective value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedProfile {
    /// Name of the profile as cargo understands it.
    pub name: String,
    /// Profile names from the most immediate parent to the root of the chain.
    pub inherits_chain: Vec<String>,
    /// Optimization level.
    pub opt_level: Resolved<OptLevel>,
    /// Link-time optimization mode.
    pub lto: Resolved<Lto>,
    /// Amount of stripped output.
    pub strip: Resolved<Strip>,
    /// Panic strategy.
    pub panic: Resolved<PanicStrategy>,
    /// Amount of debug information.
    pub debug: Resolved<DebugInfo>,
    /// Number of code generation units.
    pub codegen_units: Resolved<u32>,
    /// Whether incremental compilation is enabled.
    pub incremental: Resolved<bool>,
    /// Whether debug assertions are enabled.
    pub debug_assertions: Resolved<bool>,
    /// Whether integer overflow checks are enabled.
    pub overflow_checks: Resolved<bool>,
    /// Where debug information is emitted.
    ///
    /// This is the one setting whose cargo default depends on the target as
    /// well as the profile: `unpacked` for an Apple target under the `dev`
    /// profile and `off` everywhere else.
    pub split_debuginfo: Resolved<SplitDebuginfo>,
}

/// One of cargo's four built-in profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinProfile {
    /// Development profile, cargo's default for build commands.
    Dev,
    /// Optimized profile used for release artifacts.
    Release,
    /// Test profile, which inherits `dev`.
    Test,
    /// Benchmark profile, which inherits `release`.
    Bench,
}

impl BuiltinProfile {
    /// Returns the built-in profile with the given cargo name, if any.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "dev" => Some(Self::Dev),
            "release" => Some(Self::Release),
            "test" => Some(Self::Test),
            "bench" => Some(Self::Bench),
            _ => None,
        }
    }

    /// Returns the built-in profile's cargo name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Release => "release",
            Self::Test => "test",
            Self::Bench => "bench",
        }
    }
}

/// Returns the built-in defaults for a profile, with `name` set to its cargo name.
///
/// `test` inherits `dev` and `bench` inherits `release`, so those two return the
/// base profile's settings unchanged apart from the name. No manifest is
/// consulted, so every setting carries [`Provenance::CargoDefault`]. The
/// `split-debuginfo` default is the one setting that depends on both the
/// profile and `target_triple`.
pub fn builtin_defaults(profile: BuiltinProfile, target_triple: &str) -> ResolvedProfile {
    let (name, mut base) = match profile {
        BuiltinProfile::Dev => ("dev", dev_defaults()),
        BuiltinProfile::Release => ("release", release_defaults()),
        BuiltinProfile::Test => ("test", dev_defaults()),
        BuiltinProfile::Bench => ("bench", release_defaults()),
    };

    // Cargo leaves `split-debuginfo` unset by default except under `dev` on an
    // Apple target, where it defaults to `unpacked`. That is the only
    // target-dependent default in the model.
    base.split_debuginfo = if profile == BuiltinProfile::Dev && is_apple_target(target_triple) {
        Resolved::new_default(SplitDebuginfo::Unpacked)
    } else {
        Resolved::new_default(SplitDebuginfo::Off)
    };

    ResolvedProfile {
        name: name.to_owned(),
        ..base
    }
}

fn dev_defaults() -> ResolvedProfile {
    ResolvedProfile {
        name: String::new(),
        inherits_chain: Vec::new(),
        opt_level: Resolved::new_default(OptLevel::Zero),
        lto: Resolved::new_default(Lto::Off),
        strip: Resolved::new_default(Strip::None),
        panic: Resolved::new_default(PanicStrategy::Unwind),
        debug: Resolved::new_default(DebugInfo::Full),
        codegen_units: Resolved::new_default(256u32),
        incremental: Resolved::new_default(true),
        debug_assertions: Resolved::new_default(true),
        overflow_checks: Resolved::new_default(true),
        split_debuginfo: Resolved::new_default(SplitDebuginfo::Off),
    }
}

fn release_defaults() -> ResolvedProfile {
    ResolvedProfile {
        name: String::new(),
        inherits_chain: Vec::new(),
        opt_level: Resolved::new_default(OptLevel::Three),
        lto: Resolved::new_default(Lto::Off),
        strip: Resolved::new_default(Strip::None),
        panic: Resolved::new_default(PanicStrategy::Unwind),
        debug: Resolved::new_default(DebugInfo::None),
        codegen_units: Resolved::new_default(16u32),
        incremental: Resolved::new_default(false),
        debug_assertions: Resolved::new_default(false),
        overflow_checks: Resolved::new_default(false),
        split_debuginfo: Resolved::new_default(SplitDebuginfo::Off),
    }
}

/// The settings one `[profile.<name>.package.<key>]` override carries.
///
/// Cargo permits only a subset of profile settings in a per-package override,
/// and of those this crate reads only `opt-level`, which is the one the
/// `profile/dependency-opt-level` rule consults. The struct therefore carries
/// only that setting; a later reader must not mistake the omission for an
/// oversight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageOverride {
    /// Value of `opt-level`, when the override sets it.
    pub opt_level: Option<(OptLevel, crate::manifest::KeySpan)>,
}

/// The settings written in one `[profile.<name>]` table of a manifest.
///
/// Each setting is present only when the table sets it. The accompanying
/// [`crate::manifest::KeySpan`] locates the key, and the profile's `inherits`
/// value is captured verbatim without resolving which profile it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileTable {
    /// Profile name as written in the table header.
    pub name: String,
    /// Value of `inherits`, when the table sets it.
    pub inherits: Option<String>,
    /// Value of `opt-level`.
    pub opt_level: Option<(OptLevel, crate::manifest::KeySpan)>,
    /// Value of `lto`.
    pub lto: Option<(Lto, crate::manifest::KeySpan)>,
    /// Value of `strip`.
    pub strip: Option<(Strip, crate::manifest::KeySpan)>,
    /// Value of `panic`.
    pub panic: Option<(PanicStrategy, crate::manifest::KeySpan)>,
    /// Value of `debug`.
    pub debug: Option<(DebugInfo, crate::manifest::KeySpan)>,
    /// Value of `codegen-units`.
    pub codegen_units: Option<(u32, crate::manifest::KeySpan)>,
    /// Value of `incremental`.
    pub incremental: Option<(bool, crate::manifest::KeySpan)>,
    /// Value of `debug-assertions`.
    pub debug_assertions: Option<(bool, crate::manifest::KeySpan)>,
    /// Value of `overflow-checks`.
    pub overflow_checks: Option<(bool, crate::manifest::KeySpan)>,
    /// Value of `split-debuginfo`.
    pub split_debuginfo: Option<(SplitDebuginfo, crate::manifest::KeySpan)>,
    /// The profile's per-package overrides, keyed by package name, with the
    /// glob form keyed by the literal `*`.
    pub package_overrides: std::collections::BTreeMap<String, PackageOverride>,
}

impl ProfileTable {
    /// The glob per-package override, `[profile.<name>.package."*"]`, if any.
    ///
    /// The glob override applies to every dependency; a named override applies
    /// to one package and is reached through
    /// [`package_overrides`](Self::package_overrides).
    pub fn glob_package_override(&self) -> Option<&PackageOverride> {
        self.package_overrides.get("*")
    }
}

/// The table `name` resolves to, when the manifest declares one.
///
/// A built-in profile with no table of its own yields [`None`].
pub fn profile_table_for<'a>(
    tables: &'a std::collections::BTreeMap<String, ProfileTable>,
    name: &str,
) -> Option<&'a ProfileTable> {
    tables.get(name)
}

/// Reads every immediate `[profile.<name>]` table from `manifest`.
///
/// The result maps each profile name to the settings its table sets. A document
/// without a `[profile]` table yields an empty map. Keys outside the modelled
/// settings and `inherits` are ignored, because cargo accepts settings this
/// scope does not model.
///
/// # Errors
///
/// Returns [`crate::ZempicError::ProfileName`] when a profile name contains a
/// character outside `[A-Za-z0-9_-]`, and [`crate::ZempicError::ProfileValue`]
/// when a modelled setting holds a value cargo does not accept:
/// [`crate::ZempicError`].
pub fn profile_tables(
    manifest: &crate::manifest::ManifestDocument,
) -> crate::Result<std::collections::BTreeMap<String, ProfileTable>> {
    let mut tables = std::collections::BTreeMap::new();

    let Some(profile) = manifest
        .document()
        .as_table()
        .get("profile")
        .and_then(toml_edit::Item::as_table)
    else {
        return Ok(tables);
    };

    for (name, item) in profile.iter() {
        if !is_valid_profile_name(name) {
            return Err(crate::ZempicError::ProfileName {
                path: manifest.path().to_owned(),
                name: name.to_owned(),
            });
        }

        let table = item.as_table().expect("a profile sub-table is a table");
        let ctx = SettingContext {
            manifest,
            profile: name,
        };

        tables.insert(name.to_owned(), read_profile_table(name, table, &ctx)?);
    }

    Ok(tables)
}

/// Resolve `name` to a profile with every setting carrying an effective value
/// and the provenance of that value.
///
/// The `inherits` chain is walked from `name` toward the built-in it roots at,
/// and the settings are then folded over the chain so that each takes the value
/// from the nearest profile that sets it. A setting no profile in the chain sets
/// falls back to the root built-in's [`builtin_defaults`].
///
/// # Errors
///
/// Returns [`crate::ZempicError::UnknownProfile`] when `name` is neither a
/// built-in nor declared, [`crate::ZempicError::CustomProfileMissingInherits`]
/// when a declared non-built-in profile omits `inherits`,
/// [`crate::ZempicError::UnknownInheritedProfile`] when an `inherits` target is
/// neither a built-in nor declared, and
/// [`crate::ZempicError::ProfileInheritsCycle`] when the chain revisits a
/// profile: [`crate::ZempicError`].
pub fn resolve_profile(
    manifest: &crate::manifest::ManifestDocument,
    name: &str,
    target_triple: &str,
) -> crate::Result<ResolvedProfile> {
    let tables = profile_tables(manifest)?;
    let path = manifest.path();

    if BuiltinProfile::from_name(name).is_none() && !tables.contains_key(name) {
        return Err(crate::ZempicError::UnknownProfile {
            path: path.to_owned(),
            name: name.to_owned(),
        });
    }

    let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut traversal: Vec<String> = Vec::new();
    let mut chain: Vec<&ProfileTable> = Vec::new();
    let mut current = name.to_owned();

    let root = loop {
        if !visited.insert(current.clone()) {
            traversal.push(current);

            return Err(crate::ZempicError::ProfileInheritsCycle {
                path: path.to_owned(),
                chain: traversal,
            });
        }

        let table = tables.get(&current);
        if let Some(table) = table {
            chain.push(table);
        }

        let parent = match (BuiltinProfile::from_name(&current), table) {
            (_, Some(table)) if table.inherits.is_some() => table
                .inherits
                .clone()
                .expect("the table declares an inherits target"),
            (Some(builtin), _) => match builtin_parent(builtin) {
                Some(parent) => parent.as_str().to_owned(),
                None => {
                    traversal.push(current);

                    break builtin;
                }
            },
            (None, None) | (None, Some(_)) => {
                return Err(crate::ZempicError::CustomProfileMissingInherits {
                    path: path.to_owned(),
                    name: current,
                });
            }
        };

        if BuiltinProfile::from_name(&parent).is_none() && !tables.contains_key(&parent) {
            return Err(crate::ZempicError::UnknownInheritedProfile {
                path: path.to_owned(),
                profile: current,
                inherits: parent,
            });
        }

        traversal.push(current);
        current = parent;
    };

    let defaults = builtin_defaults(root, target_triple);
    let inherits_chain = traversal.into_iter().skip(1).collect();

    Ok(ResolvedProfile {
        name: name.to_owned(),
        inherits_chain,
        opt_level: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.opt_level.as_ref(),
            |defaults| &defaults.opt_level,
        ),
        lto: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.lto.as_ref(),
            |defaults| &defaults.lto,
        ),
        strip: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.strip.as_ref(),
            |defaults| &defaults.strip,
        ),
        panic: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.panic.as_ref(),
            |defaults| &defaults.panic,
        ),
        debug: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.debug.as_ref(),
            |defaults| &defaults.debug,
        ),
        codegen_units: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.codegen_units.as_ref(),
            |defaults| &defaults.codegen_units,
        ),
        incremental: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.incremental.as_ref(),
            |defaults| &defaults.incremental,
        ),
        debug_assertions: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.debug_assertions.as_ref(),
            |defaults| &defaults.debug_assertions,
        ),
        overflow_checks: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.overflow_checks.as_ref(),
            |defaults| &defaults.overflow_checks,
        ),
        split_debuginfo: resolve_setting(
            &chain,
            name,
            &defaults,
            |table| table.split_debuginfo.as_ref(),
            |defaults| &defaults.split_debuginfo,
        ),
    })
}

/// The built-in a built-in profile tracks through its implicit chain.
///
/// Cargo models `test` as inheriting `dev` and `bench` as inheriting `release`,
/// so walking one of them steps to the tracked built-in. The other two, and any
/// built-in reached through the step, root the chain.
fn builtin_parent(profile: BuiltinProfile) -> Option<BuiltinProfile> {
    match profile {
        BuiltinProfile::Dev | BuiltinProfile::Release => None,
        BuiltinProfile::Test => Some(BuiltinProfile::Dev),
        BuiltinProfile::Bench => Some(BuiltinProfile::Release),
    }
}

/// Resolve one setting over the chain, taking the value from the nearest table
/// that sets it and cargo's default otherwise.
///
/// `chain` runs from the analyzed profile's table to the nearest ancestor with a
/// table, in that order, and `name` is the analyzed profile. A value read from
/// `name`'s own table is [`Provenance::Explicit`], one read from an ancestor's
/// table is [`Provenance::Inherited`], and [`Provenance::CargoDefault`] applies
/// when no table sets the setting, in which case the root built-in's default
/// value is taken from `defaults`.
fn resolve_setting<T: Clone>(
    chain: &[&ProfileTable],
    name: &str,
    defaults: &ResolvedProfile,
    setting: impl Fn(&ProfileTable) -> Option<&(T, crate::manifest::KeySpan)>,
    default: impl Fn(&ResolvedProfile) -> &Resolved<T>,
) -> Resolved<T> {
    for table in chain {
        if let Some((value, key)) = setting(table) {
            let provenance = if table.name == name {
                Provenance::Explicit {
                    profile: name.to_owned(),
                    key: key.clone(),
                }
            } else {
                Provenance::Inherited {
                    from: table.name.clone(),
                    key: key.clone(),
                }
            };

            return Resolved::new(value.clone(), provenance);
        }
    }

    Resolved::new_default(default(defaults).value.clone())
}

/// Reads the modelled settings of one profile table.
///
/// Only settings that justify a rule are read; anything else in the table is
/// left untouched so that cargo settings outside this scope pass unnoticed.
fn read_profile_table(
    name: &str,
    table: &toml_edit::Table,
    ctx: &SettingContext<'_>,
) -> crate::Result<ProfileTable> {
    let item = |setting| table.get(setting);

    Ok(ProfileTable {
        name: name.to_owned(),
        inherits: item("inherits")
            .and_then(toml_edit::Item::as_str)
            .map(str::to_owned),
        opt_level: read_setting(item("opt-level"), ctx, "opt-level", parse_opt_level)?,
        lto: read_setting(item("lto"), ctx, "lto", parse_lto)?,
        strip: read_setting(item("strip"), ctx, "strip", parse_strip)?,
        panic: read_setting(item("panic"), ctx, "panic", parse_panic)?,
        debug: read_setting(item("debug"), ctx, "debug", parse_debug)?,
        codegen_units: read_setting(item("codegen-units"), ctx, "codegen-units", parse_integer)?,
        incremental: read_setting(item("incremental"), ctx, "incremental", parse_bool)?,
        debug_assertions: read_setting(
            item("debug-assertions"),
            ctx,
            "debug-assertions",
            parse_bool,
        )?,
        overflow_checks: read_setting(item("overflow-checks"), ctx, "overflow-checks", parse_bool)?,
        split_debuginfo: read_setting(
            item("split-debuginfo"),
            ctx,
            "split-debuginfo",
            parse_split_debuginfo,
        )?,
        package_overrides: read_package_overrides(table, ctx)?,
    })
}

/// Reads every `[profile.<name>.package.<key>]` override from `table`.
///
/// Each override is parsed into a [`PackageOverride`]. The glob form is keyed by
/// the literal `*` and a named form by its package name. An override carrying no
/// setting this crate reads parses with no `opt-level` rather than erroring, so
/// a valid manifest never fails the analysis; only an unparseable `opt-level`
/// value is an error, through the same path a top-level one takes.
fn read_package_overrides(
    table: &toml_edit::Table,
    ctx: &SettingContext<'_>,
) -> crate::Result<std::collections::BTreeMap<String, PackageOverride>> {
    let mut overrides = std::collections::BTreeMap::new();

    let Some(package) = table.get("package").and_then(toml_edit::Item::as_table) else {
        return Ok(overrides);
    };

    for (key, entry) in package.iter() {
        let override_table = entry.as_table().expect("a package override is a table");
        let setting = format!("package.{key}.opt-level");

        overrides.insert(
            key.to_owned(),
            PackageOverride {
                opt_level: read_setting(
                    override_table.get("opt-level"),
                    ctx,
                    &setting,
                    parse_opt_level,
                )?,
            },
        );
    }

    Ok(overrides)
}

/// Reports whether `name` is a profile name cargo accepts, that is, one whose
/// characters are all ASCII alphanumerics, `-` or `_`.
///
/// cargo restricts profile names to these characters, and the name becomes a
/// segment of the dotted keys used to look up spans, so a dot or quote inside a
/// name would make those keys ambiguous.
fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

/// Locates a setting within a manifest's profile table.
struct SettingContext<'a> {
    manifest: &'a crate::manifest::ManifestDocument,
    profile: &'a str,
}

impl SettingContext<'_> {
    /// The dotted key of `setting` within this profile, used both to locate the
    /// key's span and to name it in an error.
    fn dotted_key(&self, setting: &str) -> String {
        format!("profile.{}.{}", self.profile, setting)
    }

    /// The error raised when a setting holds a value cargo does not accept.
    fn invalid(&self, setting: &str, found: String) -> crate::ZempicError {
        crate::ZempicError::ProfileValue {
            path: self.manifest.path().to_owned(),
            key: self.dotted_key(setting),
            found,
        }
    }
}

/// Applies `parse` to an optional setting, attaching the setting's key span.
fn read_setting<T>(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
    parse: impl Fn(Option<&toml_edit::Item>, &SettingContext<'_>, &str) -> crate::Result<Option<T>>,
) -> crate::Result<Option<(T, crate::manifest::KeySpan)>> {
    match parse(item, ctx, setting)? {
        Some(value) => Ok(Some((
            value,
            ctx.manifest
                .key_span(&ctx.dotted_key(setting))
                .expect("a setting read from the document has a span"),
        ))),
        None => Ok(None),
    }
}

/// Parses `opt-level`, mapping its integer and string spellings.
fn parse_opt_level(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
) -> crate::Result<Option<OptLevel>> {
    let Some(item) = item else {
        return Ok(None);
    };

    let value = if let Some(integer) = item.as_integer() {
        match integer {
            0 => OptLevel::Zero,
            1 => OptLevel::One,
            2 => OptLevel::Two,
            3 => OptLevel::Three,
            _ => return Err(ctx.invalid(setting, render(item))),
        }
    } else if let Some(text) = item.as_str() {
        match text {
            "0" => OptLevel::Zero,
            "1" => OptLevel::One,
            "2" => OptLevel::Two,
            "3" => OptLevel::Three,
            "s" => OptLevel::Size,
            "z" => OptLevel::SizeNoVectorize,
            _ => return Err(ctx.invalid(setting, render(item))),
        }
    } else {
        return Err(ctx.invalid(setting, render(item)));
    };

    Ok(Some(value))
}

/// Parses `lto`, mapping `true` and `"fat"` to the same whole-graph mode.
fn parse_lto(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
) -> crate::Result<Option<Lto>> {
    let Some(item) = item else {
        return Ok(None);
    };

    let value = if let Some(flag) = item.as_bool() {
        if flag { Lto::Fat } else { Lto::Off }
    } else if let Some(text) = item.as_str() {
        match text {
            "off" => Lto::Off,
            "thin" => Lto::Thin,
            "fat" => Lto::Fat,
            _ => return Err(ctx.invalid(setting, render(item))),
        }
    } else {
        return Err(ctx.invalid(setting, render(item)));
    };

    Ok(Some(value))
}

/// Parses `strip`, mapping `true` to the same mode as `"symbols"`.
fn parse_strip(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
) -> crate::Result<Option<Strip>> {
    let Some(item) = item else {
        return Ok(None);
    };

    let value = if let Some(flag) = item.as_bool() {
        if flag { Strip::Symbols } else { Strip::None }
    } else if let Some(text) = item.as_str() {
        match text {
            "none" => Strip::None,
            "debuginfo" => Strip::DebugInfo,
            "symbols" => Strip::Symbols,
            _ => return Err(ctx.invalid(setting, render(item))),
        }
    } else {
        return Err(ctx.invalid(setting, render(item)));
    };

    Ok(Some(value))
}

/// Parses `panic`, whose only spellings are the two strategy names.
fn parse_panic(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
) -> crate::Result<Option<PanicStrategy>> {
    let Some(item) = item else {
        return Ok(None);
    };

    let value = match item.as_str() {
        Some("unwind") => PanicStrategy::Unwind,
        Some("abort") => PanicStrategy::Abort,
        _ => return Err(ctx.invalid(setting, render(item))),
    };

    Ok(Some(value))
}

/// Parses `debug`, mapping `1` and `"limited"` to the same detail level.
fn parse_debug(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
) -> crate::Result<Option<DebugInfo>> {
    let Some(item) = item else {
        return Ok(None);
    };

    let value = if let Some(flag) = item.as_bool() {
        if flag {
            DebugInfo::Full
        } else {
            DebugInfo::None
        }
    } else if let Some(integer) = item.as_integer() {
        match integer {
            0 => DebugInfo::None,
            1 => DebugInfo::Limited,
            2 => DebugInfo::Full,
            _ => return Err(ctx.invalid(setting, render(item))),
        }
    } else if let Some(text) = item.as_str() {
        match text {
            "none" => DebugInfo::None,
            "line-directives-only" => DebugInfo::LineDirectivesOnly,
            "line-tables-only" => DebugInfo::LineTablesOnly,
            "limited" => DebugInfo::Limited,
            "full" => DebugInfo::Full,
            _ => return Err(ctx.invalid(setting, render(item))),
        }
    } else {
        return Err(ctx.invalid(setting, render(item)));
    };

    Ok(Some(value))
}

/// Parses `split-debuginfo`, whose only spellings are the three mode names.
fn parse_split_debuginfo(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
) -> crate::Result<Option<SplitDebuginfo>> {
    let Some(item) = item else {
        return Ok(None);
    };

    match item.as_value().and_then(SplitDebuginfo::from_toml_value) {
        Some(value) => Ok(Some(value)),
        None => Err(ctx.invalid(setting, render(item))),
    }
}

/// Parses the count `codegen-units` holds.
fn parse_integer(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
) -> crate::Result<Option<u32>> {
    let Some(item) = item else {
        return Ok(None);
    };

    match item
        .as_integer()
        .and_then(|value| u32::try_from(value).ok())
    {
        Some(value) => Ok(Some(value)),
        None => Err(ctx.invalid(setting, render(item))),
    }
}

/// Parses a boolean-valued setting, which accepts only `true` and `false`.
fn parse_bool(
    item: Option<&toml_edit::Item>,
    ctx: &SettingContext<'_>,
    setting: &str,
) -> crate::Result<Option<bool>> {
    let Some(item) = item else {
        return Ok(None);
    };

    match item.as_bool() {
        Some(value) => Ok(Some(value)),
        None => Err(ctx.invalid(setting, render(item))),
    }
}

/// Renders a setting's value for an error message.
///
/// A string value is reported as its contents without quotes, since cargo
/// treats the string's payload as the value, while every other shape is
/// reported in its TOML form.
fn render(item: &toml_edit::Item) -> String {
    match item.as_str() {
        Some(text) => text.to_owned(),
        None => item.to_string().trim().to_owned(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A non-Apple target triple, for the defaults that do not depend on it.
    const LINUX: &str = "x86_64-unknown-linux-gnu";

    /// An Apple target triple, for the one default that does.
    const APPLE: &str = "aarch64-apple-darwin";

    #[test]
    fn release_defaults_match_cargo() {
        let expected = ResolvedProfile {
            name: "release".to_owned(),
            inherits_chain: Vec::new(),
            opt_level: Resolved::new(OptLevel::Three, Provenance::CargoDefault),
            lto: Resolved::new(Lto::Off, Provenance::CargoDefault),
            strip: Resolved::new(Strip::None, Provenance::CargoDefault),
            panic: Resolved::new(PanicStrategy::Unwind, Provenance::CargoDefault),
            debug: Resolved::new(DebugInfo::None, Provenance::CargoDefault),
            codegen_units: Resolved::new(16u32, Provenance::CargoDefault),
            incremental: Resolved::new(false, Provenance::CargoDefault),
            debug_assertions: Resolved::new(false, Provenance::CargoDefault),
            overflow_checks: Resolved::new(false, Provenance::CargoDefault),
            split_debuginfo: Resolved::new(SplitDebuginfo::Off, Provenance::CargoDefault),
        };

        assert_eq!(builtin_defaults(BuiltinProfile::Release, LINUX), expected);
    }

    #[test]
    fn dev_defaults_match_cargo() {
        let expected = ResolvedProfile {
            name: "dev".to_owned(),
            inherits_chain: Vec::new(),
            opt_level: Resolved::new(OptLevel::Zero, Provenance::CargoDefault),
            lto: Resolved::new(Lto::Off, Provenance::CargoDefault),
            strip: Resolved::new(Strip::None, Provenance::CargoDefault),
            panic: Resolved::new(PanicStrategy::Unwind, Provenance::CargoDefault),
            debug: Resolved::new(DebugInfo::Full, Provenance::CargoDefault),
            codegen_units: Resolved::new(256u32, Provenance::CargoDefault),
            incremental: Resolved::new(true, Provenance::CargoDefault),
            debug_assertions: Resolved::new(true, Provenance::CargoDefault),
            overflow_checks: Resolved::new(true, Provenance::CargoDefault),
            split_debuginfo: Resolved::new(SplitDebuginfo::Off, Provenance::CargoDefault),
        };

        assert_eq!(builtin_defaults(BuiltinProfile::Dev, LINUX), expected);
    }

    #[test]
    fn split_debuginfo_default_depends_on_the_target_and_the_profile() {
        let apple_dev = builtin_defaults(BuiltinProfile::Dev, APPLE);
        assert_eq!(apple_dev.split_debuginfo.value, SplitDebuginfo::Unpacked);
        assert_eq!(
            apple_dev.split_debuginfo.provenance,
            Provenance::CargoDefault
        );

        let linux_dev = builtin_defaults(BuiltinProfile::Dev, LINUX);
        assert_eq!(linux_dev.split_debuginfo.value, SplitDebuginfo::Off);

        let apple_release = builtin_defaults(BuiltinProfile::Release, APPLE);
        assert_eq!(apple_release.split_debuginfo.value, SplitDebuginfo::Off);
    }

    #[test]
    fn apple_target_detection_reads_the_vendor_field() {
        assert!(is_apple_target("aarch64-apple-darwin"));
        assert!(is_apple_target("x86_64-apple-ios"));
        assert!(!is_apple_target(LINUX));
        // A triple whose vendor field is not `apple` is rejected even when the
        // string mentions it, so a path or crate name cannot misfire.
        assert!(!is_apple_target("x86_64-pc-apple-gnu"));
    }

    #[test]
    fn explicit_split_debuginfo_carries_its_provenance_and_span() {
        let document = manifest("[profile.release]\nsplit-debuginfo = \"packed\"\n");

        let tables = profile_tables(&document).unwrap();
        let (value, span) = tables["release"].split_debuginfo.as_ref().unwrap();
        assert_eq!(*value, SplitDebuginfo::Packed);
        assert_eq!(span.key, "profile.release.split-debuginfo");

        let resolved = resolve_profile(&document, "release", LINUX).unwrap();
        assert_eq!(resolved.split_debuginfo.value, SplitDebuginfo::Packed);
        assert!(matches!(
            resolved.split_debuginfo.provenance,
            Provenance::Explicit { ref profile, .. } if profile == "release"
        ));
    }

    #[test]
    fn glob_package_override_carries_its_opt_level() {
        let document = manifest("[profile.release.package.\"*\"]\nopt-level = \"z\"\n");

        let tables = profile_tables(&document).unwrap();
        let override_entry = tables["release"]
            .glob_package_override()
            .expect("the glob override is present");
        let (value, span) = override_entry.opt_level.as_ref().unwrap();

        assert_eq!(*value, OptLevel::SizeNoVectorize);
        assert_eq!(span.key, "profile.release.package.*.opt-level");
    }

    #[test]
    fn named_package_override_is_not_the_glob_override() {
        let document = manifest("[profile.release.package.serde]\nopt-level = 3\n");

        let tables = profile_tables(&document).unwrap();
        let table = &tables["release"];

        assert!(table.package_overrides.contains_key("serde"));
        assert!(
            table.package_overrides["serde"].opt_level.is_some(),
            "the named override carries its opt-level"
        );
        assert!(
            table.glob_package_override().is_none(),
            "a named override must not satisfy the glob accessor"
        );
    }

    #[test]
    fn package_override_without_opt_level_parses() {
        let document = manifest("[profile.release.package.\"*\"]\ndebug = true\n");

        let tables = profile_tables(&document).unwrap();
        let override_entry = tables["release"]
            .glob_package_override()
            .expect("the glob override is present");

        assert!(override_entry.opt_level.is_none());
    }

    #[test]
    fn test_profile_defaults_track_dev() {
        let mut expected = builtin_defaults(BuiltinProfile::Dev, LINUX);
        expected.name = "test".to_owned();

        assert_eq!(builtin_defaults(BuiltinProfile::Test, LINUX), expected);
    }

    #[test]
    fn bench_profile_defaults_track_release() {
        let mut expected = builtin_defaults(BuiltinProfile::Release, LINUX);
        expected.name = "bench".to_owned();

        assert_eq!(builtin_defaults(BuiltinProfile::Bench, LINUX), expected);
    }

    #[test]
    fn builtin_profile_names_round_trip() {
        let cases = [
            ("dev", BuiltinProfile::Dev),
            ("release", BuiltinProfile::Release),
            ("test", BuiltinProfile::Test),
            ("bench", BuiltinProfile::Bench),
        ];

        for (name, profile) in cases {
            assert_eq!(BuiltinProfile::from_name(name), Some(profile));
            assert_eq!(profile.as_str(), name);
        }

        assert_eq!(BuiltinProfile::from_name("custom"), None);
    }

    fn manifest(text: &str) -> crate::manifest::ManifestDocument {
        crate::manifest::ManifestDocument::parse(camino::Utf8Path::new("Cargo.toml"), text).unwrap()
    }

    #[test]
    fn profile_tables_is_empty_without_a_profile_table() {
        let document = manifest("[package]\nname = \"demo\"\n");

        let tables = profile_tables(&document).unwrap();

        assert!(tables.is_empty());
    }

    #[test]
    fn lto_spellings_map_to_variants() {
        let document = manifest(
            "[profile.off]\nlto = false\n\
             [profile.on]\nlto = true\n\
             [profile.thin]\nlto = \"thin\"\n\
             [profile.fat]\nlto = \"fat\"\n",
        );

        let tables = profile_tables(&document).unwrap();

        assert_eq!(tables["off"].lto.as_ref().unwrap().0, Lto::Off);
        assert_eq!(tables["on"].lto.as_ref().unwrap().0, Lto::Fat);
        assert_eq!(tables["thin"].lto.as_ref().unwrap().0, Lto::Thin);
        assert_eq!(tables["fat"].lto.as_ref().unwrap().0, Lto::Fat);
    }

    #[test]
    fn opt_level_spellings_map_to_variants() {
        let document = manifest(
            "lto = false\n\
             [profile.three]\nopt-level = 3\n\
             [profile.three_str]\nopt-level = \"3\"\n\
             [profile.size]\nopt-level = \"s\"\n\
             [profile.zn]\nopt-level = \"z\"\n",
        );

        let tables = profile_tables(&document).unwrap();

        assert_eq!(
            tables["three"].opt_level.as_ref().unwrap().0,
            OptLevel::Three
        );
        assert_eq!(
            tables["three_str"].opt_level.as_ref().unwrap().0,
            OptLevel::Three
        );
        assert_eq!(tables["size"].opt_level.as_ref().unwrap().0, OptLevel::Size);
        assert_eq!(
            tables["zn"].opt_level.as_ref().unwrap().0,
            OptLevel::SizeNoVectorize
        );
    }

    #[test]
    fn strip_and_debug_spellings_map_to_variants() {
        let document = manifest(
            "[profile.a]\nstrip = true\ndebug = 1\n\
             [profile.b]\nstrip = false\ndebug = 0\n\
             [profile.c]\nstrip = \"debuginfo\"\ndebug = \"limited\"\n\
             [profile.d]\nstrip = \"symbols\"\ndebug = 2\n\
             [profile.e]\nstrip = \"none\"\ndebug = \"full\"\n\
             [profile.f]\ndebug = \"line-directives-only\"\n\
             [profile.g]\ndebug = \"line-tables-only\"\n\
             [profile.h]\ndebug = \"none\"\n",
        );

        let tables = profile_tables(&document).unwrap();

        assert_eq!(tables["a"].strip.as_ref().unwrap().0, Strip::Symbols);
        assert_eq!(tables["a"].debug.as_ref().unwrap().0, DebugInfo::Limited);
        assert_eq!(tables["b"].strip.as_ref().unwrap().0, Strip::None);
        assert_eq!(tables["b"].debug.as_ref().unwrap().0, DebugInfo::None);
        assert_eq!(tables["c"].strip.as_ref().unwrap().0, Strip::DebugInfo);
        assert_eq!(tables["c"].debug.as_ref().unwrap().0, DebugInfo::Limited);
        assert_eq!(tables["d"].strip.as_ref().unwrap().0, Strip::Symbols);
        assert_eq!(tables["d"].debug.as_ref().unwrap().0, DebugInfo::Full);
        assert_eq!(tables["e"].strip.as_ref().unwrap().0, Strip::None);
        assert_eq!(tables["e"].debug.as_ref().unwrap().0, DebugInfo::Full);
        assert_eq!(
            tables["f"].debug.as_ref().unwrap().0,
            DebugInfo::LineDirectivesOnly
        );
        assert_eq!(
            tables["g"].debug.as_ref().unwrap().0,
            DebugInfo::LineTablesOnly
        );
        assert_eq!(tables["h"].debug.as_ref().unwrap().0, DebugInfo::None);
    }

    #[test]
    fn settings_carry_their_key_spans() {
        let document = manifest("[profile.release]\nlto = true\npanic = \"abort\"\n");

        let tables = profile_tables(&document).unwrap();
        let table = &tables["release"];

        let lto_span = &table.lto.as_ref().unwrap().1;
        assert_eq!(lto_span.key, "profile.release.lto");
        assert_eq!(lto_span.line, 2);
        assert_eq!(lto_span.column, 1);
        assert_eq!(lto_span.file, camino::Utf8PathBuf::from("Cargo.toml"));

        let panic_span = &table.panic.as_ref().unwrap().1;
        assert_eq!(panic_span.key, "profile.release.panic");
        assert_eq!(panic_span.line, 3);
        assert_eq!(panic_span.column, 1);
    }

    #[test]
    fn inherits_is_captured_verbatim() {
        let document = manifest("[profile.custom]\ninherits = \"release\"\n");

        let tables = profile_tables(&document).unwrap();

        assert_eq!(tables["custom"].inherits.as_deref(), Some("release"));
    }

    #[test]
    fn unmodelled_keys_are_ignored() {
        let document = manifest("[profile.release]\nlto = true\nrpath = false\n");

        let tables = profile_tables(&document).unwrap();

        assert_eq!(tables["release"].lto.as_ref().unwrap().0, Lto::Fat);
    }

    #[test]
    fn malformed_value_is_an_error() {
        let document = manifest("[profile.release]\nlto = \"sometimes\"\n");

        let error = profile_tables(&document).unwrap_err();

        match error {
            crate::ZempicError::ProfileValue { path, key, found } => {
                assert_eq!(path, camino::Utf8PathBuf::from("Cargo.toml"));
                assert_eq!(key, "profile.release.lto");
                assert_eq!(found, "sometimes");
            }
            other => panic!("expected ProfileValue, got {other:?}"),
        }
    }

    #[test]
    fn illegal_profile_name_is_an_error() {
        let document = manifest("[profile.\"my.profile\"]\nlto = true\n");

        let error = profile_tables(&document).unwrap_err();

        match error {
            crate::ZempicError::ProfileName { path, name } => {
                assert_eq!(path, camino::Utf8PathBuf::from("Cargo.toml"));
                assert_eq!(name, "my.profile");
            }
            other => panic!("expected ProfileName, got {other:?}"),
        }
    }

    #[test]
    fn unset_keys_fall_back_to_cargo_defaults() {
        let document = manifest("[package]\nname = \"demo\"\n");

        let resolved = resolve_profile(&document, "release", LINUX).unwrap();

        assert_eq!(resolved, builtin_defaults(BuiltinProfile::Release, LINUX));
    }

    #[test]
    fn explicit_setting_is_marked_explicit() {
        let document = manifest("[profile.release]\nlto = true\n");

        let resolved = resolve_profile(&document, "release", LINUX).unwrap();

        assert_eq!(resolved.name, "release");
        assert!(resolved.inherits_chain.is_empty());
        assert_eq!(resolved.lto.value, Lto::Fat);
        assert!(matches!(
            resolved.lto.provenance,
            Provenance::Explicit { ref profile, .. } if profile == "release"
        ));
        assert!(resolved.strip.is_cargo_default());
    }

    #[test]
    fn single_inherits_marks_the_parent_as_source() {
        let document = manifest(
            "[profile.fast]\ninherits = \"release\"\nlto = true\n[profile.custom]\ninherits = \"fast\"\n",
        );

        let resolved = resolve_profile(&document, "custom", LINUX).unwrap();

        assert_eq!(resolved.name, "custom");
        assert_eq!(resolved.inherits_chain, vec!["fast", "release"]);
        assert_eq!(resolved.lto.value, Lto::Fat);
        assert!(matches!(
            resolved.lto.provenance,
            Provenance::Inherited { ref from, .. } if from == "fast"
        ));
        assert_eq!(resolved.opt_level.value, OptLevel::Three);
        assert!(resolved.opt_level.is_cargo_default());
    }

    #[test]
    fn two_level_chain_attributes_each_setting_to_its_nearest_source() {
        let document = manifest(
            "[profile.root]\ninherits = \"release\"\nstrip = true\n\
             [profile.middle]\ninherits = \"root\"\nlto = true\n\
             [profile.leaf]\ninherits = \"middle\"\npanic = \"abort\"\n",
        );

        let resolved = resolve_profile(&document, "leaf", LINUX).unwrap();

        assert_eq!(resolved.name, "leaf");
        assert_eq!(resolved.inherits_chain, vec!["middle", "root", "release"]);
        assert!(matches!(
            resolved.panic.provenance,
            Provenance::Explicit { ref profile, .. } if profile == "leaf"
        ));
        assert!(matches!(
            resolved.lto.provenance,
            Provenance::Inherited { ref from, .. } if from == "middle"
        ));
        assert!(matches!(
            resolved.strip.provenance,
            Provenance::Inherited { ref from, .. } if from == "root"
        ));
        assert_eq!(resolved.opt_level.value, OptLevel::Three);
        assert!(resolved.opt_level.is_cargo_default());
    }

    #[test]
    fn custom_profile_name_resolves_through_its_chain() {
        let document = manifest("[profile.tuned]\ninherits = \"release\"\nlto = \"thin\"\n");

        let resolved = resolve_profile(&document, "tuned", LINUX).unwrap();

        assert_eq!(resolved.name, "tuned");
        assert_eq!(resolved.inherits_chain, vec!["release"]);
        assert_eq!(resolved.lto.value, Lto::Thin);
        assert!(matches!(
            resolved.lto.provenance,
            Provenance::Explicit { ref profile, .. } if profile == "tuned"
        ));
        assert_eq!(resolved.opt_level.value, OptLevel::Three);
        assert!(resolved.opt_level.is_cargo_default());
    }

    #[test]
    fn inherits_cycle_is_an_error() {
        let document = manifest(
            "[profile.first]\ninherits = \"second\"\n[profile.second]\ninherits = \"first\"\n",
        );

        let error = resolve_profile(&document, "first", LINUX).unwrap_err();

        match error {
            crate::ZempicError::ProfileInheritsCycle { path, chain } => {
                assert_eq!(path, camino::Utf8PathBuf::from("Cargo.toml"));
                assert_eq!(chain, vec!["first", "second", "first"]);
            }
            other => panic!("expected ProfileInheritsCycle, got {other:?}"),
        }
    }

    #[test]
    fn unknown_requested_profile_is_an_error() {
        let document = manifest("[package]\nname = \"demo\"\n");

        let error = resolve_profile(&document, "mystery", LINUX).unwrap_err();

        match error {
            crate::ZempicError::UnknownProfile { path, name } => {
                assert_eq!(path, camino::Utf8PathBuf::from("Cargo.toml"));
                assert_eq!(name, "mystery");
            }
            other => panic!("expected UnknownProfile, got {other:?}"),
        }
    }

    #[test]
    fn unknown_inherits_target_is_an_error() {
        let document = manifest("[profile.custom]\ninherits = \"nowhere\"\n");

        let error = resolve_profile(&document, "custom", LINUX).unwrap_err();

        match error {
            crate::ZempicError::UnknownInheritedProfile {
                path,
                profile,
                inherits,
            } => {
                assert_eq!(path, camino::Utf8PathBuf::from("Cargo.toml"));
                assert_eq!(profile, "custom");
                assert_eq!(inherits, "nowhere");
            }
            other => panic!("expected UnknownInheritedProfile, got {other:?}"),
        }
    }

    #[test]
    fn custom_profile_without_inherits_is_an_error() {
        let document = manifest("[profile.shortcut]\nlto = true\n");

        let error = resolve_profile(&document, "shortcut", LINUX).unwrap_err();

        match error {
            crate::ZempicError::CustomProfileMissingInherits { path, name } => {
                assert_eq!(path, camino::Utf8PathBuf::from("Cargo.toml"));
                assert_eq!(name, "shortcut");
            }
            other => panic!("expected CustomProfileMissingInherits, got {other:?}"),
        }
    }

    #[test]
    fn inherits_may_name_a_builtin_without_a_table() {
        let document = manifest("[profile.tuned]\ninherits = \"release\"\nlto = \"thin\"\n");

        let resolved = resolve_profile(&document, "tuned", LINUX).unwrap();

        assert_eq!(resolved.inherits_chain, vec!["release"]);
        assert_eq!(resolved.opt_level.value, OptLevel::Three);
        assert_eq!(resolved.opt_level.provenance, Provenance::CargoDefault);
    }

    #[test]
    fn builtin_tracking_marks_the_tracked_table_as_inherited() {
        let document = manifest("[profile.dev]\nlto = \"thin\"\n");

        let resolved = resolve_profile(&document, "test", LINUX).unwrap();

        assert_eq!(resolved.inherits_chain, vec!["dev"]);
        assert_eq!(resolved.lto.value, Lto::Thin);
        assert!(matches!(
            resolved.lto.provenance,
            Provenance::Inherited { ref from, .. } if from == "dev"
        ));
    }

    #[test]
    fn builtin_bench_tracks_release_table_as_inherited() {
        let document = manifest("[profile.release]\nlto = true\n");

        let resolved = resolve_profile(&document, "bench", LINUX).unwrap();

        assert_eq!(resolved.inherits_chain, vec!["release"]);
        assert_eq!(resolved.lto.value, Lto::Fat);
        assert!(matches!(
            resolved.lto.provenance,
            Provenance::Inherited { ref from, .. } if from == "release"
        ));
    }

    #[test]
    fn builtin_table_may_declare_its_own_inherits() {
        let document = manifest("[profile.release]\ninherits = \"dev\"\nlto = \"thin\"\n");

        let resolved = resolve_profile(&document, "release", LINUX).unwrap();

        assert_eq!(resolved.inherits_chain, vec!["dev"]);
        assert_eq!(resolved.lto.value, Lto::Thin);
        assert!(matches!(
            resolved.lto.provenance,
            Provenance::Explicit { ref profile, .. } if profile == "release"
        ));
        assert_eq!(resolved.opt_level.value, OptLevel::Zero);
        assert_eq!(resolved.opt_level.provenance, Provenance::CargoDefault);
    }

    #[test]
    fn builtin_declaring_itself_as_inherits_is_a_cycle() {
        let document = manifest("[profile.dev]\ninherits = \"dev\"\n");

        let error = resolve_profile(&document, "dev", LINUX).unwrap_err();

        match error {
            crate::ZempicError::ProfileInheritsCycle { path, chain } => {
                assert_eq!(path, camino::Utf8PathBuf::from("Cargo.toml"));
                assert_eq!(chain, vec!["dev", "dev"]);
            }
            other => panic!("expected ProfileInheritsCycle, got {other:?}"),
        }
    }
}
