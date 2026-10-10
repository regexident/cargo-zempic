use cargo_zempic::dimension::Dimension;
use cargo_zempic::manifest::{KeySpan, ManifestDocument};
use cargo_zempic::workspace::{
    ArtifactKind, FeatureSelection, MetadataQuery, PackageSelection, classify_package,
    host_target_triple, load_metadata, select_packages,
};
use cargo_zempic::{AnalysisConfig, Resolution, ZempicError, resolve};

mod common;

const DEMO_MANIFEST: &str =
    "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[profile.release]\nlto = true\n";

#[test]
fn manifest_key_span_reports_one_based_line_and_column() {
    let directory = tempfile::TempDir::new().unwrap();
    let manifest_path = directory.path().join("Cargo.toml");
    std::fs::write(&manifest_path, DEMO_MANIFEST).unwrap();

    let manifest_path = camino::Utf8PathBuf::from_path_buf(manifest_path).unwrap();
    let document = ManifestDocument::load(&manifest_path).unwrap();

    assert_eq!(
        document.key_span("profile.release.lto"),
        Some(KeySpan {
            file: manifest_path,
            key: "profile.release.lto".to_owned(),
            line: 6,
            column: 1,
        })
    );
}

#[test]
fn manifest_key_span_is_none_for_absent_key() {
    let manifest_path = camino::Utf8PathBuf::from("Cargo.toml");
    let document = ManifestDocument::parse(&manifest_path, DEMO_MANIFEST).unwrap();

    assert_eq!(document.key_span("profile.release.codegen-units"), None);
    assert_eq!(document.key_span("profile.missing.lto"), None);
}

#[test]
fn manifest_parse_error_names_the_file() {
    let manifest_path = camino::Utf8PathBuf::from("broken/Cargo.toml");
    let error = ManifestDocument::parse(&manifest_path, "not valid toml =").unwrap_err();

    let is_manifest_parse = matches!(error, ZempicError::ManifestParse { .. });
    assert!(is_manifest_parse);
    assert!(error.to_string().contains("broken/Cargo.toml"));
}

#[test]
fn materialized_fixture_completes_member_sources() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let root = workspace.root();

    for relative in [
        "Cargo.toml",
        "alpha/Cargo.toml",
        "alpha/src/main.rs",
        "beta/Cargo.toml",
        "beta/src/lib.rs",
    ] {
        assert!(root.join(relative).exists(), "{relative} should exist");
    }

    let main = std::fs::read_to_string(root.join("alpha/src/main.rs")).unwrap();
    let library = std::fs::read_to_string(root.join("beta/src/lib.rs")).unwrap();

    assert_eq!(main, "fn main() {}\n");
    assert!(library.is_empty());
}

#[test]
fn materialization_preserves_provided_files() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let provided = std::fs::read("tests/fixtures/multi-member/alpha/Cargo.toml").unwrap();
    let materialized = std::fs::read(workspace.root().join("alpha/Cargo.toml")).unwrap();

    assert_eq!(materialized, provided);
}

#[test]
fn materialization_creates_absent_member_manifests() {
    let directory = tempfile::TempDir::new().unwrap();
    let root = camino::Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();

    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"gamma\"]\n",
    )
    .unwrap();

    common::complete_workspace(&root);

    let manifest = std::fs::read_to_string(root.join("gamma/Cargo.toml")).unwrap();

    assert!(manifest.contains("name = \"gamma\""), "{manifest}");
    assert!(manifest.contains("version = \"0.1.0\""), "{manifest}");
    assert!(manifest.contains("edition = \"2024\""), "{manifest}");
    assert!(root.join("gamma/src/main.rs").exists());
    assert!(root.join("gamma/src/lib.rs").exists());
}

#[test]
fn materialized_roots_are_distinct_between_calls() {
    let first = common::Fixture::new("multi-member").materialize();
    let second = common::Fixture::new("multi-member").materialize();

    assert_ne!(first.root(), second.root());
}

#[test]
fn host_target_triple_matches_rustc() {
    let output = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap();

    assert_eq!(host_target_triple().unwrap(), expected);
}

