//! Curated advice about specific crates, embedded in the binary.
//!
//! The knowledge base records what is known about individual crates: how
//! costly a crate is to compile, whether a crate is almost always a
//! development-only dependency, and which of its default features can be
//! switched off. Entries are matched against the version a project resolved,
//! so advice written for one release line is never applied to another. An entry
//! is marked stale once the resolved version is newer than the release the
//! advice was last verified against, and a `zempic.toml` file may extend the
//! embedded document or replace the entry of a single crate.
//!
//! # Entry format
//!
//! The document is TOML, versioned by a top-level `kb_version`. Each crate has
//! a `[crates.<name>]` table, and each of its features a
//! `[[crates.<name>.features]]` table:
//!
//! ```toml
//! kb_version = 3
//!
//! [crates.reqwest]
//! versions = ">=0.11.2, <0.13"
//! verified_through = "0.12.28"
//! magnitude = "large"
//!
//! [[crates.reqwest.features]]
//! name = "default-tls"
//! default = true
//! magnitude = "large"
//! advice = "Links native-tls; the rustls-tls feature is a pure-Rust alternative."
//! fixable = true
//! ```
//!
//! `versions` is a semver requirement and `verified_through` a version that
//! must satisfy it. `magnitude` on a crate is the cost of compiling the crate
//! itself, and `dev_only` marks a crate that is almost always used only by
//! tests, benchmarks or examples. A feature's `default` states that the feature
//! is in the crate's default set, and its `fixable` states that the advice is
//! safe to apply by editing the manifest; advice that is not fixable is still
//! reported, but never as an applicable fix.
//!
//! # Staleness
//!
//! Stale advice still produces findings, because it is usually still right,
//! but it is reported with [`stale_note`] and its fixability drops to
//! [`Fixability::ReportOnly`] through [`effective_fixability`].
//!
//! # Overrides
//!
//! [`KnowledgeBase::extend`] replaces an entry of the same crate name entirely.
//! Features are never merged between the replaced and the replacing entry, so
//! an override states everything that is to be known about the crate.

use std::collections::BTreeMap;

use crate::finding::{Fixability, Magnitude};

/// The `kb_version` the embedded document and every accepted document declare.
pub const KNOWLEDGE_BASE_VERSION: u32 = 3;

/// The document compiled into the binary.
const EMBEDDED: &str = include_str!("knowledge/crates.toml");

/// The origin reported for errors in the embedded document.
const EMBEDDED_ORIGIN: &str = "<embedded knowledge base>";

/// A set of curated crate entries keyed by crate name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeBase {
    crates: BTreeMap<String, CrateEntry>,
}

impl KnowledgeBase {
    /// Parse the knowledge base embedded in the binary.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::KnowledgeBaseParse`](crate::ZempicError::KnowledgeBaseParse)
    /// when the document is not valid TOML or carries an unknown key,
    /// [`ZempicError::KnowledgeBaseVersion`](crate::ZempicError::KnowledgeBaseVersion)
    /// when its `kb_version` is not [`KNOWLEDGE_BASE_VERSION`], and
    /// [`ZempicError::KnowledgeBaseEntry`](crate::ZempicError::KnowledgeBaseEntry)
    /// when an entry holds an invalid value.
    pub fn embedded() -> crate::Result<Self> {
        Self::parse(EMBEDDED_ORIGIN, EMBEDDED)
    }

    /// Parse `text` into a knowledge base, naming `origin` in any error.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::KnowledgeBaseParse`](crate::ZempicError::KnowledgeBaseParse)
    /// when `text` is not valid TOML or carries an unknown key,
    /// [`ZempicError::KnowledgeBaseVersion`](crate::ZempicError::KnowledgeBaseVersion)
    /// when its `kb_version` is not [`KNOWLEDGE_BASE_VERSION`], and
    /// [`ZempicError::KnowledgeBaseEntry`](crate::ZempicError::KnowledgeBaseEntry)
    /// when an entry holds an invalid value.
    pub fn parse(origin: &str, text: &str) -> crate::Result<Self> {
        let raw: RawKnowledgeBase = toml_edit::de::from_str(text).map_err(|source| {
            crate::ZempicError::KnowledgeBaseParse {
                origin: origin.to_owned(),
                source,
            }
        })?;

        if raw.kb_version != KNOWLEDGE_BASE_VERSION {
            return Err(crate::ZempicError::KnowledgeBaseVersion {
                found: raw.kb_version,
                expected: KNOWLEDGE_BASE_VERSION,
            });
        }

        let crates = parse_crates(origin, raw.crates)?;

        Ok(Self { crates })
    }

    /// Insert every override, replacing an existing entry of the same name entirely.
    pub fn extend(&mut self, overrides: BTreeMap<String, CrateEntry>) {
        self.crates.extend(overrides);
    }

    /// The entry named `name`, when its `versions` requirement matches `version`.
    pub fn entry_for(&self, name: &str, version: &semver::Version) -> Option<&CrateEntry> {
        self.crates
            .get(name)
            .filter(|entry| entry.versions.matches(version))
    }

