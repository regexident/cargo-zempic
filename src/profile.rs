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
/// consulted, so every setting carries [`Provenance::CargoDefault`].
pub fn builtin_defaults(profile: BuiltinProfile) -> ResolvedProfile {
    let (name, base) = match profile {
        BuiltinProfile::Dev => ("dev", dev_defaults()),
        BuiltinProfile::Release => ("release", release_defaults()),
        BuiltinProfile::Test => ("test", dev_defaults()),
        BuiltinProfile::Bench => ("bench", release_defaults()),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };

        assert_eq!(builtin_defaults(BuiltinProfile::Release), expected);
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
        };

        assert_eq!(builtin_defaults(BuiltinProfile::Dev), expected);
    }

    #[test]
    fn test_profile_defaults_track_dev() {
        let mut expected = builtin_defaults(BuiltinProfile::Dev);
        expected.name = "test".to_owned();

        assert_eq!(builtin_defaults(BuiltinProfile::Test), expected);
    }

    #[test]
    fn bench_profile_defaults_track_release() {
        let mut expected = builtin_defaults(BuiltinProfile::Release);
        expected.name = "bench".to_owned();

        assert_eq!(builtin_defaults(BuiltinProfile::Bench), expected);
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
}