/// Loads metadata for the `default-members` fixture, whose resolved default
/// member list is `["alpha"]` while its full member list is `["alpha", "beta"]`.
fn default_members_metadata() -> (common::MaterializedWorkspace, cargo_metadata::Metadata) {
    let workspace = common::Fixture::new("default-members").materialize();

    let query = MetadataQuery {
        manifest_path: Some(workspace.manifest_path()),
        ..MetadataQuery::default()
    };

    let metadata = load_metadata(&query).unwrap();

    (workspace, metadata)
}

/// Collects the names of selected packages in the order they were returned.
fn selected_names(packages: &[&cargo_metadata::Package]) -> Vec<String> {
    packages
        .iter()
        .map(|package| package.name.to_string())
        .collect()
}

#[test]
fn default_selection_uses_workspace_default_members() {
    let (_workspace, metadata) = default_members_metadata();

    let selected =
        select_packages(&metadata, &PackageSelection::default()).expect("selection succeeds");

    assert_eq!(selected_names(&selected), ["alpha"]);
}

#[test]
fn workspace_flag_selects_every_member() {
    let (_workspace, metadata) = default_members_metadata();

    let selection = PackageSelection {
        workspace: true,
        ..PackageSelection::default()
    };

    let selected = select_packages(&metadata, &selection).expect("selection succeeds");

    assert_eq!(selected_names(&selected), ["alpha", "beta"]);
}

#[test]
fn package_flag_selects_named_members() {
    let (_workspace, metadata) = default_members_metadata();

    let selection = PackageSelection {
        packages: vec!["beta".to_owned()],
        ..PackageSelection::default()
    };

    let selected = select_packages(&metadata, &selection).expect("selection succeeds");

    assert_eq!(selected_names(&selected), ["beta"]);
}

#[test]
fn exclude_removes_members_under_workspace() {
    let (_workspace, metadata) = default_members_metadata();

    let selection = PackageSelection {
        workspace: true,
        exclude: vec!["beta".to_owned()],
        ..PackageSelection::default()
    };

    let selected = select_packages(&metadata, &selection).expect("selection succeeds");

    assert_eq!(selected_names(&selected), ["alpha"]);
}

#[test]
fn exclude_without_workspace_is_an_error() {
    let (_workspace, metadata) = default_members_metadata();

    let selection = PackageSelection {
        exclude: vec!["beta".to_owned()],
        ..PackageSelection::default()
    };

    let error = select_packages(&metadata, &selection).unwrap_err();

    assert!(
        matches!(error, ZempicError::ExcludeWithoutWorkspace),
        "{error}"
    );
}

#[test]
fn unknown_package_spec_is_an_error() {
    let (_workspace, metadata) = default_members_metadata();

    let selection = PackageSelection {
        packages: vec!["gamma".to_owned()],
        ..PackageSelection::default()
    };

    let error = select_packages(&metadata, &selection).unwrap_err();

    match error {
        ZempicError::UnknownPackage { spec } => assert_eq!(spec, "gamma"),
        other => panic!("expected an unknown-package error, got: {other}"),
    }
}

#[test]
fn unknown_package_spec_is_an_error_even_under_workspace() {
    let (_workspace, metadata) = default_members_metadata();

    let selection = PackageSelection {
        workspace: true,
        packages: vec!["gamma".to_owned()],
        ..PackageSelection::default()
    };

    let error = select_packages(&metadata, &selection).unwrap_err();

    match error {
        ZempicError::UnknownPackage { spec } => assert_eq!(spec, "gamma"),
        other => panic!("expected an unknown-package error, got: {other}"),
    }
}

#[test]
fn selection_is_ordered_by_name() {
    let (_workspace, metadata) = default_members_metadata();

    let selection = PackageSelection {
        workspace: true,
        ..PackageSelection::default()
    };

    let selected = select_packages(&metadata, &selection).expect("selection succeeds");

    assert_eq!(selected_names(&selected), ["alpha", "beta"]);
    assert!(selected[0].name < selected[1].name);
}

/// Loads metadata for the `crate-types` fixture, whose members cover each
/// target shape a `[lib]` or `[[bin]]` table can produce.
fn crate_types_metadata() -> (common::MaterializedWorkspace, cargo_metadata::Metadata) {
    let workspace = common::Fixture::new("crate-types").materialize();

    let query = MetadataQuery {
        manifest_path: Some(workspace.manifest_path()),
        ..MetadataQuery::default()
    };

    let metadata = load_metadata(&query).unwrap();

    (workspace, metadata)
}

