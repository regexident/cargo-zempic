//! Snapshot tests of the three renderers over firing fixtures.
//!
//! A snapshot is taken over a materialized fixture through each renderer. The
//! workspace root is normalized to `[WORKSPACE_ROOT]` so a snapshot does not
//! embed a temporary directory path, and the target triple is pinned where the
//! rendered content depends on it.

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
    };

    let report = analyze(&config).expect("the analysis succeeds");
    let document = match format {
        Format::Human => cargo_zempic::report::human::render(&report, false, 80),
        Format::Json => cargo_zempic::report::json::render(&report).expect("the JSON renders"),
        Format::Markdown => cargo_zempic::report::markdown::render(&report),
    };

    document.replace(workspace.root().as_str(), "[WORKSPACE_ROOT]")
}

/// Assert a snapshot under the settings that normalize the workspace root.
fn assert_snapshot(name: &str, document: String) {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("snapshots");
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
