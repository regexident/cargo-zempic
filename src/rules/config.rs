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

use crate::cargo_config::{RustflagsEntry, RustflagsSource};
use crate::finding::{
    Change, Confidence, Effort, Finding, Fixability, Location, RuleId, RuntimeCost,
};
use crate::manifest::KeySpan;
use crate::rules::{Rule, RuleContext, RuleMetadata};

/// Whether `package` is selected and emits an artifact a linker flag governs.
///
/// §7.1: a linker flag governs a linked artifact but not an `rlib` or a
/// procedural macro, which is built for the host rather than linked into the
/// program under analysis. A rule reading [`RuleContext::cargo_configs`] gates
/// on this predicate for the same reason the `profile/*` rules gate on
/// [`crate::rules::profile`]'s own copy of it.
pub(crate) fn config_rules_apply(context: &RuleContext<'_>, package: &str) -> bool {
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
pub(crate) fn intended_span(workspace_root: &camino::Utf8Path, key: &str) -> KeySpan {
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
                    metadata.fixability,
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

/// Reports a build whose live rustflags carry `-C link-dead-code`.
///
/// The flag exists so coverage instrumentation can observe code the program
/// never calls; outside a coverage run it only enlarges the artifact for no
/// benefit. A shadowed occurrence of the flag is not reported here, by the
/// same reasoning [`RustflagsShadowedRule`] exists for: a flag cargo never
/// applies is not a property of the artifact.
pub struct LinkDeadCodeRule;

/// The instance registered for [`LinkDeadCodeRule`].
pub const LINK_DEAD_CODE_RULE: LinkDeadCodeRule = LinkDeadCodeRule;

/// The live entry whose own flags carry `link-dead-code`, among `live`.
///
/// `resolved.flags` is the concatenation of every live entry and does not say
/// which one contributed the flag, so the search re-checks each entry's own
/// flags individually to find the key a finding should point at.
///
/// # Panics
///
/// Panics when no entry in `live` carries the flag. A caller holding a
/// `TargetTables` or `BuildTable` source whose concatenated `flags` enables
/// `link-dead-code` always has at least one such entry, because the
/// concatenation is built only from `live`'s own entries.
fn live_entry_carrying_link_dead_code(live: &[RustflagsEntry]) -> &RustflagsEntry {
    live.iter()
        .find(|entry| crate::cargo_config::flag_enabled(&entry.flags, "link-dead-code"))
        .expect(
            "a concatenation carrying the flag is built only from `live`'s own entries, \
             so one of them carries it",
        )
}

impl Rule for LinkDeadCodeRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("config/link-dead-code");
        let mut findings = Vec::new();

        for (package, resolved) in context.cargo_configs {
            if !config_rules_apply(context, package) {
                continue;
            }

            if !crate::cargo_config::flag_enabled(&resolved.flags, "link-dead-code") {
                continue;
            }

            // Same confidence cap as `RustflagsShadowedRule`: an unevaluated
            // `cfg(...)` table may be the only reason this rank won at all.
            let confidence = if resolved.source == RustflagsSource::TargetTables
                && resolved.cfg_tables_unevaluated
            {
                Confidence::Medium
            } else {
                Confidence::High
            };

            let (location, fixability, source_description) = match &resolved.source {
                RustflagsSource::Environment { variable } => (
                    intended_span(&context.resolution.workspace_root, "build.rustflags"),
                    Fixability::ReportOnly,
                    format!("the `{}` environment variable", variable.as_str()),
                ),
                RustflagsSource::TargetTables | RustflagsSource::BuildTable => {
                    let entry = live_entry_carrying_link_dead_code(&resolved.live);

                    // A remedy can only be written to a file the project
                    // owns; a chain file above the workspace root belongs to
                    // the user's machine, not to the project under analysis.
                    let outside_workspace = !entry
                        .key
                        .file
                        .starts_with(&context.resolution.workspace_root);

                    let fixability = if outside_workspace {
                        Fixability::ReportOnly
                    } else {
                        metadata.fixability
                    };

                    (
                        entry.key.clone(),
                        fixability,
                        format!("`{}` in `{}`", entry.key.key, entry.key.file),
                    )
                }
                RustflagsSource::Unset => unreachable!(
                    "`Unset` carries no flags, so `flag_enabled` on it is always false \
                     and this arm is never reached"
                ),
            };

            let downgrade_note = if fixability == Fixability::ReportOnly {
                " This source lies outside the workspace, so the change is the user's to \
                 make."
            } else {
                ""
            };

            let detail = format!(
                "{source_description} sets `-C link-dead-code`, which keeps symbols the \
                 linker would otherwise discard.{downgrade_note}"
            );

            findings.push(build(
                metadata,
                Some(package.clone()),
                confidence,
                fixability,
                None,
                Some(location),
                "The build retains dead code",
                detail,
                "The flag increases size; rarely intended outside coverage runs. Like any \
                 rustflags change, removing it invalidates the whole build cache, costing \
                 a full rebuild.",
            ));
        }

        findings
    }
}