/// Finds the named package in `metadata`'s resolved package list.
fn package<'a>(metadata: &'a cargo_metadata::Metadata, name: &str) -> &'a cargo_metadata::Package {
    metadata
        .packages
        .iter()
        .find(|package| package.name.as_ref() == name)
        .unwrap_or_else(|| panic!("package `{name}` is missing from the metadata"))
}

#[test]
fn bin_package_produces_a_linked_artifact() {
    let (_workspace, metadata) = crate_types_metadata();

    let classification = classify_package(package(&metadata, "exe"));

    assert!(classification.produces_linked_artifact());

    let linked: Vec<&str> = classification
        .linked_targets()
        .map(|target| target.name.as_str())
        .collect();

    assert_eq!(linked, ["exe"]);
}

#[test]
fn lib_only_package_produces_no_linked_artifact() {
    let (_workspace, metadata) = crate_types_metadata();

    let classification = classify_package(package(&metadata, "plain-lib"));

    assert!(!classification.produces_linked_artifact());
    assert_eq!(classification.targets.len(), 1);
    assert_eq!(classification.targets[0].artifact, ArtifactKind::Unlinked);
}

#[test]
fn cdylib_package_produces_a_linked_artifact() {
    let (_workspace, metadata) = crate_types_metadata();

    let classification = classify_package(package(&metadata, "shared"));

    assert!(classification.produces_linked_artifact());
}

#[test]
fn proc_macro_package_produces_no_linked_artifact() {
    let (_workspace, metadata) = crate_types_metadata();

    let classification = classify_package(package(&metadata, "macros"));

    assert!(!classification.produces_linked_artifact());
}

#[test]
fn mixed_package_is_linked_and_names_its_bin_target() {
    let (_workspace, metadata) = crate_types_metadata();

    let classification = classify_package(package(&metadata, "mixed"));

    assert!(classification.produces_linked_artifact());

    let linked: Vec<&str> = classification
        .linked_targets()
        .map(|target| target.name.as_str())
        .collect();

    assert_eq!(linked, ["mixed"]);
}

#[test]
fn test_and_build_targets_are_omitted_from_classification() {
    let (_workspace, metadata) = crate_types_metadata();

    let source = package(&metadata, "with-aux");

    // The fixture package declares enough targets that cargo reports a
    // `custom-build` target for its build script and one `test`, `bench` and
    // `example` target. Without them the omission rule would be untested.
    let auxiliaries: Vec<&str> = source
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(is_auxiliary_kind))
        .map(|target| target.name.as_str())
        .collect();

    assert_eq!(
        auxiliaries.len(),
        4,
        "fixture must expose one target of each auxiliary kind: {auxiliaries:?}"
    );

    let classification = classify_package(source);
    let retained: Vec<&str> = classification
        .targets
        .iter()
        .map(|target| target.name.as_str())
        .collect();

    // The auxiliary targets are omitted while the package's linked and
    // unlinked targets survive, which fails if the omission rule reports
    // an auxiliary kind as classifiable.
    assert_eq!(retained, ["with_aux", "with-aux"]);

    for name in auxiliaries {
        assert!(
            !classification
                .targets
                .iter()
                .any(|target| target.name == name),
            "`{name}` should not survive classification: {retained:?}"
        );
    }

    assert!(
        !classification
            .targets
            .iter()
            .any(|target| target.kinds.iter().any(is_auxiliary_kind)),
        "a retained target must not carry an auxiliary kind"
    );
}

/// Whether a kind marks a target the analyzed profile never governs.
///
/// Mirrors the classifier's own predicate so the fixture assertion fails when
/// the production rule stops recognizing an auxiliary kind.
fn is_auxiliary_kind(kind: &cargo_metadata::TargetKind) -> bool {
    matches!(
        kind,
        cargo_metadata::TargetKind::Test
            | cargo_metadata::TargetKind::Bench
            | cargo_metadata::TargetKind::Example
            | cargo_metadata::TargetKind::CustomBuild
    )
}

