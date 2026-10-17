//! Rules about which rustflags source cargo actually applies.
//!
//! Cargo takes rustflags from exactly one of three ranks — an environment
//! variable, a matching `target.*` table, or `build.rustflags` — and ignores
//! every other rank entirely; it never merges flags across ranks. A flag's
//! presence in a `config.toml` is therefore not evidence that the compiler
//! ever sees it: the file may declare a rank cargo never reaches. The rules in
//! this module reason over [`crate::cargo_config::ResolvedRustflags`], the
//! already-resolved winner and the ranks it shadowed, rather than over the
//! configuration files' text, so a finding here is about what cargo applies
//! rather than about what a file happens to contain.

use crate::cargo_config::RustflagsSource;
use crate::finding::{Change, Confidence, Effort, Finding, Location, RuleId, RuntimeCost};
use crate::manifest::KeySpan;
use crate::rules::{Rule, RuleContext, RuleMetadata};

/// Whether `package` is selected and emits an artifact a linker flag governs.
///
/// §7.1: a linker flag governs a linked artifact but not an `rlib` or a
/// procedural macro, which is built for the host rather than linked into the
/// program under analysis. A rule reading [`RuleContext::cargo_configs`] gates
/// on this predicate for the same reason the `profile/*` rules gate on
/// [`crate::rules::profile`]'s own copy of it.
fn config_rules_apply(context: &RuleContext<'_>, package: &str) -> bool {
    context.resolution.selected.iter().any(|candidate| {
        candidate.name == package && candidate.classification.produces_linked_artifact()
    })
}

/// The key span a remedy would write when no chain file declares `key` yet.
///
/// `<workspace_root>/.cargo/config.toml` is the file cargo itself would create
/// were a user to run `cargo config edit`-style tooling at the workspace root,
/// so it is the natural destination for a key no file in the chain declares.
/// Line one, column one stands in for "the file does not yet contain this
/// key" rather than naming a real position in an existing document.
#[expect(
    dead_code,
    reason = "reserved for a rule that recommends a key no chain file declares yet"
)]
fn intended_span(workspace_root: &camino::Utf8Path, key: &str) -> KeySpan {
    KeySpan {
        file: workspace_root.join(".cargo").join("config.toml"),
        key: key.to_owned(),
        line: 1,
        column: 1,
    }
}

/// Describe the rustflags source that won resolution, for use in a finding's
/// detail text.
///
/// Named by the environment variable for [`RustflagsSource::Environment`], or
/// by the dotted key of every live chain entry for
/// [`RustflagsSource::TargetTables`]. The other two variants never win
/// alongside a non-empty `shadowed` list — `resolve` records nothing below
/// `build.rustflags`, and `Unset` shadows nothing — so a caller holding a
/// non-empty `shadowed` list never reaches them.
fn live_source_description(resolved: &crate::cargo_config::ResolvedRustflags) -> String {
    match &resolved.source {
        RustflagsSource::Environment { variable } => {
            format!("the `{}` environment variable", variable.as_str())
        }
        RustflagsSource::TargetTables => {
            let keys: Vec<&str> = resolved
                .live
                .iter()
                .map(|entry| entry.key.key.as_str())
                .collect();

            format!("`{}`", keys.join("`, `"))
        }
        RustflagsSource::BuildTable | RustflagsSource::Unset => {
            unreachable!(
                "`resolve` records a shadowed entry only when the environment or a \
                 target table wins, so a non-empty `shadowed` list never carries \
                 either of these sources"
            )
        }
    }
}

/// Reports a rustflags source cargo ignores because a higher-ranked source
/// won resolution.
pub struct RustflagsShadowedRule;

/// The instance registered for [`RustflagsShadowedRule`].
pub const RUSTFLAGS_SHADOWED_RULE: RustflagsShadowedRule = RustflagsShadowedRule;

impl Rule for RustflagsShadowedRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("config/rustflags-shadowed");
        let mut findings = Vec::new();

        for (package, resolved) in context.cargo_configs {
            if !config_rules_apply(context, package) {
                continue;
            }

            if resolved.shadowed.is_empty() {
                continue;
            }

            // A `cfg(...)` table counts as live purely because it is present;
            // this crate evaluates no predicate to decide whether it would
            // actually match the host cargo builds for. When one contributed
            // to the winning rank, the verdict rests on that unevaluated
            // guess, so the finding is capped at medium confidence rather
            // than reported as certain.
            let confidence = if resolved.source == RustflagsSource::TargetTables
                && resolved.cfg_tables_unevaluated
            {
                Confidence::Medium
            } else {
                Confidence::High
            };

            let live_description = live_source_description(resolved);
            let capped_note = if confidence == Confidence::Medium {
                " The live rank includes a `cfg(...)` table whose predicate was not \
                 evaluated, so this verdict is not certain."
            } else {
                ""
            };

            for entry in &resolved.shadowed {
                let detail = format!(
                    "Cargo takes rustflags from one source only. {live_description} is \
                     the source cargo applies here; `{}` is set but never read.{capped_note}",
                    entry.key.key,
                );

                findings.push(build(
                    metadata,
                    Some(package.clone()),
                    confidence,
                    None,
                    Some(entry.key.clone()),
                    &format!("`{}` is inert", entry.key.key),
                    detail,
                    "Existing flags are silently inert.",
                ));
            }
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

/// Build a finding from a registry entry's metadata and the rule-specific
/// fields.
///
/// Mirrors [`crate::rules::profile`]'s private `build`, but takes `confidence`
/// as a parameter rather than hardcoding [`Confidence::High`]: a rule in this
/// module may report at reduced confidence when its verdict rests on a
/// `cfg(...)` table this crate does not evaluate.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors `crate::rules::profile`'s own `build`, plus the `confidence` \
              parameter that module hardcodes; splitting the parameters into a struct \
              would not make a one-call-site constructor any clearer"
)]
fn build(
    metadata: &'static RuleMetadata,
    package: Option<String>,
    confidence: Confidence,
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
        confidence,
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
