//! Snapshot tests of the three renderers over firing fixtures.
//!
//! A snapshot is taken over a materialized fixture through each renderer. The
//! workspace root, the resolved target triple and the package version are
//! normalized, so a snapshot depends on the fixture and not on the machine or
//! the build that produced it.

mod common;

use cargo_zempic::report::Format;
use cargo_zempic::workspace::{FeatureSelection, PackageSelection};
use cargo_zempic::{AnalysisConfig, analyze};

/// Materialize `fixture`, analyze it, and render the report in `format`.
///
/// The analysis uses the default configuration, so a snapshot varies only with
/// the fixture. Output is uncolored and, for the human renderer, fixed at 80
/// columns.
fn rendered(fixture: &str, format: Format) -> String {
    rendered_for(fixture, format, None)
}

/// Render `fixture` through `format`, optionally pinning the target triple.
fn rendered_for(fixture: &str, format: Format, target: Option<&str>) -> String {
    let workspace = common::Fixture::new(fixture).materialize();

    let config = AnalysisConfig {
        dimensions: Vec::new(),
        manifest_path: Some(workspace.manifest_path()),
        selection: PackageSelection::default(),
        profile: None,
        features: FeatureSelection::default(),
        target: target.map(str::to_owned),
        format: Some(format),
        fail_on: None,
        quiet: false,
        gates: cargo_zempic::config::Gates::default(),
        today: cargo_zempic::config::CivilDate::today(),
        environment: cargo_zempic::cargo_config::CargoEnvironment::sealed(workspace.root()),
        toolchain: Some(cargo_zempic::toolchain::ToolchainChannel::Stable),
    };

    let report = analyze(&config).expect("the analysis succeeds");
    let document = match format {
        Format::Human => cargo_zempic::report::human::render(&report, false, 80),
        Format::Json => cargo_zempic::report::json::render(&report).expect("the JSON renders"),
        Format::Markdown => cargo_zempic::report::markdown::render(&report),
    };

    document.replace(workspace.root().as_str(), "[WORKSPACE_ROOT]")
}

/// Assert a snapshot under the settings that normalize the workspace root, the
/// target triple and the package version.
///
/// Every format prints the version the binary was built at, so a version bump
/// would otherwise rewrite every snapshot. The redaction applies to all three
/// renderings of the version: the human and markdown header, and the
/// `zempic_version` field of the JSON document.
///
/// The target triple is the host's unless a fixture pins one, so leaving it
/// whole would make a snapshot depend on the machine that produced it. Each
/// format states it once, in its own header, and no finding quotes it.
fn assert_snapshot(name: &str, document: String) {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("snapshots");
    settings.add_filter(
        r"cargo-zempic \d+\.\d+\.\d+(?:[-+][0-9A-Za-z.+-]+)?",
        "cargo-zempic [VERSION]",
    );
    settings.add_filter(
        r#""zempic_version": "\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.+-]+)?""#,
        r#""zempic_version": "[VERSION]""#,
    );
    settings.add_filter(r"(?m)^(  target:         )\S+$", "${1}[TARGET]");
    settings.add_filter(r"(- Target: `)[^`]+(`)", "${1}[TARGET]${2}");
    settings.bind(|| {
        insta::assert_snapshot!(name, document);
    });
}

#[test]
fn fully_optimized_human() {
    assert_snapshot(
        "fully_optimized_human",
        rendered("fully-optimized", Format::Human),
    );
}

#[test]
fn fully_optimized_json() {
    assert_snapshot(
        "fully_optimized_json",
        rendered("fully-optimized", Format::Json),
    );
}

#[test]
fn fully_optimized_markdown() {
    assert_snapshot(
        "fully_optimized_markdown",
        rendered("fully-optimized", Format::Markdown),
    );
}

#[test]
fn strip_none_human() {
    assert_snapshot("strip_none_human", rendered("strip-none", Format::Human));
}

#[test]
fn debug_info_enabled_human() {
    assert_snapshot(
        "debug_info_enabled_human",
        rendered("debug-info-enabled", Format::Human),
    );
}

