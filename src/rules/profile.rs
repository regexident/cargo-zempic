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
use crate::profile::{
    DebugInfo, Lto, OptLevel, PanicStrategy, Provenance, Resolved, SplitDebuginfo, Strip,
};
use crate::rules::{Rule, RuleContext, RuleMetadata};

/// Whether any selected package emits an artifact the profile governs.
///
/// A profile governs a linked artifact but not an `rlib` or a procedural
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

        let profile = &context.resolution.profile;

        if profile.configures_any_setting() {
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

/// Reports incremental compilation in a release-like profile.
pub struct IncrementalRule;

/// The instance registered for [`IncrementalRule`].
pub const INCREMENTAL_RULE: IncrementalRule = IncrementalRule;

impl Rule for IncrementalRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let profile = &context.resolution.profile;
        if !profile.incremental.value || !is_release_like(profile) {
            return Vec::new();
        }

        let metadata = registered("profile/incremental");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: "true".to_owned(),
                recommended: "false".to_owned(),
            }),
            location_of(&profile.incremental),
            "Incremental compilation is enabled in a release-like profile",
            "Incremental compilation trades runtime speed for rebuild speed, which \
             a shipped artifact does not need."
                .to_owned(),
            "Rebuilds are slower.",
        )]
    }
}

/// Reports unoptimized dependencies under a size-oriented profile.
pub struct DependencyOptLevelRule;

/// The instance registered for [`DependencyOptLevelRule`].
pub const DEPENDENCY_OPT_LEVEL_RULE: DependencyOptLevelRule = DependencyOptLevelRule;

impl Rule for DependencyOptLevelRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let opt_level = &context.resolution.profile.opt_level;

        // The condition is an explicitly or inherited non-size `opt-level` with
        // no glob override of it; that is exactly the state the remedy addresses.
        if opt_level.is_cargo_default() || is_size_level(opt_level.value) {
            return Vec::new();
        }

        let tables = crate::profile::profile_tables(context.root_manifest)
            .expect("the root manifest already parsed during profile resolution");
        let table = crate::profile::profile_table_for(&tables, &context.resolution.profile.name);

        let override_present = table
            .and_then(|table| table.glob_package_override())
            .is_some_and(|entry| entry.opt_level.is_some());

        if override_present {
            return Vec::new();
        }

        let metadata = registered("profile/dependency-opt-level");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: "unset".to_owned(),
                recommended: "\"z\"".to_owned(),
            }),
            location_of(opt_level),
            "Dependencies are not optimized separately",
            "The profile's optimization level applies to dependencies too. A glob \
             override optimizes them for size while leaving the workspace's members \
             on the profile's setting."
                .to_owned(),
            "The override applies to dependencies, not to workspace members.",
        )]
    }
}

/// Whether an optimization level targets size, `"s"` or `"z"`.
fn is_size_level(value: OptLevel) -> bool {
    matches!(value, OptLevel::Size | OptLevel::SizeNoVectorize)
}

/// Whether a profile roots at cargo's `release` built-in.
///
/// A profile is release-like when its name or any entry in its `inherits` chain
/// names the `release` built-in, which is where cargo sets the shipped-artifact
/// defaults.
fn is_release_like(profile: &crate::profile::ResolvedProfile) -> bool {
    std::iter::once(&profile.name)
        .chain(profile.inherits_chain.iter())
        .any(|name| {
            crate::profile::BuiltinProfile::from_name(name)
                == Some(crate::profile::BuiltinProfile::Release)
        })
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

/// Whether the panic setting is inert for the analyzed profile.
///
/// Cargo rejects `panic` in the `test` and `bench` profiles, so the profile's
/// name or any entry in its `inherits` chain reaching either makes the setting
/// inert. The rule and the notice that reports the skip share this predicate.
pub fn panic_strategy_is_inert(profile: &crate::profile::ResolvedProfile) -> bool {
    std::iter::once(&profile.name)
        .chain(profile.inherits_chain.iter())
        .any(|name| matches!(name.as_str(), "test" | "bench"))
}

/// Reports a profile that unwinds instead of aborting.
pub struct PanicAbortRule;

/// The instance registered for [`PanicAbortRule`].
pub const PANIC_ABORT_RULE: PanicAbortRule = PanicAbortRule;

impl Rule for PanicAbortRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let profile = &context.resolution.profile;
        if panic_strategy_is_inert(profile) || profile.panic.value == PanicStrategy::Abort {
            return Vec::new();
        }

        let metadata = registered("profile/panic-abort");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: current_text(&profile.panic, render_panic),
                recommended: "\"abort\"".to_owned(),
            }),
            location_of(&profile.panic),
            "The profile unwinds instead of aborting",
            "Unwinding carries the machinery that walks the stack and runs \
             destructors. Aborting on panic lets the compiler drop it."
                .to_owned(),
            "Aborting loses unwinding: a `catch_unwind` in the workspace or any \
             dependency will no longer recover from a panic. The setting applies \
             workspace-wide, because `panic` cannot be set per package.",
        )]
    }
}