/// The analysis configuration a test resolves a workspace with.
///
/// Names the fixture's root manifest so resolution never picks up the crate's
/// own workspace, and defaults every other field so a test states only the
/// choice it exercises.
fn analysis_config(workspace: &common::MaterializedWorkspace) -> AnalysisConfig {
    AnalysisConfig {
        dimensions: Vec::new(),
        manifest_path: Some(workspace.manifest_path()),
        selection: PackageSelection::default(),
        profile: "release".to_owned(),
        features: FeatureSelection::default(),
        target: None,
        format: None,
        fail_on: None,
        quiet: false,
        gates: cargo_zempic::config::Gates::default(),
        today: cargo_zempic::config::CivilDate::today(),
    }
}

/// An analysis configuration for a workspace, with explicit gates and date.
fn analyze_config(workspace: &common::MaterializedWorkspace, only: &[&str]) -> AnalysisConfig {
    AnalysisConfig {
        dimensions: vec![Dimension::Size],
        manifest_path: Some(workspace.manifest_path()),
        selection: PackageSelection::default(),
        profile: "release".to_owned(),
        features: FeatureSelection::default(),
        target: None,
        format: None,
        fail_on: None,
        quiet: false,
        gates: cargo_zempic::config::Gates {
            ignore: Vec::new(),
            only: only
                .iter()
                .map(|text| cargo_zempic::finding::Selector::parse(text).unwrap())
                .collect(),
            include_advisory: true,
        },
        today: cargo_zempic::config::CivilDate::parse("2026-10-04").unwrap(),
    }
}

/// Canonicalizes `path` so it can be compared with a path cargo reports, which
/// resolves symlinks such as `/var` to `/private/var`.
fn canonical(path: &camino::Utf8Path) -> camino::Utf8PathBuf {
    let resolved = std::fs::canonicalize(path).expect("path exists");

    camino::Utf8PathBuf::from_path_buf(resolved).expect("path is not UTF-8")
}

#[test]
fn resolution_reports_the_workspace_root_and_member_count() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let resolution = resolve(&analysis_config(&workspace)).expect("resolution succeeds");

    assert_eq!(
        canonical(&resolution.workspace_root),
        canonical(workspace.root())
    );
    assert_eq!(resolution.member_count, 2);
    assert_eq!(resolution.selected.len(), 2);
}

/// The names of `resolution`'s selected packages whose classification reports a
/// profile-governed artifact, in the order the selection returned them.
fn linked_selection_names(resolution: &Resolution) -> Vec<&str> {
    resolution
        .selected
        .iter()
        .filter(|package| package.classification.produces_linked_artifact())
        .map(|package| package.name.as_str())
        .collect()
}

/// The names of `resolution`'s selected packages whose classification reports no
/// profile-governed artifact, in the order the selection returned them.
fn unlinked_selection_names(resolution: &Resolution) -> Vec<&str> {
    resolution
        .selected
        .iter()
        .filter(|package| !package.classification.produces_linked_artifact())
        .map(|package| package.name.as_str())
        .collect()
}

#[test]
fn resolution_reports_the_requested_target_triple() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let requested = "aarch64-unknown-linux-gnu".to_owned();
    let mut config = analysis_config(&workspace);
    config.target = Some(requested.clone());

    let resolution = resolve(&config).expect("resolution succeeds");

    // An explicit non-host `config.target` must surface as the resolved triple
    // rather than being replaced by the host's, which would misreport the
    // platform the analysis runs against.
    assert_eq!(resolution.target_triple, requested);
}

#[test]
fn resolution_classifies_each_selected_package() {
    let workspace = common::Fixture::new("crate-types").materialize();
    let mut config = analysis_config(&workspace);
    config.selection.workspace = true;

    let resolution = resolve(&config).expect("resolution succeeds");

    // Every workspace member is selected, and each selected package carries the
    // classification of its own targets rather than being lumped together.
    let names: Vec<&str> = resolution
        .selected
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["exe", "macros", "mixed", "plain-lib", "shared", "with-aux"]
    );

    // The fixture's `exe` and `shared` members emit artifacts the analyzed
    // profile governs while `plain-lib` and `macros` do not, which fails if a
    // selected package is misclassified or dropped from the result.
    let linked = linked_selection_names(&resolution);
    let unlinked = unlinked_selection_names(&resolution);

    assert!(linked.contains(&"exe"), "linked: {linked:?}");
    assert!(linked.contains(&"shared"), "linked: {linked:?}");
    assert!(!linked.contains(&"plain-lib"), "linked: {linked:?}");
    assert!(!linked.contains(&"macros"), "linked: {linked:?}");
    assert!(unlinked.contains(&"plain-lib"), "unlinked: {unlinked:?}");
    assert!(unlinked.contains(&"macros"), "unlinked: {unlinked:?}");

    for name in ["exe", "plain-lib", "macros", "shared"] {
        let package = resolution
            .selected
            .iter()
            .find(|package| package.name == name)
            .unwrap_or_else(|| panic!("`{name}` should be selected: {names:?}"));

        assert_eq!(package.classification.name, name);
    }
}

