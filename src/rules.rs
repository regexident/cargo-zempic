//! The rule registry and the single table that assigns every rule its severity.

use crate::finding::{RuleId, Selector, Severity};

/// The metadata every rule registers: what it is, not what it found.
///
/// The struct carries no severity field. Severity is assigned in exactly one
/// place, [`severity_assignments`], so a rule's severity cannot drift from the
/// table the whole project is reviewed against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleMetadata {
    /// The rule's canonical identifier.
    pub id: &'static str,
    /// The report section the rule's findings belong to.
    pub tier: crate::finding::Tier,
    /// The editorial size of the rule's findings.
    pub magnitude: crate::finding::Magnitude,
    /// Whether the rule's findings can be applied automatically.
    pub fixability: crate::finding::Fixability,
    /// A one-line description of the rule.
    pub summary: &'static str,
    /// The rule's documentation, embedded from its doc file.
    pub documentation: &'static str,
}

impl RuleMetadata {
    /// The rule's identifier, parsed.
    ///
    /// A registered identifier that does not parse is an authoring error the
    /// registry's own test catches before any user can reach it.
    ///
    /// # Panics
    ///
    /// Panics with a message naming the identifier when it is not a well-formed
    /// [`RuleId`].
    pub fn rule_id(&self) -> RuleId {
        RuleId::parse(self.id)
            .unwrap_or_else(|error| panic!("registered rule `{}` is malformed: {error}", self.id))
    }
}

/// Every rule the crate knows, as metadata.
///
/// The registry is the set a run evaluates and the set the documentation and
/// severity tests enumerate. Each entry's severity is looked up in
/// [`severity_assignments`] rather than stored on the entry.
pub fn registry() -> &'static [RuleMetadata] {
    &[]
}

/// The single place every rule's severity is assigned.
///
/// Changing one entry is intended to be a reviewable diff. An entry may be
/// either a complete rule identifier or a category selector standing for a
/// family of rules, which is how the `conflict`, `nightly` and `advisory`
/// families are written.
pub fn severity_assignments() -> &'static [(&'static str, Severity)] {
    const ASSIGNMENTS: &[(&str, Severity)] = &[
        // High
        ("profile/strip", Severity::High),
        ("profile/debug-info", Severity::High),
        ("profile/opt-level:size", Severity::High),
        ("config/link-dead-code", Severity::High),
        ("config/rustflags-shadowed", Severity::High),
        ("workspace/ignored-member-profile", Severity::High),
        ("deps/workspace-resolver", Severity::High),
        ("conflict", Severity::High),
        // Medium
        ("profile/lto", Severity::Medium),
        ("profile/incremental", Severity::Medium),
        ("profile/missing-config", Severity::Medium),
        ("profile/dependency-opt-level", Severity::Medium),
        ("profile/debug-assertions", Severity::Medium),
        ("profile/overflow-checks", Severity::Medium),
        ("config/link-gc-sections", Severity::Medium),
        ("deps/duplicate-versions", Severity::Medium),
        ("deps/default-features", Severity::Medium),
        ("deps/feature-unification", Severity::Medium),
        ("deps/feature-trim", Severity::Medium),
        ("deps/likely-dev-only", Severity::Medium),
        ("lib/heavy-default-features", Severity::Medium),
        ("lib/no-feature-granularity", Severity::Medium),
        // Low
        ("profile/codegen-units", Severity::Low),
        ("profile/panic-abort", Severity::Low),
        ("profile/split-debuginfo", Severity::Low),
        ("deps/feature-provenance", Severity::Low),
        ("lib/non-optional-dependency", Severity::Low),
        ("nightly", Severity::Low),
        // Info
        ("advisory", Severity::Info),
        ("deps/build-only-note", Severity::Info),
    ];

    ASSIGNMENTS
}

/// The severity assigned to `id`, or [`None`] when no assignment matches.
///
/// When several assignments match, the one whose selector has the longest path
/// wins, so a specific assignment overrides a family. Two matching assignments
/// of equal path length that disagree yield [`None`], a case the table's own
/// test proves unreachable for every registered rule.
pub fn severity_of(id: &RuleId) -> Option<Severity> {
    let mut best: Option<(usize, Severity)> = None;

    for (text, severity) in severity_assignments() {
        let selector = Selector::parse(text).expect("every assignment is a well-formed selector");

        if !selector.matches(id) {
            continue;
        }

        let length = path_length(text);

        match best {
            None => best = Some((length, *severity)),
            Some((best_length, best_severity)) => {
                if length > best_length {
                    best = Some((length, *severity));
                } else if length == best_length && best_severity != *severity {
                    return None;
                }
            }
        }
    }

    best.map(|(_, severity)| severity)
}