/// The linker family a target triple resolves to, so far as its substrings
/// reveal.
///
/// Classification reads only the triple's text; it never inspects a linker
/// binary or a `cc` invocation. [`Unknown`](Self::Unknown) is the honest
/// answer for a triple this crate's substring table does not cover, rather
/// than a guess at a linker that might not apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkerFamily {
    /// A Unix-like target linked with an ELF linker such as GNU `ld` or
    /// `lld`.
    Elf,
    /// An Apple target linked with Apple's `ld`.
    MachO,
    /// A Windows target linked with the MSVC linker.
    Msvc,
    /// A WebAssembly target linked with `wasm-ld`.
    Wasm,
    /// A triple whose linker this crate does not classify.
    Unknown,
}

/// Classify `target_triple` by the linker rustc hands its output to.
///
/// The order below is load-bearing: `windows-msvc` is checked before the ELF
/// arm's `windows-gnu` substring, so an MSVC triple is never misclassified as
/// an ELF one.
fn linker_family(target_triple: &str) -> LinkerFamily {
    if target_triple.contains("-apple-") {
        LinkerFamily::MachO
    } else if target_triple.contains("windows-msvc") {
        LinkerFamily::Msvc
    } else if target_triple.contains("wasm") {
        LinkerFamily::Wasm
    } else if [
        "linux",
        "windows-gnu",
        "android",
        "freebsd",
        "netbsd",
        "openbsd",
        "dragonfly",
        "solaris",
        "illumos",
        "fuchsia",
        "redox",
    ]
    .iter()
    .any(|substring| target_triple.contains(substring))
    {
        LinkerFamily::Elf
    } else {
        LinkerFamily::Unknown
    }
}

/// The rustflag that requests linker section garbage collection for `family`,
/// or `None` when the family's own spelling is not known.
fn section_garbage_collection_flag(family: LinkerFamily) -> Option<&'static str> {
    match family {
        LinkerFamily::Elf => Some("-C link-arg=-Wl,--gc-sections"),
        LinkerFamily::MachO => Some("-C link-arg=-Wl,-dead_strip"),
        LinkerFamily::Msvc => Some("-C link-arg=/OPT:REF"),
        LinkerFamily::Wasm => Some("-C link-arg=--gc-sections"),
        LinkerFamily::Unknown => None,
    }
}

/// Whether rustc passes `family`'s linker a section-garbage-collection flag
/// of its own accord, absent `-C link-dead-code`.
///
/// Every classified family defaults to garbage-collecting unreferenced
/// sections; only [`LinkerFamily::Unknown`] carries no such default to claim.
fn performs_section_garbage_collection_by_default(family: LinkerFamily) -> bool {
    family != LinkerFamily::Unknown
}

/// Reports a linker that retains sections nothing references, because
/// rustc's own section-garbage-collection default is either suppressed or
/// unknown to this crate.
///
/// rustc passes `--gc-sections` to an ELF linker and `-dead_strip` to a
/// Mach-O one on its own, without cargo or the user asking for it, and stops
/// doing so under `-C link-dead-code` — the same flag [`LinkDeadCodeRule`]
/// reports. This rule is therefore quiet on a project that sets no flag at
/// all on a target it can classify: the absence of an explicit flag is not
/// evidence of anything, because the default already applies. It fires only
/// when that default is suppressed, or when the resolved target's linker is
/// one this crate does not classify and so cannot claim a default for.
pub struct LinkGcSectionsRule;

/// The instance registered for [`LinkGcSectionsRule`].
pub const LINK_GC_SECTIONS_RULE: LinkGcSectionsRule = LinkGcSectionsRule;

