//! Rules about cargo profile settings and workspace profile placement.
//!
//! Every compiled artifact is built under exactly one profile, and the profile's
//! settings decide its size and behavior. Cargo fills any setting a manifest
//! leaves unset from a built-in default, so a key absent from `Cargo.toml` still
//! has an effective value; a rule therefore distinguishes an explicitly set
//! value from a default rather than reading the manifest text alone.
//!
//! Cargo honors a `[profile.*]` table only in the workspace root manifest, so a
//! member's own table is inert. The rules that report a profile setting read the
//! root manifest and are gated on the selection producing at least one linked
//! artifact, because a profile does not govern an `rlib` or a procedural macro.
//! The one rule about a member's own table lives here too, since it is about the
//! same table cargo parses and has no other module to live in.

use crate::finding::{Change, Confidence, Effort, Finding, Location, RuleId, RuntimeCost};
use crate::manifest::KeySpan;
use crate::profile::{DebugInfo, Lto, OptLevel, Provenance, Resolved, SplitDebuginfo, Strip};
use crate::rules::{Rule, RuleContext, RuleMetadata};

/// Whether any selected package emits an artifact the profile governs.
///
/// §7.1: a profile governs a linked artifact but not an `rlib` or a procedural
/// macro. Every `profile/*` rule returns nothing when this is false.
fn profile_rules_apply(context: &RuleContext<'_>) -> bool {
    context
        .resolution
        .selected
        .iter()
        .any(|package| package.classification.produces_linked_artifact())
}

/// Reports symbols retained in the linked artifact.
pub struct StripRule;

/// The instance registered for [`StripRule`].
pub const STRIP_RULE: StripRule = StripRule;

impl Rule for StripRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let strip = &context.resolution.profile.strip;
        if strip.value != Strip::None {
            return Vec::new();
        }

        let metadata = registered("profile/strip");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: current_text(strip, render_strip),
                recommended: "\"symbols\"".to_owned(),
            }),
            location_of(strip),
            "Symbols are retained in the linked artifact",
            "The symbol table and the strings it references are kept in the linked \
             artifact even though they are not needed to run it."
                .to_owned(),
            "Stripping removes function names from backtraces and profiler output.",
        )]
    }
}

/// Reports debug information written into the linked artifact.
pub struct DebugInfoRule;

/// The instance registered for [`DebugInfoRule`].
pub const DEBUG_INFO_RULE: DebugInfoRule = DebugInfoRule;

impl Rule for DebugInfoRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let debug = &context.resolution.profile.debug;
        if !debug_info_enabled(debug.value) {
            return Vec::new();
        }

        let metadata = registered("profile/debug-info");

        // When stripping already removes symbols, the linked artifact changes
        // little, so the benefit is compile time and intermediate disk.
        let detail = if context.resolution.profile.strip.value == Strip::Symbols {
            "Debug information is produced and then dropped by stripping. Disabling \
             `debug` saves compile time and the intermediate disk it writes while \
             leaving the linked artifact largely unchanged."
                .to_owned()
        } else {
            "Debug information is written into the artifact. Disabling `debug` keeps \
             it out of the linked artifact."
                .to_owned()
        };

        vec![build(
            metadata,
            None,
            Some(Change {
                current: current_text(debug, render_debug_info),
                recommended: "false".to_owned(),
            }),
            location_of(debug),
            "Debug information is enabled",
            detail,
            "A binary built without debug information cannot be stepped through.",
        )]
    }
}

/// Whether a debug level emits debug information.
///
/// `0` and `"none"` are the only values that emit none; every other variant
/// writes at least line tables.
fn debug_info_enabled(level: DebugInfo) -> bool {
    !matches!(level, DebugInfo::None)
}

/// Reports in-binary debug information on an Apple target.
pub struct SplitDebuginfoRule;

/// The instance registered for [`SplitDebuginfoRule`].
pub const SPLIT_DEBUGINFO_RULE: SplitDebuginfoRule = SplitDebuginfoRule;

impl Rule for SplitDebuginfoRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let profile = &context.resolution.profile;
        if !crate::profile::is_apple_target(context.target_triple)
            || !debug_info_enabled(profile.debug.value)
            || profile.split_debuginfo.value != SplitDebuginfo::Off
        {
            return Vec::new();
        }

        let metadata = registered("profile/split-debuginfo");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: current_text(&profile.split_debuginfo, render_split_debuginfo),
                recommended: "\"packed\"".to_owned(),
            }),
            location_of(&profile.split_debuginfo),
            "Debug information is linked into the artifact",
            "On this target debug information is left in the linked artifact. \
             Splitting it into side files moves it out without discarding it."
                .to_owned(),
            "This recommendation is platform-specific and the split files must be \
             kept beside the binary for a debugger to find them.",
        )]
    }
}