#[test]
fn resolution_reads_the_profile_from_the_workspace_root() {
    let workspace = common::Fixture::new("multi-member").materialize();

    // The workspace root declares the only profile table in the fixture, so a
    // resolution that surfaced it read the root manifest.
    let root_manifest = concat!(
        "[workspace]\n",
        "members = [\"alpha\", \"beta\"]\n\n",
        "[profile.release]\n",
        "lto = \"fat\"\n",
    );
    std::fs::write(workspace.root().join("Cargo.toml"), root_manifest).unwrap();

    let resolution = resolve(&analysis_config(&workspace)).expect("resolution succeeds");

    assert_eq!(resolution.profile.name, "release");
    assert!(resolution.profile.lto.is_explicit());
    assert_eq!(
        resolution.profile.lto.value,
        cargo_zempic::profile::Lto::Fat
    );
}

#[test]
fn resolution_ignores_a_member_profile_table() {
    let workspace = common::Fixture::new("multi-member").materialize();

    // A member's profile table is inert to cargo, so resolution must report the
    // built-in default rather than the value the member declares.
    let root_manifest = "[workspace]\nmembers = [\"alpha\", \"beta\"]\n";
    std::fs::write(workspace.root().join("Cargo.toml"), root_manifest).unwrap();

    let member_manifest = concat!(
        "[package]\n",
        "name = \"alpha\"\n",
        "version = \"0.1.0\"\n",
        "edition = \"2021\"\n\n",
        "[[bin]]\n",
        "name = \"alpha\"\n",
        "path = \"src/main.rs\"\n\n",
        "[profile.release]\n",
        "lto = \"fat\"\n",
    );
    std::fs::write(workspace.root().join("alpha/Cargo.toml"), member_manifest).unwrap();

    let resolution = resolve(&analysis_config(&workspace)).expect("resolution succeeds");

    assert_eq!(resolution.profile.name, "release");
    assert!(resolution.profile.lto.is_cargo_default());
    assert_eq!(
        resolution.profile.lto.value,
        cargo_zempic::profile::Lto::Off
    );
}

#[test]
fn resolution_defaults_to_the_size_dimension() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let resolution = resolve(&analysis_config(&workspace)).expect("resolution succeeds");

    assert_eq!(resolution.dimensions, [Dimension::Size]);
}

#[test]
fn resolution_defaults_to_the_release_profile_when_requested() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let mut config = analysis_config(&workspace);
    config.profile = "release".to_owned();

    let resolution = resolve(&config).expect("resolution succeeds");

    assert_eq!(resolution.profile.name, "release");
    assert_eq!(resolution.member_count, 2);
}

#[test]
fn resolution_fails_on_an_unparseable_root_manifest() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let root_manifest = workspace.root().join("Cargo.toml");
    std::fs::write(&root_manifest, "not valid toml =").unwrap();

    let error = resolve(&analysis_config(&workspace)).unwrap_err();

    // `cargo metadata` and the manifest reader both name the offending file, so
    // the assertion holds whichever rejects the broken manifest.
    assert!(
        error.to_string().contains("Cargo.toml"),
        "error must name the offending manifest: {error}"
    );
}

/// The `Resolution` type carries the analyzed workspace's root so a caller can
/// report where the analysis ran.
#[allow(dead_code)]
fn resolution_root_of(resolution: &Resolution) -> &camino::Utf8Path {
    &resolution.workspace_root
}