/// Reports debug assertions enabled in a release-like profile.
pub struct DebugAssertionsRule;

/// The instance registered for [`DebugAssertionsRule`].
pub const DEBUG_ASSERTIONS_RULE: DebugAssertionsRule = DebugAssertionsRule;

impl Rule for DebugAssertionsRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let profile = &context.resolution.profile;
        if !profile.debug_assertions.value || !is_release_like(profile) {
            return Vec::new();
        }

        let metadata = registered("profile/debug-assertions");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: "true".to_owned(),
                recommended: "false".to_owned(),
            }),
            location_of(&profile.debug_assertions),
            "Debug assertions are enabled in a release-like profile",
            "Debug assertions and internal consistency checks are compiled into \
             the shipped artifact."
                .to_owned(),
            "The removed checks catch bugs; an invariant they guard goes \
             unnoticed in the release build.",
        )]
    }
}

/// Reports overflow checks enabled in a release-like profile.
pub struct OverflowChecksRule;

/// The instance registered for [`OverflowChecksRule`].
pub const OVERFLOW_CHECKS_RULE: OverflowChecksRule = OverflowChecksRule;

impl Rule for OverflowChecksRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !profile_rules_apply(context) {
            return Vec::new();
        }

        let profile = &context.resolution.profile;
        if !profile.overflow_checks.value || !is_release_like(profile) {
            return Vec::new();
        }

        let metadata = registered("profile/overflow-checks");

        vec![build(
            metadata,
            None,
            Some(Change {
                current: "true".to_owned(),
                recommended: "false".to_owned(),
            }),
            location_of(&profile.overflow_checks),
            "Overflow checks are enabled in a release-like profile",
            "Integer arithmetic is checked for overflow in the shipped artifact.".to_owned(),
            "With the checks removed, arithmetic overflow no longer panics; it \
             wraps and continues with a wrong value.",
        )]
    }
}

/// Render a `panic` value the way a manifest spells it.
fn render_panic(value: PanicStrategy) -> String {
    match value {
        PanicStrategy::Unwind => "\"unwind\"".to_owned(),
        PanicStrategy::Abort => "\"abort\"".to_owned(),
    }
}

/// Reports a member manifest whose profile table cargo ignores.
///
/// This rule is not subject to the linked-artifact gate: a member producing only
/// an `rlib` still has an inert profile table, and the finding is about the
/// manifest rather than about the artifact.
pub struct IgnoredMemberProfileRule;

/// The instance registered for [`IgnoredMemberProfileRule`].
pub const IGNORED_MEMBER_PROFILE_RULE: IgnoredMemberProfileRule = IgnoredMemberProfileRule;

impl Rule for IgnoredMemberProfileRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("workspace/ignored-member-profile");
        let mut findings = Vec::new();

        for (name, manifest) in context.member_manifests {
            // The presence of a `profile` key is the whole condition; parsing
            // the settings would validate a table this rule does not read.
            if manifest.document().as_table().get("profile").is_none() {
                continue;
            }

            let location = manifest.key_span("profile");

            let mut finding = build(
                metadata,
                Some(name.clone()),
                None,
                location,
                "A member manifest declares a profile table cargo ignores",
                "Cargo honors a `[profile.*]` table only in the workspace root \
                 manifest, so this member's table has no effect. Moving the \
                 settings to the root makes them take effect."
                    .to_owned(),
                "The table is silently inert, so removing it changes nothing \
                 about the artifact.",
            );
            finding.magnitude = crate::finding::Magnitude::None;

            findings.push(finding);
        }

        findings
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
        advice: None,
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