/// Reports a profile that sets nothing explicitly.
pub struct MissingConfigRule;

/// The instance registered for [`MissingConfigRule`].
pub const MISSING_CONFIG_RULE: MissingConfigRule = MissingConfigRule;

impl Rule for MissingConfigRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        // Every field is enumerated explicitly rather than reflected over, so
        // adding a setting to `ResolvedProfile` without considering this rule
        // fails to compile instead of silently changing the condition. An
        // inherited setting counts as configured, because the parent declares
        // it; only a value no manifest sets anywhere is absent.
        let profile = &context.resolution.profile;
        let configured = [
            profile.opt_level.is_explicit() || is_inherited(&profile.opt_level),
            profile.lto.is_explicit() || is_inherited(&profile.lto),
            profile.strip.is_explicit() || is_inherited(&profile.strip),
            profile.panic.is_explicit() || is_inherited(&profile.panic),
            profile.debug.is_explicit() || is_inherited(&profile.debug),
            profile.codegen_units.is_explicit() || is_inherited(&profile.codegen_units),
            profile.incremental.is_explicit() || is_inherited(&profile.incremental),
            profile.debug_assertions.is_explicit() || is_inherited(&profile.debug_assertions),
            profile.overflow_checks.is_explicit() || is_inherited(&profile.overflow_checks),
            profile.split_debuginfo.is_explicit() || is_inherited(&profile.split_debuginfo),
        ];

        if configured.iter().any(|set| *set) {
            return Vec::new();
        }

        // The location is the profile table's own key when a table exists but
        // sets nothing this crate reads, and absent when there is no table.
        let location = profile_table_location(context, &profile.name);
        let metadata = registered("profile/missing-config");

        vec![build(
            metadata,
            None,
            None,
            location,
            "The profile sets nothing explicitly",
            "Every setting falls back to cargo's defaults, which favor compile time \
             over a small artifact. The other findings in this report name the \
             settings to set."
                .to_owned(),
            "There is nothing to lose by configuring the profile.",
        )]
    }
}

/// The key span of the analyzed profile's own table, when one exists.
fn profile_table_location(context: &RuleContext<'_>, name: &str) -> Option<KeySpan> {
    let tables = crate::profile::profile_tables(context.root_manifest).ok()?;

    crate::profile::profile_table_for(&tables, name)
        .and_then(|_| context.root_manifest.key_span(&format!("profile.{name}")))
}

/// Reports an optimization level that does not target size.
pub struct OptLevelRule;

/// The instance registered for [`OptLevelRule`].
pub const OPT_LEVEL_RULE: OptLevelRule = OptLevelRule;

impl Rule for OptLevelRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        // The rule answers only the size dimension; under any other dimension it
        // has no recommendation.
        if context.dimension != crate::dimension::Dimension::Size {
            return Vec::new();
        }

        let opt_level = &context.resolution.profile.opt_level;
        if is_size_level(opt_level.value) {
            return Vec::new();
        }

        let metadata = registered("profile/opt-level:size");

        let mut finding = build(
            metadata,
            None,
            Some(Change {
                current: current_text(opt_level, render_opt_level),
                recommended: "\"z\"".to_owned(),
            }),
            location_of(opt_level),
            "The optimization level does not target size",
            "The compiler optimizes for speed. Optimizing for size lets it drop \
             code that exists only to run faster."
                .to_owned(),
            "Runtime cost, pronounced with `\"z\"` on compute-heavy code.",
        );
        finding.runtime_cost = RuntimeCost::Notable;

        vec![finding]
    }
}

/// Reports partial link-time optimization.
pub struct LtoRule;

/// The instance registered for [`LtoRule`].
pub const LTO_RULE: LtoRule = LtoRule;

impl Rule for LtoRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let lto = &context.resolution.profile.lto;
        if lto.value == Lto::Fat {
            return Vec::new();
        }

        let metadata = registered("profile/lto");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: current_text(lto, render_lto),
                recommended: "\"fat\"".to_owned(),
            }),
            location_of(lto),
            "Link-time optimization is partial or disabled",
            "Optimizing across crate boundaries removes code a per-crate build must \
             keep."
                .to_owned(),
            "Link time rises substantially.",
        )]
    }
}

/// Reports more than one code generation unit.
pub struct CodegenUnitsRule;

/// The instance registered for [`CodegenUnitsRule`].
pub const CODEGEN_UNITS_RULE: CodegenUnitsRule = CodegenUnitsRule;

