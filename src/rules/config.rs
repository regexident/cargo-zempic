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

use crate::cargo_config::{RustflagsOrigin, RustflagsSource};
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
/// Every source yields a description. [`RustflagsSource::Environment`] is
/// named by its environment variable; [`RustflagsSource::TargetTables`] and
/// [`RustflagsSource::BuildTable`] are named by the dotted key of every live
/// chain entry. A source with no live entry to name, such as
/// [`RustflagsSource::Unset`], falls back to "the cargo configuration".
fn live_source_description(resolved: &crate::cargo_config::ResolvedRustflags) -> String {
    match &resolved.source {
        RustflagsSource::Environment { variable } => {
            format!("the `{}` environment variable", variable.as_str())
        }
        RustflagsSource::TargetTables | RustflagsSource::BuildTable
            if !resolved.live.is_empty() =>
        {
            let keys: Vec<&str> = resolved.live.iter().map(|entry| entry.name()).collect();

            format!("`{}`", keys.join("`, `"))
        }
        RustflagsSource::TargetTables | RustflagsSource::BuildTable | RustflagsSource::Unset => {
            "the cargo configuration".to_owned()
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
                    entry.name(),
                );

                findings.push(build(
                    metadata,
                    Some(package.clone()),
                    confidence,
                    metadata.fixability,
                    None,
                    entry.key_span().cloned(),
                    &format!("`{}` is inert", entry.name()),
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

/// Where a finding about `-C link-dead-code` points and how it names its
/// source.
struct DeadCodeSource {
    /// The key a remedy would edit.
    location: KeySpan,
    /// Whether the remedy can be written to a file the project owns.
    fixability: Fixability,
    /// The source as it reads inside a finding's detail text.
    description: String,
}

/// Locate the source of `-C link-dead-code` in `resolved`.
///
/// `resolved.flags` is the concatenation of every live entry and does not say
/// which one contributed the flag, so each live entry's own flags are checked
/// individually. A remedy can only be written to a file the project owns, so
/// an entry in a chain file outside `workspace_root` is reported with
/// [`Fixability::ReportOnly`] and any other with `fixability`. When no live
/// entry carries the flag, the resolution is unattributable, and the result
/// points at the file a remedy would create. An entry read from an environment
/// variable has no file to edit and is reported without a fix.
fn dead_code_source(
    resolved: &crate::cargo_config::ResolvedRustflags,
    workspace_root: &camino::Utf8Path,
    fixability: Fixability,
) -> DeadCodeSource {
    if let RustflagsSource::Environment { variable } = &resolved.source {
        return DeadCodeSource {
            location: intended_span(workspace_root, "build.rustflags"),
            fixability: Fixability::ReportOnly,
            description: format!("the `{}` environment variable", variable.as_str()),
        };
    }

    let carrying_entry = match resolved.source {
        RustflagsSource::TargetTables | RustflagsSource::BuildTable => resolved
            .live
            .iter()
            .find(|entry| crate::cargo_config::flag_enabled(&entry.flags, "link-dead-code")),
        RustflagsSource::Environment { .. } | RustflagsSource::Unset => None,
    };

    if let Some(RustflagsOrigin::Environment { variable }) =
        carrying_entry.map(|entry| &entry.origin)
    {
        return DeadCodeSource {
            location: intended_span(workspace_root, "build.rustflags"),
            fixability: Fixability::ReportOnly,
            description: format!("the `{variable}` environment variable"),
        };
    }

    let Some(span) = carrying_entry.and_then(|entry| entry.key_span()) else {
        return DeadCodeSource {
            location: intended_span(workspace_root, "build.rustflags"),
            fixability: Fixability::ReportOnly,
            description: "the cargo configuration".to_owned(),
        };
    };

    let fixability = if span.file.starts_with(workspace_root) {
        fixability
    } else {
        Fixability::ReportOnly
    };

    DeadCodeSource {
        location: span.clone(),
        fixability,
        description: format!("`{}` in `{}`", span.key, span.file),
    }
}

impl Rule for LinkDeadCodeRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        let metadata = registered("config/link-dead-code");
        let mut findings = Vec::new();

        for (package, resolved) in context.cargo_configs {
            if !config_rules_apply(context, package) {
                continue;
            }

            if !resolved.flag_enabled("link-dead-code") {
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

            let DeadCodeSource {
                location,
                fixability,
                description: source_description,
            } = dead_code_source(
                resolved,
                &context.resolution.workspace_root,
                metadata.fixability,
            );

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

            let explicit_flag_present = resolved.link_arguments().iter().any(|argument| {
                let lowered = argument.to_ascii_lowercase();
                lowered.contains("--gc-sections")
                    || lowered.contains("-dead_strip")
                    || lowered.contains("/opt:ref")
            });

            if explicit_flag_present {
                continue;
            }

            let dead_code_live = resolved.flag_enabled("link-dead-code");
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
                .iter()
                .find_map(|entry| entry.key_span().cloned())
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
                let live_description = dead_code_source(
                    resolved,
                    &context.resolution.workspace_root,
                    metadata.fixability,
                )
                .description;

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

#[cfg(test)]
mod tests {
    use super::{dead_code_source, intended_span, live_source_description};
    use crate::cargo_config::{
        EnvironmentVariable, ResolvedRustflags, RustflagsEntry, RustflagsOrigin, RustflagsSource,
    };
    use crate::finding::Fixability;
    use crate::manifest::KeySpan;
    use camino::{Utf8Path, Utf8PathBuf};

    /// A configuration entry declaring one flag list at `key`.
    fn entry(file: &str, key: &str, flags: &[&str]) -> RustflagsEntry {
        RustflagsEntry {
            origin: RustflagsOrigin::File(KeySpan {
                file: Utf8PathBuf::from(file),
                key: key.to_owned(),
                line: 1,
                column: 1,
            }),
            flags: flags.iter().map(|flag| (*flag).to_owned()).collect(),
        }
    }

    fn resolved(source: RustflagsSource, live: Vec<RustflagsEntry>) -> ResolvedRustflags {
        ResolvedRustflags {
            source,
            flags: live
                .iter()
                .flat_map(|entry| entry.flags.iter().cloned())
                .collect(),
            live,
            shadowed: Vec::new(),
            cfg_tables_unevaluated: false,
        }
    }

    #[test]
    fn dead_code_source_names_the_carrying_entry() {
        let resolved = resolved(
            RustflagsSource::BuildTable,
            vec![
                entry(
                    "/w/.cargo/config.toml",
                    "target.'cfg(unix)'.rustflags",
                    &["-C", "opt-level=z"],
                ),
                entry(
                    "/w/.cargo/config.toml",
                    "build.rustflags",
                    &["-C", "link-dead-code"],
                ),
            ],
        );

        let inside = dead_code_source(&resolved, Utf8Path::new("/w"), Fixability::Safe);

        assert_eq!(inside.location.key, "build.rustflags");
        assert_eq!(inside.fixability, Fixability::Safe);
        assert_eq!(
            inside.description,
            "`build.rustflags` in `/w/.cargo/config.toml`"
        );

        let outside = dead_code_source(&resolved, Utf8Path::new("/elsewhere"), Fixability::Safe);

        assert_eq!(outside.fixability, Fixability::ReportOnly);
    }

    #[test]
    fn dead_code_source_reports_the_environment_variable() {
        let resolved = ResolvedRustflags {
            source: RustflagsSource::Environment {
                variable: EnvironmentVariable::Rustflags,
            },
            flags: vec!["-C".to_owned(), "link-dead-code".to_owned()],
            live: Vec::new(),
            shadowed: Vec::new(),
            cfg_tables_unevaluated: false,
        };

        let source = dead_code_source(&resolved, Utf8Path::new("/w"), Fixability::Safe);

        assert_eq!(
            source.location,
            intended_span(Utf8Path::new("/w"), "build.rustflags")
        );
        assert_eq!(source.fixability, Fixability::ReportOnly);
        assert_eq!(source.description, "the `RUSTFLAGS` environment variable");
    }

    #[test]
    fn dead_code_source_falls_back_without_a_carrying_entry() {
        let unset = resolved(RustflagsSource::Unset, Vec::new());
        let split = resolved(
            RustflagsSource::TargetTables,
            vec![
                entry("/w/.cargo/config.toml", "target.x.rustflags", &["-C"]),
                entry(
                    "/w/.cargo/config.toml",
                    "target.'cfg(unix)'.rustflags",
                    &["link-dead-code"],
                ),
            ],
        );

        for resolved in [unset, split] {
            let source = dead_code_source(&resolved, Utf8Path::new("/w"), Fixability::Safe);

            assert_eq!(
                source.location,
                intended_span(Utf8Path::new("/w"), "build.rustflags")
            );
            assert_eq!(source.fixability, Fixability::ReportOnly);
            assert_eq!(source.description, "the cargo configuration");
        }
    }

    #[test]
    fn live_source_description_names_build_table_keys() {
        let resolved = resolved(
            RustflagsSource::BuildTable,
            vec![entry(
                "/w/.cargo/config.toml",
                "build.rustflags",
                &["-C", "opt-level=z"],
            )],
        );

        assert_eq!(live_source_description(&resolved), "`build.rustflags`");
    }

    #[test]
    fn live_source_description_falls_back_without_live_entries() {
        let unset = resolved(RustflagsSource::Unset, Vec::new());
        let target_tables = resolved(RustflagsSource::TargetTables, Vec::new());

        assert_eq!(live_source_description(&unset), "the cargo configuration");
        assert_eq!(
            live_source_description(&target_tables),
            "the cargo configuration"
        );
    }
}
