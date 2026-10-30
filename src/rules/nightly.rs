//! Rules about the compiler behavior only a nightly channel offers.
//!
//! A `-Z` flag is unstable: rustc accepts it only on a nightly toolchain, cargo
//! passes it through as a rustflag like any other, and the flag's spelling and
//! semantics may change between nightly releases. Every rule here therefore
//! reads nothing at all on a stable or beta channel, not even to report that the
//! option exists, because a project that does not build with nightly has no use
//! for a suggestion it cannot apply.
//!
//! A rule in this module reads the live rustflags cargo actually applies, the
//! same [`crate::cargo_config::ResolvedRustflags`] the `config/*` rules read, so
//! a `-Z` flag declared in a shadowed rank is not mistaken for one the compiler
//! sees. A nightly remedy rewrites the flags the artifact is built from, which
//! is why the findings are consequential only when the analysis gate accepts a
//! nightly-only remedy and report-only otherwise.

use crate::cargo_config::{ResolvedRustflags, RustflagsEntry, RustflagsSource, unstable_flag};
use crate::finding::{
    Change, Confidence, Effort, Finding, Fixability, Location, RuleId, RuntimeCost,
};
use crate::manifest::KeySpan;
use crate::rules::config::{config_rules_apply, intended_span};
use crate::rules::{Rule, RuleContext, RuleMetadata};
use camino::Utf8Path;

/// The unstable book's page for `-Z location-detail`.
const LOCATION_DETAIL_REFERENCE: &str =
    "https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/location-detail.html";

/// The unstable book's page for `-Z panic-immediate-abort`.
const PANIC_IMMEDIATE_ABORT_REFERENCE: &str =
    "https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/panic-immediate-abort.html";

/// Cargo's documentation of the `-Z build-std` option the panic strategy
/// requires.
const BUILD_STD_REFERENCE: &str =
    "https://doc.rust-lang.org/cargo/reference/unstable.html#build-std";

/// Reports a nightly build whose panic locations are retained.
///
/// `-Z location-detail=none` makes the compiler drop the source location a
/// panic report would otherwise carry. The setting only takes effect under
/// nightly, so the rule is silent on every other channel.
///
/// The rule reports a package whose live rustflags leave the option at any
/// other state: absent, bare, or valued with something other than `none`.
///
/// A `-Z location-detail=none` declared in a rank cargo ignores does not count
/// as configured: the remedy adds the flag to the rank cargo applies, and a
/// declaration in a rank it ignores cannot satisfy that. A build carrying such
/// a declaration therefore still reports, and the finding's detail says where
/// the ignored declaration sits, because the option is declared but not absent.
pub struct NightlyLocationDetailRule;

/// The instance registered for [`NightlyLocationDetailRule`].
pub const NIGHTLY_LOCATION_DETAIL_RULE: NightlyLocationDetailRule = NightlyLocationDetailRule;

impl Rule for NightlyLocationDetailRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !context.toolchain.is_nightly() {
            return Vec::new();
        }

        let metadata = registered("nightly/location-detail");
        let mut findings = Vec::new();

        for (package, resolved) in context.cargo_configs {
            if !config_rules_apply(context, package) {
                continue;
            }

            let setting = resolved.unstable_flag("location-detail");

            // The option is configured only in its valued `none` spelling: a
            // bare `-Z location-detail` selects rustc's own default, which keeps
            // the full location a panic report prints.
            if matches!(&setting, Some(Some(value)) if value == "none") {
                continue;
            }

            let current = match &setting {
                None => "unset".to_owned(),
                Some(None) => "location-detail".to_owned(),
                Some(Some(value)) => format!("location-detail={value}"),
            };

            let state = match &setting {
                None => "The live rustflags do not set `-Z location-detail=none`".to_owned(),
                Some(None) => {
                    "`-Z location-detail` is set without a value, so rustc keeps its own \
                     default"
                        .to_owned()
                }
                Some(Some(value)) => format!("`-Z location-detail={value}` is set instead"),
            };

            let detail = if shadowed_location_detail(&resolved.shadowed) {
                format!(
                    "{state}, so a panic report still prints the source location it came from. \
                     `-Z location-detail=none` is declared in a rustflags rank cargo does not \
                     apply, so it must be moved to, or added to, the rank cargo applies. \
                     `-Z location-detail=none` is a nightly compiler option, accepted only on a \
                     nightly channel."
                )
            } else {
                format!(
                    "{state}, so a panic report still prints the source location it came from. \
                     `-Z location-detail=none` is a nightly compiler option, accepted only on a \
                     nightly channel."
                )
            };

            findings.push(build(
                metadata,
                context,
                Some(package.clone()),
                resolved,
                FindingFields {
                    change: Change {
                        current,
                        recommended: "location-detail=none".to_owned(),
                    },
                    title: "Panic reports retain source locations",
                    detail: &detail,
                    tradeoff: "Panic reports lose their source locations: a crash no longer \
                               names the file and line it came from. The flag is unstable, so \
                               an upgrade of nightly may change or remove it.",
                    references: vec![LOCATION_DETAIL_REFERENCE.to_owned()],
                },
            ));
        }

        findings
    }
}