/// Every registered rule must have a documentation file.
#[test]
fn every_registered_rule_has_a_documentation_file() {
    let root = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docs = root.join(cargo_zempic::rules::DOCUMENTATION_DIRECTORY);

    for entry in cargo_zempic::rules::registry() {
        let expected = docs.join(cargo_zempic::rules::documentation_path(&entry.rule_id()));

        assert!(
            expected.is_file(),
            "rule `{}` has no documentation file at `{}`",
            entry.id,
            expected
        );
    }
}

/// Every documentation file must have a registered rule.
#[test]
fn every_documentation_file_has_a_registered_rule() {
    let root = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docs = root.join(cargo_zempic::rules::DOCUMENTATION_DIRECTORY);

    let expected: Vec<String> = cargo_zempic::rules::registry()
        .iter()
        .map(|entry| cargo_zempic::rules::documentation_path(&entry.rule_id()))
        .collect();

    for relative in collect_markdown_files(&docs) {
        assert!(
            expected.contains(&relative),
            "documentation file `{relative}` has no registered rule"
        );
    }
}

/// Collect every Markdown file under `root`, as paths relative to `root`.
///
/// An absent directory contributes nothing, so a project with no rule
/// documentation yet passes the totality check.
fn collect_markdown_files(root: &camino::Utf8Path) -> Vec<String> {
    let mut collected = Vec::new();
    walk_markdown(root, root, &mut collected);

    collected
}

/// Recursively add Markdown files under `directory` to `collected`.
fn walk_markdown(
    root: &camino::Utf8Path,
    directory: &camino::Utf8Path,
    collected: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let path = camino::Utf8PathBuf::from_path_buf(entry.path()).expect("paths are UTF-8");

        if path.is_dir() {
            walk_markdown(root, &path, collected);
        } else if path.extension() == Some("md") {
            let relative = path
                .strip_prefix(root)
                .expect("the file is under the root")
                .to_owned();

            collected.push(relative.into_string());
        }
    }
}

/// The workspace file wins over the dot file, and a member file merges over
/// the base.
#[test]
fn config_discovery_merges_package_files_over_the_workspace_base() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let root = workspace.root();

    std::fs::write(
        root.join("zempic.toml"),
        "schema_version = 1\n\n[[ignore]]\nrule = \"profile/lto\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".zempic.toml"),
        "schema_version = 1\n\n[[ignore]]\nrule = \"never/read\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("alpha").join("zempic.toml"),
        "schema_version = 1\n\n[[ignore]]\nrule = \"deps/default-features\"\n",
    )
    .unwrap();

    let directories = vec![root.join("alpha"), root.join("beta")];
    let config = cargo_zempic::config::discover(root, &directories).unwrap();

    let texts: Vec<&str> = config
        .ignore
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();

    assert_eq!(texts, ["profile/lto", "deps/default-features"]);
}

/// `analyze` returns a report over a real workspace, carrying the resolution it
/// ran against and no findings.
#[test]
fn analyze_returns_an_empty_report_for_a_resolvable_workspace() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let config = analyze_config(&workspace, &[]);

    let report = cargo_zempic::analyze(&config).expect("the analysis succeeds");

    assert!(report.findings.is_empty());
    assert!(report.conflicts.is_empty());
    assert!(report.notices.is_empty());
    assert_eq!(report.summary, cargo_zempic::report::Summary::default());
    assert!(report.is_empty());
    assert_eq!(report.resolution.workspace_root, workspace.root());
}

/// `analyze` surfaces the notices of the workspace configuration.
#[test]
fn analyze_surfaces_notices_from_the_workspace_configuration() {
    let workspace = common::Fixture::new("multi-member").materialize();

    std::fs::write(
        workspace.root().join("zempic.toml"),
        "schema_version = 1\n\n[[ignore]]\nrule = \"profile/lto\"\nexpires = \"2026-10-03\"\n",
    )
    .unwrap();

    let report = cargo_zempic::analyze(&analyze_config(&workspace, &[])).expect("analyze runs");

    assert_eq!(
        report.notices,
        vec![
            cargo_zempic::report::Notice::ExpiredIgnore {
                selector: "profile/lto".to_owned(),
                expired: "2026-10-03".to_owned(),
            },
            cargo_zempic::report::Notice::UndocumentedIgnore {
                selector: "profile/lto".to_owned(),
            },
        ]
    );
}