#[test]
fn split_debuginfo_off_human() {
    // The rule fires only on an Apple target, so the triple is pinned rather
    // than taken from the host, which would differ between developers.
    assert_snapshot(
        "split_debuginfo_off_human",
        rendered_for(
            "split-debuginfo-off",
            Format::Human,
            Some("aarch64-apple-darwin"),
        ),
    );
}

#[test]
fn opt_level_speed_human() {
    assert_snapshot(
        "opt_level_speed_human",
        rendered("opt-level-speed", Format::Human),
    );
}

#[test]
fn lto_thin_human() {
    assert_snapshot("lto_thin_human", rendered("lto-thin", Format::Human));
}

#[test]
fn codegen_units_many_human() {
    assert_snapshot(
        "codegen_units_many_human",
        rendered("codegen-units-many", Format::Human),
    );
}

#[test]
fn incremental_enabled_human() {
    assert_snapshot(
        "incremental_enabled_human",
        rendered("incremental-enabled", Format::Human),
    );
}

#[test]
fn dependency_opt_level_human() {
    assert_snapshot(
        "dependency_opt_level_human",
        rendered("dependency-opt-level", Format::Human),
    );
}

#[test]
fn panic_unwind_human() {
    assert_snapshot(
        "panic_unwind_human",
        rendered("panic-unwind", Format::Human),
    );
}

#[test]
fn debug_assertions_enabled_human() {
    assert_snapshot(
        "debug_assertions_enabled_human",
        rendered("debug-assertions-enabled", Format::Human),
    );
}

#[test]
fn overflow_checks_enabled_human() {
    assert_snapshot(
        "overflow_checks_enabled_human",
        rendered("overflow-checks-enabled", Format::Human),
    );
}

#[test]
fn member_profile_table_human() {
    assert_snapshot(
        "member_profile_table_human",
        rendered("member-profile-table", Format::Human),
    );
}

#[test]
fn bare_release_human() {
    assert_snapshot(
        "bare_release_human",
        rendered("bare-release", Format::Human),
    );
}

#[test]
fn bare_release_json() {
    assert_snapshot("bare_release_json", rendered("bare-release", Format::Json));
}

#[test]
fn bare_release_markdown() {
    assert_snapshot(
        "bare_release_markdown",
        rendered("bare-release", Format::Markdown),
    );
}

#[test]
fn link_dead_code_human() {
    assert_snapshot(
        "link_dead_code_human",
        rendered_for(
            "link-dead-code",
            Format::Human,
            Some("x86_64-unknown-linux-gnu"),
        ),
    );
}

#[test]
fn rustflags_shadowed_human() {
    assert_snapshot(
        "rustflags_shadowed_human",
        rendered_for(
            "rustflags-shadowed",
            Format::Human,
            Some("x86_64-unknown-linux-gnu"),
        ),
    );
}

#[test]
fn snapshots_carry_no_absolute_path() {
    let repository_root = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let snapshots = repository_root.join("tests/snapshots");
    let entries = std::fs::read_dir(&snapshots).expect("the snapshot directory exists");

    for entry in entries.flatten() {
        let path = camino::Utf8PathBuf::from_path_buf(entry.path()).expect("paths are UTF-8");
        let text = std::fs::read_to_string(&path).expect("the snapshot is readable");

        for forbidden in ["/tmp/", "/var/folders/"] {
            assert!(
                !text.contains(forbidden),
                "snapshot `{path}` embeds `{forbidden}`"
            );
        }

        assert!(
            !text.contains(repository_root.as_str()),
            "snapshot `{path}` embeds the repository root"
        );
    }
}

#[test]
fn rule_reference_matches_registry_documentation() {
    let path = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/rules.md");
    let checked_in = std::fs::read_to_string(&path).expect("the rule reference is readable");
    let generated = cargo_zempic::rules::reference();

    assert_eq!(
        checked_in, generated,
        "`{path}` differs from the reference generated from the rule documentation"
    );
}