    /// Every entry, keyed by crate name.
    pub fn crates(&self) -> &BTreeMap<String, CrateEntry> {
        &self.crates
    }
}

/// The curated knowledge about one crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateEntry {
    /// The resolved versions the entry applies to.
    pub versions: semver::VersionReq,
    /// The newest release the advice was verified against.
    pub verified_through: semver::Version,
    /// The cost of compiling the crate itself, when known.
    pub magnitude: Option<Magnitude>,
    /// Whether the crate is almost always used only by tests, benchmarks or examples.
    pub dev_only: bool,
    /// The crate's features the knowledge base advises on.
    pub features: Vec<FeatureEntry>,
}

impl CrateEntry {
    /// Whether `resolved` is newer than the release the advice was verified against.
    pub fn is_stale(&self, resolved: &semver::Version) -> bool {
        resolved > &self.verified_through
    }

    /// Whether compiling the crate itself is moderately or highly costly.
    pub fn is_heavy(&self) -> bool {
        matches!(self.magnitude, Some(Magnitude::Moderate | Magnitude::Large))
    }
}

/// Advice about one feature of a crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeatureEntry {
    /// The feature's name.
    pub name: String,
    /// Whether the feature is in the crate's default set.
    pub default: bool,
    /// The cost the feature adds to a build.
    pub magnitude: Magnitude,
    /// What enabling the feature costs and what to do instead.
    pub advice: String,
    /// Whether the advice is safe to apply by editing the manifest.
    pub fixable: bool,
}

/// The fixability a finding from a knowledge-base entry carries.
///
/// Returns `declared` when the entry is `fixable` and not `stale`, and
/// [`Fixability::ReportOnly`] otherwise.
pub fn effective_fixability(declared: Fixability, fixable: bool, stale: bool) -> Fixability {
    if fixable && !stale {
        return declared;
    }

    Fixability::ReportOnly
}

/// The line reported beside advice that has outlived its verification.
pub fn stale_note(
    name: &str,
    verified_through: &semver::Version,
    resolved: &semver::Version,
) -> String {
    format!("Advice verified through {name} {verified_through}; you have {resolved}.")
}

/// The magnitude `text` names, or [`None`] when it names none.
///
/// This is the inverse of [`Magnitude::as_str`].
fn parse_magnitude(text: &str) -> Option<Magnitude> {
    match text {
        "none" => Some(Magnitude::None),
        "negligible" => Some(Magnitude::Negligible),
        "small" => Some(Magnitude::Small),
        "moderate" => Some(Magnitude::Moderate),
        "large" => Some(Magnitude::Large),
        _ => None,
    }
}

/// The document as written, before its values are validated.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKnowledgeBase {
    kb_version: u32,
    #[serde(default)]
    crates: BTreeMap<String, RawCrateEntry>,
}

/// A crate entry as written, before its values are validated.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawCrateEntry {
    versions: String,
    verified_through: String,
    #[serde(default)]
    magnitude: Option<String>,
    #[serde(default)]
    dev_only: bool,
    #[serde(default)]
    features: Vec<RawFeatureEntry>,
}

/// A feature entry as written, before its values are validated.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFeatureEntry {
    name: String,
    #[serde(default)]
    default: bool,
    magnitude: String,
    advice: String,
    #[serde(default)]
    fixable: bool,
}

/// Validate every raw entry, naming the first invalid crate in name order.
///
/// # Errors
///
/// Returns [`ZempicError::KnowledgeBaseEntry`](crate::ZempicError::KnowledgeBaseEntry)
/// when an entry's `versions` is not a requirement, its `verified_through` is
/// not a version or does not satisfy `versions`, or a magnitude is unknown.
pub(crate) fn parse_crates(
    origin: &str,
    raw: BTreeMap<String, RawCrateEntry>,
) -> crate::Result<BTreeMap<String, CrateEntry>> {
    let mut crates = BTreeMap::new();

    for (name, entry) in raw {
        let invalid = |detail: String| crate::ZempicError::KnowledgeBaseEntry {
            origin: origin.to_owned(),
            name: name.clone(),
            detail,
        };

        let versions = semver::VersionReq::parse(&entry.versions).map_err(|error| {
            invalid(format!("`versions` is not a version requirement: {error}"))
        })?;

        let verified_through = semver::Version::parse(&entry.verified_through)
            .map_err(|error| invalid(format!("`verified_through` is not a version: {error}")))?;

        if !versions.matches(&verified_through) {
            return Err(invalid(format!(
                "`verified_through` {verified_through} does not satisfy `versions` {versions}"
            )));
        }

        let magnitude = match &entry.magnitude {
            Some(text) => Some(
                parse_magnitude(text)
                    .ok_or_else(|| invalid(format!("unknown magnitude `{text}`")))?,
            ),
            None => None,
        };

        let mut features = Vec::new();

        for feature in entry.features {
            let feature_magnitude = parse_magnitude(&feature.magnitude)
                .ok_or_else(|| invalid(format!("unknown magnitude `{}`", feature.magnitude)))?;

            features.push(FeatureEntry {
                name: feature.name,
                default: feature.default,
                magnitude: feature_magnitude,
                advice: feature.advice,
                fixable: feature.fixable,
            });
        }

        let parsed = CrateEntry {
            versions,
            verified_through,
            magnitude,
            dev_only: entry.dev_only,
            features,
        };

        crates.insert(name.clone(), parsed);
    }

    Ok(crates)
}