/// `analyze` fails with the file named when the workspace configuration is
/// invalid.
#[test]
fn analyze_fails_when_the_workspace_configuration_is_invalid() {
    let workspace = common::Fixture::new("multi-member").materialize();

    std::fs::write(
        workspace.root().join("zempic.toml"),
        "schema_version = 1\n\n[analysis]\nnonsense = true\n",
    )
    .unwrap();

    let error = cargo_zempic::analyze(&analyze_config(&workspace, &[]))
        .expect_err("the invalid configuration fails the analysis");

    assert!(error.to_string().contains("zempic.toml"));
    assert!(error.to_string().contains("nonsense"));
}

/// The `multi-member` fixture's resolution, for assembling reports by hand.
fn fixture_resolution() -> Resolution {
    let workspace = common::Fixture::new("multi-member").materialize();
    let mut config = analysis_config(&workspace);
    config.dimensions = vec![Dimension::Size];

    resolve(&config).expect("the fixture resolves")
}

/// Build a finding from the fields the renderers read.
fn finding(
    rule: &str,
    tier: cargo_zempic::finding::Tier,
    severity: cargo_zempic::finding::Severity,
    magnitude: cargo_zempic::finding::Magnitude,
    change: Option<cargo_zempic::finding::Change>,
) -> cargo_zempic::finding::Finding {
    cargo_zempic::finding::Finding {
        rule: cargo_zempic::finding::RuleId::parse(rule).expect("the rule parses"),
        package: None,
        tier,
        severity,
        magnitude,
        confidence: cargo_zempic::finding::Confidence::High,
        effort: cargo_zempic::finding::Effort::Trivial,
        runtime_cost: cargo_zempic::finding::RuntimeCost::None,
        fixability: cargo_zempic::finding::Fixability::Safe,
        change,
        location: None,
        title: "title".to_owned(),
        detail: "detail".to_owned(),
        tradeoff: "tradeoff".to_owned(),
        references: Vec::new(),
    }
}

/// Assemble a report from findings over the fixture resolution.
fn report_of(findings: Vec<cargo_zempic::finding::Finding>) -> cargo_zempic::report::Report {
    cargo_zempic::report::Report::new(
        fixture_resolution(),
        findings,
        Vec::new(),
        Vec::new(),
        cargo_zempic::report::ReportSettings::default(),
    )
    .expect("the report assembles")
}

/// A key span pointing at `relative` under the resolution's workspace root.
fn span_in(root: &camino::Utf8Path, relative: &str, line: usize) -> KeySpan {
    KeySpan {
        file: root.join(relative),
        key: "profile.release.strip".to_owned(),
        line,
        column: 1,
    }
}

#[test]
fn human_renderer_emits_a_section_per_non_empty_tier() {
    use cargo_zempic::finding::{Magnitude, Severity, Tier};

    let resolution = fixture_resolution();
    let mut findings = vec![
        finding(
            "profile/strip",
            Tier::Foundational,
            Severity::High,
            Magnitude::Large,
            Some(cargo_zempic::finding::Change {
                current: "unset".to_owned(),
                recommended: "\"symbols\"".to_owned(),
            }),
        ),
        finding(
            "profile/debug-info",
            Tier::Foundational,
            Severity::High,
            Magnitude::Moderate,
            None,
        ),
        finding(
            "profile/lto",
            Tier::Compiler,
            Severity::Medium,
            Magnitude::Moderate,
            None,
        ),
        finding(
            "deps/duplicate-versions",
            Tier::Dependencies,
            Severity::Medium,
            Magnitude::Moderate,
            None,
        ),
        finding(
            "deps/workspace-resolver",
            Tier::Dependencies,
            Severity::High,
            Magnitude::Moderate,
            None,
        ),
    ];

    findings[1].location = Some(cargo_zempic::finding::Location::new(span_in(
        &resolution.workspace_root,
        "Cargo.toml",
        6,
    )));

    let report = cargo_zempic::report::Report::new(
        resolution,
        findings,
        Vec::new(),
        Vec::new(),
        cargo_zempic::report::ReportSettings::default(),
    )
    .unwrap();

    let rendered = cargo_zempic::report::human::render(&report, false, 80);

    assert!(rendered.contains("TIER 1  FOUNDATIONAL"), "{rendered}");
    assert!(rendered.contains("TIER 2  COMPILER"), "{rendered}");
    assert!(rendered.contains("TIER 4  DEPENDENCIES"), "{rendered}");
    assert!(!rendered.contains("TIER 3"), "{rendered}");
    assert!(!rendered.contains("TIER 5"), "{rendered}");
    assert!(rendered.contains("2 findings"), "{rendered}");
    assert!(rendered.contains("1 finding"), "{rendered}");
    assert!(rendered.contains("unset -> \"symbols\""), "{rendered}");
    assert!(rendered.contains("effort: trivial   runtime cost: none   fix: safe"));
    assert!(rendered.contains("Tradeoff: tradeoff"), "{rendered}");
}