/// Reports a nightly build whose panics unwind instead of aborting
/// immediately.
///
/// `-Z panic-immediate-abort` makes the standard library's panic path abort
/// rather than unwind. The flag is unstable and needs the standard library
/// rebuilt with the same strategy, so it only takes effect in a
/// `-Z build-std` build.
pub struct NightlyPanicImmediateAbortRule;

/// The instance registered for [`NightlyPanicImmediateAbortRule`].
pub const NIGHTLY_PANIC_IMMEDIATE_ABORT_RULE: NightlyPanicImmediateAbortRule =
    NightlyPanicImmediateAbortRule;

impl Rule for NightlyPanicImmediateAbortRule {
    fn evaluate(&self, context: &RuleContext<'_>) -> Vec<Finding> {
        if !context.toolchain.is_nightly() {
            return Vec::new();
        }

        let metadata = registered("nightly/panic-immediate-abort");
        let mut findings = Vec::new();

        for (package, resolved) in context.cargo_configs {
            if !config_rules_apply(context, package) {
                continue;
            }

            // The option is a switch with no value, so any occurrence counts as
            // configured however it is spelled; its value, when one is
            // attached, is not a state this rule distinguishes.
            if resolved.unstable_flag("panic-immediate-abort").is_some() {
                continue;
            }

            findings.push(build(
                metadata,
                context,
                Some(package.clone()),
                resolved,
                FindingFields {
                    change: Change {
                        current: "unset".to_owned(),
                        recommended: "panic-immediate-abort".to_owned(),
                    },
                    title: "The build aborts immediately on panic",
                    detail: &format!(
                        "`-Z panic-immediate-abort` is not among the live rustflags for \
                         `{package}`, so a panic unwinds through the standard library's unwind \
                         machinery instead of stopping immediately. The flag is a nightly \
                         compiler option, and it only takes effect in a `-Z build-std` build \
                         whose standard library is compiled with the same strategy."
                    ),
                    tradeoff: "A panic aborts without unwinding, so `catch_unwind` never \
                               recovers and the destructors of the values on the stack do not \
                               run. The technique needs a `-Z build-std` build and may break \
                               across nightly releases.",
                    references: vec![
                        PANIC_IMMEDIATE_ABORT_REFERENCE.to_owned(),
                        BUILD_STD_REFERENCE.to_owned(),
                    ],
                },
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

/// Whether any rank cargo ignores declares `-Z location-detail=none`.
///
/// A shadowed declaration never reaches the compiler, so it cannot silence the
/// rule, but a finding that described the option as absent would be false about
/// a configuration file that does declare it.
fn shadowed_location_detail(shadowed: &[RustflagsEntry]) -> bool {
    shadowed.iter().any(|entry| {
        matches!(unstable_flag(&entry.flags, "location-detail"), Some(Some(value)) if value == "none")
    })
}

/// The confidence a nightly finding carries.
///
/// A `cfg(...)` target table counts as live purely because it is present, and
/// this crate evaluates no predicate to decide whether it would match the
/// host cargo builds for; a verdict resting on that unevaluated guess is
/// capped below certainty, the same way the `config/*` rules cap theirs.
fn confidence_of(resolved: &ResolvedRustflags) -> Confidence {
    if resolved.source == RustflagsSource::TargetTables && resolved.cfg_tables_unevaluated {
        Confidence::Medium
    } else {
        Confidence::High
    }
}

/// The location a nightly finding points at.
///
/// The first live entry in chain order names the key in the configuration file
/// nearest the package among the chain files carrying the resolved rank; that
/// is the key a reader edits to change the flags cargo applies. When the
/// resolution has no live entry — an environment-supplied `RUSTFLAGS` or
/// `CARGO_ENCODED_RUSTFLAGS`, or a chain reaching no configuration file — the
/// finding points at `build.rustflags` in the workspace root's
/// `.cargo/config.toml`, the file a remedy would create.
fn location_of(workspace_root: &Utf8Path, resolved: &ResolvedRustflags) -> KeySpan {
    resolved
        .live
        .first()
        .map(|entry| entry.key.clone())
        .unwrap_or_else(|| intended_span(workspace_root, "build.rustflags"))
}

/// The rule-specific text a nightly finding carries.
struct FindingFields<'a> {
    change: Change,
    title: &'a str,
    detail: &'a str,
    tradeoff: &'a str,
    references: Vec<String>,
}

/// Build a finding for one package's live rustflags.
///
/// The constant fields are read from the registry entry, so a rule cannot drift
/// from the entry that documents it; severity comes from
/// [`crate::rules::severity_of`], the single assignment table. Only the gate
/// decides whether the remedy is consequential or report-only. `fields` carries
/// the text the rule owns, including the documentation of the unstable option
/// it recommends.
fn build(
    metadata: &'static RuleMetadata,
    context: &RuleContext<'_>,
    package: Option<String>,
    resolved: &ResolvedRustflags,
    fields: FindingFields<'_>,
) -> Finding {
    let severity = crate::rules::severity_of(&metadata.rule_id())
        .expect("every registered rule has a severity assignment");

    let fixability = if context.allow_nightly {
        Fixability::Consequential
    } else {
        Fixability::ReportOnly
    };

    Finding {
        rule: metadata.rule_id(),
        package,
        tier: metadata.tier,
        severity,
        magnitude: metadata.magnitude,
        confidence: confidence_of(resolved),
        effort: Effort::Trivial,
        runtime_cost: RuntimeCost::None,
        fixability,
        change: Some(fields.change),
        location: Some(Location::new(location_of(
            &context.resolution.workspace_root,
            resolved,
        ))),
        title: fields.title.to_owned(),
        detail: fields.detail.to_owned(),
        advice: None,
        tradeoff: fields.tradeoff.to_owned(),
        references: fields.references,
    }
}

#[cfg(test)]
mod tests {
    use super::{confidence_of, location_of, shadowed_location_detail};
    use crate::cargo_config::{ResolvedRustflags, RustflagsEntry, RustflagsSource};
    use crate::finding::Confidence;
    use crate::manifest::KeySpan;
    use camino::{Utf8Path, Utf8PathBuf};

    /// A configuration entry declaring one flag list at `key`.
    fn entry(file: &str, key: &str, flags: &[&str]) -> RustflagsEntry {
        RustflagsEntry {
            key: KeySpan {
                file: Utf8PathBuf::from(file),
                key: key.to_owned(),
                line: 1,
                column: 1,
            },
            flags: flags.iter().map(|flag| (*flag).to_owned()).collect(),
        }
    }

    #[test]
    fn confidence_is_capped_for_an_unevaluated_cfg_table() {
        let target_tables = |cfg_tables_unevaluated| ResolvedRustflags {
            source: RustflagsSource::TargetTables,
            flags: vec!["-C".to_owned(), "target-cpu=native".to_owned()],
            live: vec![entry(
                "/probe/.cargo/config.toml",
                "target.'cfg(unix)'.rustflags",
                &["-C", "target-cpu=native"],
            )],
            shadowed: Vec::new(),
            cfg_tables_unevaluated,
        };

        assert_eq!(
            confidence_of(&target_tables(true)),
            Confidence::Medium,
            "a verdict resting on an unevaluated cfg predicate is capped"
        );
        assert_eq!(
            confidence_of(&target_tables(false)),
            Confidence::High,
            "a matching triple table is read exactly"
        );

        let build_table = ResolvedRustflags {
            source: RustflagsSource::BuildTable,
            flags: vec!["-C".to_owned(), "target-cpu=native".to_owned()],
            live: vec![entry(
                "/probe/.cargo/config.toml",
                "build.rustflags",
                &["-C", "target-cpu=native"],
            )],
            shadowed: Vec::new(),
            cfg_tables_unevaluated: true,
        };

        assert_eq!(
            confidence_of(&build_table),
            Confidence::High,
            "flags the build table supplies carry no predicate at all"
        );
    }

    #[test]
    fn location_is_the_first_live_key_or_the_intended_one() {
        let workspace_root = Utf8Path::new("/probe");

        let with_live = ResolvedRustflags {
            source: RustflagsSource::TargetTables,
            flags: vec!["-C".to_owned(), "target-cpu=native".to_owned()],
            live: vec![
                entry(
                    "/probe/pkg/.cargo/config.toml",
                    "target.'cfg(unix)'.rustflags",
                    &["-C", "target-cpu=native"],
                ),
                entry(
                    "/probe/.cargo/config.toml",
                    "build.rustflags",
                    &["-C", "target-cpu=native"],
                ),
            ],
            shadowed: Vec::new(),
            cfg_tables_unevaluated: true,
        };

        assert_eq!(
            location_of(workspace_root, &with_live),
            with_live.live[0].key,
            "the nearest live entry names the key a reader edits"
        );

        let without_live = ResolvedRustflags {
            source: RustflagsSource::Unset,
            flags: Vec::new(),
            live: Vec::new(),
            shadowed: Vec::new(),
            cfg_tables_unevaluated: false,
        };
        let location = location_of(workspace_root, &without_live);

        assert_eq!(location.key, "build.rustflags");
        assert_eq!(
            location.file,
            Utf8PathBuf::from("/probe/.cargo/config.toml"),
            "a resolution with no live entry points at the file a remedy creates"
        );
    }

    #[test]
    fn a_shadowed_declaration_is_recognized_in_its_valued_none_spelling() {
        assert!(!shadowed_location_detail(&[]));
        assert!(!shadowed_location_detail(&[entry(
            "/probe/.cargo/config.toml",
            "build.rustflags",
            &["-Z", "location-detail"],
        )]));
        assert!(!shadowed_location_detail(&[entry(
            "/probe/.cargo/config.toml",
            "build.rustflags",
            &["-Z", "location-detail=file"],
        )]));
        assert!(shadowed_location_detail(&[entry(
            "/probe/.cargo/config.toml",
            "build.rustflags",
            &["-Zlocation-detail=none"],
        )]));
    }
}