#[cfg(test)]
mod tests {
    use semver::Version;

    use super::*;
    use crate::ZempicError;

    const DEMO: &str = r#"
kb_version = 3

[crates.demo]
versions = ">=1.0, <2"
verified_through = "1.4.0"
magnitude = "large"

[[crates.demo.features]]
name = "extras"
default = true
magnitude = "moderate"
advice = "Disable it."
fixable = true
"#;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn demo() -> KnowledgeBase {
        KnowledgeBase::parse("demo.toml", DEMO).unwrap()
    }

    #[test]
    fn entry_applies_only_within_its_version_range() {
        let base = demo();

        assert!(base.entry_for("demo", &v("1.0.0")).is_some());
        assert!(base.entry_for("demo", &v("1.9.9")).is_some());
        assert!(base.entry_for("demo", &v("0.9.9")).is_none());
        assert!(base.entry_for("demo", &v("2.0.0")).is_none());
        assert!(base.entry_for("other", &v("1.0.0")).is_none());
    }

    #[test]
    fn entry_is_stale_beyond_verified_through() {
        let base = demo();
        let entry = &base.crates()["demo"];

        assert!(!entry.is_stale(&v("1.3.9")));
        assert!(!entry.is_stale(&v("1.4.0")));
        assert!(entry.is_stale(&v("1.4.1")));
        assert!(entry.is_heavy());
    }

    #[test]
    fn stale_or_unfixable_advice_is_report_only() {
        assert_eq!(
            effective_fixability(Fixability::Consequential, true, false),
            Fixability::Consequential
        );
        assert_eq!(
            effective_fixability(Fixability::Consequential, true, true),
            Fixability::ReportOnly
        );
        assert_eq!(
            effective_fixability(Fixability::Consequential, false, false),
            Fixability::ReportOnly
        );
        assert_eq!(
            stale_note("reqwest", &v("0.12.9"), &v("0.12.14")),
            "Advice verified through reqwest 0.12.9; you have 0.12.14."
        );
    }

    #[test]
    fn override_replaces_an_embedded_entry_entirely() {
        let mut base = demo();
        let override_base = KnowledgeBase::parse(
            "override.toml",
            "kb_version = 3\n\n[crates.demo]\nversions = \">=2.0, <3\"\nverified_through = \"2.0.0\"\n",
        )
        .unwrap();

        base.extend(override_base.crates().clone());

        assert!(base.entry_for("demo", &v("1.4.0")).is_none());
        assert!(
            base.entry_for("demo", &v("2.0.0"))
                .unwrap()
                .features
                .is_empty()
        );
        assert_eq!(base.crates()["demo"].magnitude, None);
    }

    #[test]
    fn parse_rejects_invalid_entries() {
        let with = |replacement: &str, original: &str| DEMO.replace(original, replacement);

        let error = KnowledgeBase::parse(
            "demo.toml",
            &with("versions = \"not a range\"", "versions = \">=1.0, <2\""),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ZempicError::KnowledgeBaseEntry { ref name, .. } if name == "demo"
        ));

        let error = KnowledgeBase::parse(
            "demo.toml",
            &with(
                "verified_through = \"2.1.0\"",
                "verified_through = \"1.4.0\"",
            ),
        )
        .unwrap_err();
        assert!(matches!(error, ZempicError::KnowledgeBaseEntry { .. }));
        assert!(error.to_string().contains("does not satisfy"), "{error}");

        let error = KnowledgeBase::parse(
            "demo.toml",
            &with("verified_through = \"1.0\"", "verified_through = \"1.4.0\""),
        )
        .unwrap_err();
        assert!(matches!(error, ZempicError::KnowledgeBaseEntry { .. }));

        let error = KnowledgeBase::parse(
            "demo.toml",
            &with("magnitude = \"huge\"", "magnitude = \"moderate\""),
        )
        .unwrap_err();
        assert!(matches!(error, ZempicError::KnowledgeBaseEntry { .. }));
        assert!(error.to_string().contains("huge"), "{error}");

        let error = KnowledgeBase::parse("demo.toml", &with("kb_version = 2", "kb_version = 3"))
            .unwrap_err();
        assert!(matches!(
            error,
            ZempicError::KnowledgeBaseVersion {
                found: 2,
                expected: 3
            }
        ));

        let error = KnowledgeBase::parse(
            "demo.toml",
            &with(
                "magnitude = \"large\"\nreplacement = \"other\"",
                "magnitude = \"large\"",
            ),
        )
        .unwrap_err();
        assert!(matches!(error, ZempicError::KnowledgeBaseParse { .. }));
    }
}