#[test]
fn human_renderer_renders_the_change_column_or_the_location() {
    use cargo_zempic::finding::{Change, Location, Magnitude, Severity, Tier};

    let resolution = fixture_resolution();
    let mut with_location = finding(
        "config/rustflags-shadowed",
        Tier::Compiler,
        Severity::High,
        Magnitude::None,
        None,
    );
    with_location.location = Some(Location::new(span_in(
        &resolution.workspace_root,
        ".cargo/config.toml",
        3,
    )));

    let report = cargo_zempic::report::Report::new(
        resolution,
        vec![
            finding(
                "profile/strip",
                Tier::Foundational,
                Severity::High,
                Magnitude::Large,
                Some(Change {
                    current: "unset".to_owned(),
                    recommended: "\"symbols\"".to_owned(),
                }),
            ),
            with_location,
        ],
        Vec::new(),
        Vec::new(),
        cargo_zempic::report::ReportSettings::default(),
    )
    .unwrap();

    let rendered = cargo_zempic::report::human::render(&report, false, 80);

    assert!(rendered.contains("unset -> \"symbols\""), "{rendered}");
    assert!(rendered.contains(".cargo/config.toml"), "{rendered}");

    let absolute = report.resolution.workspace_root.join(".cargo/config.toml");
    assert!(
        !rendered.contains(absolute.as_str()),
        "a location inside the root must be relative: {rendered}"
    );
}

#[test]
fn human_renderer_states_there_are_no_findings_when_empty() {
    let report = report_of(Vec::new());

    let rendered = cargo_zempic::report::human::render(&report, false, 80);

    assert!(rendered.contains("cargo-zempic"), "{rendered}");
    assert!(rendered.contains("workspace:"), "{rendered}");
    assert!(rendered.contains("No findings."), "{rendered}");
}

#[test]
fn human_renderer_emits_no_escape_sequence_without_color() {
    use cargo_zempic::finding::{Magnitude, Severity, Tier};

    let report = report_of(vec![finding(
        "profile/strip",
        Tier::Foundational,
        Severity::High,
        Magnitude::Large,
        None,
    )]);

    let rendered = cargo_zempic::report::human::render(&report, false, 80);

    assert!(!rendered.contains('\u{1b}'), "{rendered:?}");
}

#[test]
fn human_renderer_renders_a_conflict_block() {
    use cargo_zempic::dimension::Dimension;
    use cargo_zempic::finding::{Magnitude, Recommendation, Severity, Tier};

    let report = cargo_zempic::report::Report::new(
        fixture_resolution(),
        vec![finding(
            "conflict/profile-opt-level",
            Tier::Compiler,
            Severity::High,
            Magnitude::None,
            None,
        )],
        vec![cargo_zempic::finding::Conflict {
            rule_path: "profile/opt-level".to_owned(),
            recommendations: vec![
                Recommendation {
                    dimension: Dimension::Size,
                    setting: "opt-level = \"z\"".to_owned(),
                },
                Recommendation {
                    dimension: Dimension::Size,
                    setting: "opt-level = 3".to_owned(),
                },
            ],
        }],
        Vec::new(),
        cargo_zempic::report::ReportSettings::default(),
    )
    .unwrap();

    let rendered = cargo_zempic::report::human::render(&report, false, 80);

    assert!(
        rendered.contains("CONFLICT  profile/opt-level"),
        "{rendered}"
    );
    assert!(
        rendered.contains("size recommends  opt-level = \"z\""),
        "{rendered}"
    );
    assert!(
        rendered.contains("No fix applied. Choose one dimension, or set a per-package override."),
        "{rendered}"
    );
}