/// The number of path segments an assignment selector names.
///
/// The path is the selector text before its dimension qualifier, so the count
/// is the number of non-empty segments that part splits into.
fn path_length(text: &str) -> usize {
    let path = text.split(':').next().unwrap_or(text);

    path.split('/')
        .filter(|segment| !segment.is_empty())
        .count()
}

/// The registered metadata for `id`, or [`None`] when no rule is registered
/// under it.
pub fn metadata_for(id: &RuleId) -> Option<&'static RuleMetadata> {
    registry().iter().find(|entry| entry.rule_id() == *id)
}

#[cfg(test)]
mod tests {
    use super::{registry, severity_assignments, severity_of};
    use crate::finding::{RuleId, Selector, Severity};
    use std::collections::BTreeSet;

    #[test]
    fn every_severity_assignment_is_a_well_formed_selector() {
        for (selector, _) in severity_assignments() {
            assert!(
                Selector::parse(selector).is_ok(),
                "`{selector}` must be a well-formed selector"
            );
        }
    }

    #[test]
    fn severity_assignments_are_unique() {
        let assignments = severity_assignments();
        let unique: BTreeSet<&str> = assignments.iter().map(|(text, _)| *text).collect();

        assert_eq!(unique.len(), assignments.len(), "no identifier may repeat");
    }

    #[test]
    fn severity_assignments_cover_the_whole_specification_table() {
        let assignments = severity_assignments();

        assert_eq!(assignments.len(), 30, "the table has thirty entries");

        let count = |severity: Severity| {
            assignments
                .iter()
                .filter(|(_, assigned)| *assigned == severity)
                .count()
        };

        assert_eq!(count(Severity::High), 8);
        assert_eq!(count(Severity::Medium), 14);
        assert_eq!(count(Severity::Low), 6);
        assert_eq!(count(Severity::Info), 2);

        for expected in [
            ("profile/opt-level:size", Severity::High),
            ("conflict", Severity::High),
            ("nightly", Severity::Low),
            ("advisory", Severity::Info),
            ("deps/build-only-note", Severity::Info),
        ] {
            assert!(
                assignments.contains(&expected),
                "the table must contain {expected:?}"
            );
        }
    }

    #[test]
    fn severity_lookup_resolves_families_and_prefers_the_specific_entry() {
        let severity = |text: &str| severity_of(&RuleId::parse(text).unwrap());

        assert_eq!(severity("profile/strip"), Some(Severity::High));
        assert_eq!(severity("profile/opt-level:size"), Some(Severity::High));
        assert_eq!(severity("nightly/build-std"), Some(Severity::Low));
        assert_eq!(severity("advisory/upx"), Some(Severity::Info));
        assert_eq!(severity("made-up/rule"), None);
    }

    #[test]
    fn no_rule_identifier_is_assigned_two_severities() {
        let assignments = severity_assignments();

        for (left_text, left_severity) in assignments {
            for (right_text, right_severity) in assignments {
                if left_text == right_text || left_severity == right_severity {
                    continue;
                }

                let right_selector = Selector::parse(right_text).unwrap();
                let left_selector = Selector::parse(left_text).unwrap();

                assert!(
                    !right_selector.matches(&RuleId::parse(left_text).unwrap()),
                    "`{left_text}` is matched by `{right_text}` with a different severity"
                );
                assert!(
                    !left_selector.matches(&RuleId::parse(right_text).unwrap()),
                    "`{right_text}` is matched by `{left_text}` with a different severity"
                );
            }
        }
    }

    #[test]
    fn every_registered_rule_parses_and_has_a_severity() {
        for entry in registry() {
            let id = RuleId::parse(entry.id)
                .unwrap_or_else(|error| panic!("`{}` does not parse: {error}", entry.id));

            assert!(
                severity_of(&id).is_some(),
                "`{}` has no severity assignment",
                entry.id
            );
        }
    }
}