impl Rule for LinkGcSectionsRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("config/link-gc-sections");
        let mut findings = Vec::new();

        for (package, resolved) in context.cargo_configs {
            if !config_rules_apply(context, package) {
                continue;
            }

            let explicit_flag_present = crate::cargo_config::link_arguments(&resolved.flags)
                .iter()
                .any(|argument| {
                    let lowered = argument.to_ascii_lowercase();
                    lowered.contains("--gc-sections")
                        || lowered.contains("-dead_strip")
                        || lowered.contains("/opt:ref")
                });

            if explicit_flag_present {
                continue;
            }

            let dead_code_live =
                crate::cargo_config::flag_enabled(&resolved.flags, "link-dead-code");
            let family = linker_family(context.target_triple);
            let unclassified = family == LinkerFamily::Unknown;

            if !dead_code_live && !unclassified {
                continue;
            }

            debug_assert!(
                dead_code_live || !performs_section_garbage_collection_by_default(family),
                "an unclassified family is the only remaining reason to fire here"
            );

            let cfg_table_unevaluated_in_live_rank =
                resolved.source == RustflagsSource::TargetTables && resolved.cfg_tables_unevaluated;

            // Dead-code-live outranks an unclassified target: once rustc's own
            // default is suppressed by `-C link-dead-code`, that is the cause
            // reported here, regardless of whether the target is also unclassified.
            let confidence = if dead_code_live {
                if cfg_table_unevaluated_in_live_rank {
                    Confidence::Medium
                } else {
                    Confidence::High
                }
            } else if unclassified {
                Confidence::Medium
            } else {
                Confidence::High
            };

            let location = resolved
                .live
                .first()
                .map(|entry| entry.key.clone())
                .unwrap_or_else(|| {
                    intended_span(&context.resolution.workspace_root, "build.rustflags")
                });

            let fixability = if matches!(resolved.source, RustflagsSource::Environment { .. }) {
                Fixability::ReportOnly
            } else {
                metadata.fixability
            };

            let recommended_flag = section_garbage_collection_flag(family);
            let change = recommended_flag.map(|flag| Change {
                current: "unset".to_owned(),
                recommended: flag.to_owned(),
            });

            let detail = if !dead_code_live && unclassified {
                format!(
                    "The resolved target `{}` is not a linker family this tool \
                     classifies, so it cannot claim rustc's usual \
                     section-garbage-collection default applies here.",
                    context.target_triple
                )
            } else {
                let live_description = match &resolved.source {
                    RustflagsSource::Environment { variable } => {
                        format!("the `{}` environment variable", variable.as_str())
                    }
                    RustflagsSource::TargetTables | RustflagsSource::BuildTable => {
                        let entry = live_entry_carrying_link_dead_code(&resolved.live);

                        format!("`{}` in `{}`", entry.key.key, entry.key.file)
                    }
                    RustflagsSource::Unset => unreachable!(
                        "`Unset` carries no flags, so `flag_enabled` on it is always false \
                         and this arm is never reached"
                    ),
                };

                format!(
                    "{live_description} sets `-C link-dead-code`, which suppresses rustc's \
                     own section-garbage-collection default. `config/link-dead-code` reports \
                     the flag itself; this finding reports the linker consequence and the \
                     flag that restores the behavior."
                )
            };

            findings.push(build(
                metadata,
                Some(package.clone()),
                confidence,
                fixability,
                change,
                Some(location),
                "The linker retains unreferenced sections",
                detail,
                "Platform-specific: `--gc-sections` on ELF, `-dead_strip` on Mach-O. Like \
                 any rustflags change, adding it invalidates the whole build cache, costing \
                 a full rebuild.",
            ));
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
/// and `fixability` as parameters rather than hardcoding
/// [`Confidence::High`] and the registry entry's own fixability: a rule in
/// this module may report at reduced confidence when its verdict rests on a
/// `cfg(...)` table this crate does not evaluate, and may downgrade
/// fixability when the source it would edit lies outside what it can write
/// to — a file outside the workspace, or an environment variable with no file
/// at all.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors `crate::rules::profile`'s own `build`, plus the `confidence` and \
              `fixability` parameters that module hardcodes; splitting the parameters \
              into a struct would not make a one-call-site constructor any clearer"
)]
fn build(
    metadata: &'static RuleMetadata,
    package: Option<String>,
    confidence: Confidence,
    fixability: Fixability,
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
        fixability,
        change,
        location: location.map(Location::new),
        title: title.to_owned(),
        detail,
        advice: None,
        tradeoff: tradeoff.to_owned(),
        references: Vec::new(),
    }
}