impl Rule for CodegenUnitsRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let codegen_units = &context.resolution.profile.codegen_units;
        if codegen_units.value <= 1 {
            return Vec::new();
        }

        let metadata = registered("profile/codegen-units");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: current_text(codegen_units, |value| value.to_string()),
                recommended: "1".to_owned(),
            }),
            location_of(codegen_units),
            "The crate is split across several code generation units",
            "A single code generation unit lets `profile/lto` optimize across the \
             whole crate; with several units, that optimization is limited."
                .to_owned(),
            "Compiles are slower; the setting is what unlocks link-time optimization.",
        )]
    }
}
/// Whether an optimization level targets size, `"s"` or `"z"`.
fn is_size_level(value: OptLevel) -> bool {
    matches!(value, OptLevel::Size | OptLevel::SizeNoVectorize)
}

/// Render an `opt-level` value the way a manifest spells it.
fn render_opt_level(value: OptLevel) -> String {
    match value {
        OptLevel::Zero => "0".to_owned(),
        OptLevel::One => "1".to_owned(),
        OptLevel::Two => "2".to_owned(),
        OptLevel::Three => "3".to_owned(),
        OptLevel::Size => "\"s\"".to_owned(),
        OptLevel::SizeNoVectorize => "\"z\"".to_owned(),
    }
}

/// Render an `lto` value the way a manifest spells it.
fn render_lto(value: Lto) -> String {
    match value {
        Lto::Off => "false".to_owned(),
        Lto::Thin => "\"thin\"".to_owned(),
        Lto::Fat => "\"fat\"".to_owned(),
    }
}

/// The registered metadata for `id`.
///
/// # Panics
///
/// Panics when `id` is not a registered rule, which is an authoring error the
/// registry's totality test catches.
fn registered(id: &str) -> &'static RuleMetadata {
    let rule = RuleId::parse(id).expect("the rule identifier is well-formed");

    crate::rules::metadata_for(&rule).expect("the rule is registered")
}

/// Build a finding from a registry entry's metadata and the rule-specific fields.
///
/// The constant fields — tier, magnitude, confidence, effort, runtime cost and
/// fixability — are read from the registry entry, so a rule cannot drift from
/// the entry that documents it. Severity is read from
/// [`crate::rules::severity_of`], the single assignment table.
fn build(
    metadata: &'static RuleMetadata,
    package: Option<String>,
    change: Option<Change>,
    location: Option<KeySpan>,
    title: &str,
    detail: String,
    tradeoff: &str,
) -> Finding {
    let severity = crate::rules::severity_of(&metadata.rule_id())
        .expect("every registered rule has a severity assignment");

    Finding {
        rule: metadata.rule_id(),
        package,
        tier: metadata.tier,
        severity,
        magnitude: metadata.magnitude,
        confidence: Confidence::High,
        effort: Effort::Trivial,
        runtime_cost: RuntimeCost::None,
        fixability: metadata.fixability,
        change,
        location: location.map(Location::new),
        title: title.to_owned(),
        detail,
        tradeoff: tradeoff.to_owned(),
        references: Vec::new(),
    }
}

/// The `change.current` text: the rendered value, or `unset` for a cargo default.
fn current_text<T: Copy>(setting: &Resolved<T>, render: fn(T) -> String) -> String {
    if setting.is_cargo_default() {
        "unset".to_owned()
    } else {
        render(setting.value)
    }
}

/// The location of a setting: its key span when a manifest set it, else [`None`].
fn location_of<T>(setting: &Resolved<T>) -> Option<KeySpan> {
    match &setting.provenance {
        Provenance::Explicit { key, .. } | Provenance::Inherited { key, .. } => Some(key.clone()),
        Provenance::CargoDefault => None,
    }
}

/// Whether a setting's value was inherited from another profile.
fn is_inherited<T>(setting: &Resolved<T>) -> bool {
    matches!(setting.provenance, Provenance::Inherited { .. })
}

/// Render a `strip` value the way a manifest spells it.
fn render_strip(value: Strip) -> String {
    match value {
        Strip::None => "\"none\"".to_owned(),
        Strip::DebugInfo => "\"debuginfo\"".to_owned(),
        Strip::Symbols => "\"symbols\"".to_owned(),
    }
}

/// Render a `debug` value the way a manifest spells it.
fn render_debug_info(value: DebugInfo) -> String {
    match value {
        DebugInfo::None => "false".to_owned(),
        DebugInfo::LineDirectivesOnly => "\"line-directives-only\"".to_owned(),
        DebugInfo::LineTablesOnly => "\"line-tables-only\"".to_owned(),
        DebugInfo::Limited => "\"limited\"".to_owned(),
        DebugInfo::Full => "true".to_owned(),
    }
}

/// Render a `split-debuginfo` value the way a manifest spells it.
fn render_split_debuginfo(value: SplitDebuginfo) -> String {
    format!("\"{}\"", value.as_str())
}
