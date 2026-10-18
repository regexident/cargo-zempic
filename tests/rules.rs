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

/// The fixture directories the rule tests name, one per rule condition.
const RULE_FIXTURES: [&str; 30] = [
    "fully-optimized",
    "bare-release",
    "strip-none",
    "debug-info-enabled",
    "split-debuginfo-off",
    "opt-level-speed",
    "lto-thin",
    "codegen-units-many",
    "incremental-enabled",
    "dependency-opt-level",
    "panic-unwind",
    "debug-assertions-enabled",
    "overflow-checks-enabled",
    "member-profile-table",
    "member-profile-tables",
    "rustflags-shadowed",
    "rustflags-cfg-table",
    "link-dead-code",
    "resolver-unset",
    "duplicate-versions",
    "default-features",
    "feature-unification",
    "feature-provenance",
    "library-only",
    "no-feature-granularity",
    "feature-trim",
    "knowledge-override",
    "likely-dev-only",
    "heavy-default-features",
    "build-only-note",
];

/// Every fixture materializes into a workspace `cargo metadata` resolves.
#[test]
fn every_fixture_materializes_and_resolves() {
    for name in RULE_FIXTURES {
        let workspace = common::Fixture::new(name).materialize();
        let config = analysis_config(&workspace);

        let resolution = resolve(&config)
            .unwrap_or_else(|error| panic!("fixture `{name}` must resolve: {error}"));

        assert!(
            resolution.member_count >= 1,
            "fixture `{name}` must select at least one member"
        );
    }
}

/// Every fixture directory is named by at least one test source file.
#[test]
fn every_fixture_directory_is_referenced_by_a_test() {
    let fixtures = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    let mut sources = String::new();

    for name in ["rules.rs", "cli.rs", "render.rs"] {
        let path = fixtures.parent().unwrap().join(name);
        sources.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
    }

    for entry in std::fs::read_dir(&fixtures).unwrap().flatten() {
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();

        assert!(
            sources.contains(&format!("\"{name}\"")),
            "fixture directory `{name}` is referenced by no test"
        );
    }
}

/// No fixture declares an external dependency.
#[test]
fn no_fixture_declares_an_external_dependency() {
    let fixtures = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut manifests = Vec::new();
    collect_fixture_manifests(&fixtures, &mut manifests);

    for path in manifests {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = text
            .parse::<toml_edit::DocumentMut>()
            .unwrap_or_else(|error| panic!("`{path}` must parse: {error}"));

        for table in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(dependencies) = document.get(table).and_then(|item| item.as_table_like())
            else {
                continue;
            };

            for (name, value) in dependencies.iter() {
                assert!(
                    value.get("path").is_some(),
                    "`{path}` declares external dependency `{name}`"
                );
            }
        }
    }
}

/// Collect every `Cargo.toml` under `directory` into `manifests`.
fn collect_fixture_manifests(
    directory: &camino::Utf8Path,
    manifests: &mut Vec<camino::Utf8PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let path = camino::Utf8PathBuf::from_path_buf(entry.path()).expect("paths are UTF-8");

        if path.is_dir() {
            collect_fixture_manifests(&path, manifests);
        } else if path.file_name() == Some("Cargo.toml") {
            manifests.push(path);
        }
    }
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
        profile: None,
        features: FeatureSelection::default(),
        target: None,
        format: None,
        fail_on: None,
        quiet: false,
        gates: cargo_zempic::config::Gates::default(),
        today: cargo_zempic::config::CivilDate::today(),
        environment: cargo_zempic::cargo_config::CargoEnvironment::sealed(workspace.root()),
    }
}

/// An analysis configuration for a workspace, with explicit gates and date.
fn analyze_config(workspace: &common::MaterializedWorkspace, only: &[&str]) -> AnalysisConfig {
    AnalysisConfig {
        dimensions: vec![Dimension::Size],
        manifest_path: Some(workspace.manifest_path()),
        selection: PackageSelection::default(),
        profile: None,
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
        environment: cargo_zempic::cargo_config::CargoEnvironment::sealed(workspace.root()),
    }
}

/// The findings a named fixture produces, optionally for a pinned target.
///
/// The analysis runs with the default configuration; `target` pins the triple
/// when a rule's condition depends on it.
fn findings_for(fixture: &str, target: Option<&str>) -> Vec<cargo_zempic::finding::Finding> {
    let workspace = common::Fixture::new(fixture).materialize();
    let mut config = analyze_config(&workspace, &[]);
    config.target = target.map(str::to_owned);

    cargo_zempic::analyze(&config)
        .expect("the analysis succeeds")
        .findings
}

/// The findings a named fixture produces after `files` overwrite or extend it.
fn findings_with_files(
    fixture: &str,
    files: &[(&str, &str)],
) -> Vec<cargo_zempic::finding::Finding> {
    let workspace = common::Fixture::new(fixture).materialize();

    for (relative_path, contents) in files {
        std::fs::write(workspace.root().join(relative_path), contents)
            .expect("the fixture file is written");
    }

    cargo_zempic::analyze(&analyze_config(&workspace, &[]))
        .expect("the analysis succeeds")
        .findings
}

/// The identifiers of `findings`, in report order.
fn finding_rules(findings: &[cargo_zempic::finding::Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| finding.rule.to_string())
        .collect()
}

/// The single finding whose rule renders as `rule`.
fn finding_for<'a>(
    findings: &'a [cargo_zempic::finding::Finding],
    rule: &str,
) -> &'a cargo_zempic::finding::Finding {
    let matches: Vec<&cargo_zempic::finding::Finding> = findings
        .iter()
        .filter(|finding| finding.rule.to_string() == rule)
        .collect();

    assert_eq!(
        matches.len(),
        1,
        "expected exactly one `{rule}`: {findings:?}"
    );

    matches[0]
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
    config.profile = Some("release".to_owned());

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

/// The analysis evaluates the rule registry against the resolved workspace.
#[test]
fn analysis_of_a_workspace_evaluates_the_registry() {
    let workspace = common::Fixture::new("bare-release").materialize();
    let config = analysis_config(&workspace);

    let resolution = resolve(&config).expect("resolution succeeds");
    let root_manifest =
        ManifestDocument::load(&workspace.root().join("Cargo.toml")).expect("the root manifest");

    let metadata =
        cargo_zempic::workspace::load_metadata(&cargo_zempic::workspace::MetadataQuery {
            manifest_path: Some(workspace.manifest_path()),
            target: Some(resolution.target_triple.clone()),
            features: FeatureSelection::default(),
        })
        .expect("the metadata loads");

    let findings = cargo_zempic::rules::evaluate(
        &resolution,
        &metadata,
        &cargo_zempic::knowledge::KnowledgeBase::embedded()
            .expect("the embedded knowledge base parses"),
        &root_manifest,
        &[],
        &[],
    );

    // A bare release profile leaves `strip` at its cargo default, so the
    // registry's `profile/strip` entry produces a finding.
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule.to_string() == "profile/strip"),
        "the registry must be evaluated: {findings:?}"
    );
}

/// The member manifests a rule reads exclude the workspace root's own.
#[test]
fn member_manifests_exclude_the_workspace_root() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let config = analysis_config(&workspace);

    let resolution = resolve(&config).expect("resolution succeeds");
    let manifests = cargo_zempic::member_manifests(&resolution).expect("the members parse");

    let names: Vec<&str> = manifests.iter().map(|(name, _)| name.as_str()).collect();

    assert_eq!(names, ["alpha", "beta"]);

    let root_manifest_path = resolution.workspace_root.join("Cargo.toml");

    for (_, document) in &manifests {
        assert_ne!(
            document.path(),
            root_manifest_path.as_path(),
            "the root package's own manifest must be excluded"
        );
    }
}

/// The resolved rustflags carried on the rule context cover every selected
/// package, including a non-virtual workspace's root package, unlike
/// `member_manifests` which excludes it.
#[test]
fn cargo_configs_cover_every_selected_package_including_the_root() {
    let single_package_workspace = common::Fixture::new("fully-optimized").materialize();
    let single_package_config = analysis_config(&single_package_workspace);
    let single_package_resolution = resolve(&single_package_config).expect("resolution succeeds");

    let single_package_configs = cargo_zempic::package_rustflags(
        &single_package_resolution,
        &single_package_config.environment,
    )
    .expect("rustflags resolve");

    assert_eq!(single_package_configs.len(), 1);
    assert_eq!(single_package_configs[0].0, "fully-optimized");
    assert_eq!(
        single_package_configs[0].1.source,
        cargo_zempic::cargo_config::RustflagsSource::Unset
    );

    let multi_member_workspace = common::Fixture::new("multi-member").materialize();
    let multi_member_config = analysis_config(&multi_member_workspace);
    let multi_member_resolution = resolve(&multi_member_config).expect("resolution succeeds");

    let multi_member_configs =
        cargo_zempic::package_rustflags(&multi_member_resolution, &multi_member_config.environment)
            .expect("rustflags resolve");

    let names: Vec<&str> = multi_member_configs
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();

    assert_eq!(names, ["alpha", "beta"]);
}

/// A member manifest that cannot be parsed fails the analysis.
#[test]
fn an_unparseable_member_manifest_fails_the_analysis() {
    let workspace = common::Fixture::new("multi-member").materialize();

    std::fs::write(
        workspace.root().join("alpha/Cargo.toml"),
        "not valid toml =",
    )
    .unwrap();

    let error = cargo_zempic::analyze(&analyze_config(&workspace, &[]))
        .expect_err("an unparseable member manifest fails the analysis");

    assert!(
        error.to_string().contains("Cargo.toml"),
        "the error must name the offending manifest: {error}"
    );
}

/// `analyze` returns a report over a real workspace, carrying the resolution it
/// ran against and no findings.
#[test]
fn analyze_returns_an_empty_report_for_a_resolvable_workspace() {
    let workspace = common::Fixture::new("fully-optimized").materialize();
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

#[test]
fn human_summary_lists_only_non_zero_categories() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity, Tier};

    let mut first = finding(
        "profile/strip",
        Tier::Foundational,
        Severity::High,
        Magnitude::Large,
        None,
    );
    first.fixability = Fixability::Safe;

    let second = finding(
        "config/link-dead-code",
        Tier::Compiler,
        Severity::High,
        Magnitude::Large,
        None,
    );

    let report = report_of(vec![first, second]);

    let rendered = cargo_zempic::report::human::render(&report, false, 80);

    assert!(rendered.contains("SUMMARY"), "{rendered}");
    assert!(rendered.contains("2 findings: 2 high."), "{rendered}");
    assert!(rendered.contains("By magnitude: 2 large."), "{rendered}");
    assert!(rendered.contains("2 auto-fixable."), "{rendered}");

    for absent in ["0 medium", "0 low", "0 info", "0 negligible", "0 none"] {
        assert!(
            !rendered.contains(absent),
            "must omit `{absent}`: {rendered}"
        );
    }
}

#[test]
fn human_summary_omits_the_fixable_line_when_nothing_is_fixable() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity, Tier};

    let mut only = finding(
        "config/rustflags-shadowed",
        Tier::Compiler,
        Severity::High,
        Magnitude::None,
        None,
    );
    only.fixability = Fixability::ReportOnly;

    let report = report_of(vec![only]);

    let rendered = cargo_zempic::report::human::render(&report, false, 80);

    assert!(!rendered.contains("auto-fixable"), "{rendered}");
}

#[test]
fn human_renderer_always_emits_the_magnitude_legend() {
    use cargo_zempic::finding::{Magnitude, Severity, Tier};

    let populated = report_of(vec![finding(
        "profile/strip",
        Tier::Foundational,
        Severity::High,
        Magnitude::Large,
        None,
    )]);
    let empty = report_of(Vec::new());

    for report in [&populated, &empty] {
        let rendered = cargo_zempic::report::human::render(report, false, 80);

        assert!(rendered.contains("ordinal judgement"), "{rendered}");
        assert!(rendered.contains("reports no sizes"), "{rendered}");
    }
}

/// A report carrying five findings across three tiers, for the quiet tests.
fn five_finding_report() -> cargo_zempic::report::Report {
    use cargo_zempic::finding::{Magnitude, Severity, Tier};

    report_of(vec![
        finding(
            "profile/strip",
            Tier::Foundational,
            Severity::High,
            Magnitude::Large,
            None,
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
    ])
}

#[test]
fn quiet_renderer_emits_one_line_per_finding() {
    let report = five_finding_report();

    let rendered = cargo_zempic::report::human::render_quiet(&report, false, 80);

    for rule in [
        "profile/strip",
        "profile/debug-info",
        "profile/lto",
        "deps/duplicate-versions",
        "deps/workspace-resolver",
    ] {
        assert!(
            rendered.lines().any(|line| line.starts_with(rule)),
            "`{rule}` must start a line: {rendered}"
        );
    }

    assert!(!rendered.contains("workspace:"), "{rendered}");
    assert!(!rendered.contains("TIER 1"), "{rendered}");
    assert!(!rendered.contains("Tradeoff:"), "{rendered}");
    assert!(!rendered.contains("effort:"), "{rendered}");
}

#[test]
fn quiet_renderer_keeps_the_summary_and_the_legend() {
    let report = five_finding_report();

    let rendered = cargo_zempic::report::human::render_quiet(&report, false, 80);

    assert!(rendered.contains("SUMMARY"), "{rendered}");
    assert!(rendered.contains("5 findings:"), "{rendered}");
    assert!(rendered.contains("ordinal judgement"), "{rendered}");
}

/// Assert the keys appear in `text` in exactly the order `keys` lists.
///
/// `serde_json::Value` does not preserve object key order without the
/// `preserve_order` feature, so the order is checked against the rendered text
/// by locating each key's quoted occurrence, ascending.
fn assert_key_order(text: &str, keys: &[String]) {
    let mut previous = 0;

    for key in keys {
        let needle = format!("\"{key}\"");
        let found = text[previous..]
            .find(&needle)
            .unwrap_or_else(|| panic!("key `{key}` not found after byte {previous}: {text}"));

        previous += found;
    }
}

/// The checked-in key sets of the JSON report fixture.
fn schema_fixture() -> serde_json::Value {
    let path = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/schema/report.schema.json");
    let text = std::fs::read_to_string(&path).expect("the schema fixture is checked in");

    serde_json::from_str(&text).expect("the schema fixture is valid JSON")
}

/// The key names of an array in the schema fixture.
fn fixture_keys(fixture: &serde_json::Value, name: &str) -> Vec<String> {
    fixture[name]
        .as_array()
        .unwrap_or_else(|| panic!("`{name}` is an array"))
        .iter()
        .map(|key| key.as_str().expect("a key name is a string").to_owned())
        .collect()
}

/// A report with one finding carrying a qualified rule, a change, a package
/// and a location, for the JSON key-set tests.
fn json_report() -> cargo_zempic::report::Report {
    use cargo_zempic::finding::{
        Change, Confidence, Effort, Fixability, Magnitude, RuleId, RuntimeCost, Severity, Tier,
    };

    let resolution = fixture_resolution();
    let with_everything = cargo_zempic::finding::Finding {
        rule: RuleId::parse("profile/opt-level:size").unwrap(),
        package: Some("alpha".to_owned()),
        tier: Tier::Compiler,
        severity: Severity::High,
        magnitude: Magnitude::Large,
        confidence: Confidence::High,
        effort: Effort::Moderate,
        runtime_cost: RuntimeCost::Minor,
        fixability: Fixability::Consequential,
        change: Some(Change {
            current: "3".to_owned(),
            recommended: "\"z\"".to_owned(),
        }),
        location: Some(cargo_zempic::finding::Location::new(span_in(
            &resolution.workspace_root,
            "Cargo.toml",
            12,
        ))),
        title: "Opt level".to_owned(),
        detail: "detail".to_owned(),
        tradeoff: "tradeoff".to_owned(),
        references: vec!["https://example.invalid".to_owned()],
    };
    let next = finding(
        "profile/lto",
        Tier::Compiler,
        Severity::Medium,
        Magnitude::Moderate,
        None,
    );
    let mut without = finding(
        "deps/duplicate-versions",
        Tier::Dependencies,
        Severity::Medium,
        Magnitude::Moderate,
        None,
    );
    without.change = None;
    without.package = None;
    without.location = None;

    cargo_zempic::report::Report::new(
        resolution,
        vec![with_everything, next, without],
        Vec::new(),
        Vec::new(),
        cargo_zempic::report::ReportSettings::default(),
    )
    .unwrap()
}

#[test]
fn json_document_matches_the_checked_in_key_sets() {
    let report = json_report();
    let document = cargo_zempic::report::json::render(&report).expect("the document renders");

    let fixture = schema_fixture();

    assert_key_order(&document, &fixture_keys(&fixture, "document"));

    // The first finding is the one built to carry every field.
    let findings_start = document.find("\"findings\"").expect("findings present");
    assert_key_order(
        &document[findings_start..],
        &fixture_keys(&fixture, "finding"),
    );

    let summary_start = document.find("\"summary\"").expect("summary present");
    assert_key_order(
        &document[summary_start..],
        &fixture_keys(&fixture, "summary"),
    );

    assert!(document.contains("\"schema_version\": 1"), "{document}");
    assert!(document.ends_with("}\n"), "{document}");
}

#[test]
fn json_document_splits_the_rule_path_from_its_dimension() {
    let report = json_report();
    let document = cargo_zempic::report::json::render(&report).expect("the document renders");
    let parsed: serde_json::Value = serde_json::from_str(&document).expect("the document is JSON");

    let first = &parsed["findings"][0];

    assert_eq!(first["rule"], "profile/opt-level");
    assert_eq!(first["dimension"], "size");
    assert_eq!(first["package"], "alpha");
    assert_eq!(first["change"]["current"], "3");
    assert_eq!(first["change"]["recommended"], "\"z\"");
    assert_eq!(first["location"]["file"], "Cargo.toml");
    assert_eq!(first["location"]["line"], 12);

    let third = &parsed["findings"][2];
    assert!(third["change"].is_null(), "{third}");
    assert!(third["package"].is_null(), "{third}");
    assert!(third["location"].is_null(), "{third}");
}

#[test]
fn json_document_carries_the_knowledge_base_version() {
    let report = json_report();
    let document = cargo_zempic::report::json::render(&report).expect("the document renders");
    let parsed: serde_json::Value = serde_json::from_str(&document).expect("the document is JSON");

    assert_eq!(parsed["kb_version"], 3, "{document}");
    assert_eq!(parsed["schema_version"], 1);

    // Every summary count is emitted, including zeros, so a machine consumer
    // reads a fixed shape.
    assert_eq!(parsed["summary"]["by_severity"]["info"], 0);
    assert_eq!(parsed["summary"]["by_magnitude"]["none"], 0);
}

/// A report with three findings over two tiers, one carrying a detail with a
/// pipe and a tradeoff with a newline.
fn markdown_report() -> cargo_zempic::report::Report {
    use cargo_zempic::finding::{Magnitude, Severity, Tier};

    let mut piped = finding(
        "profile/strip",
        Tier::Foundational,
        Severity::High,
        Magnitude::Large,
        None,
    );
    piped.detail = "a | b".to_owned();
    piped.tradeoff = "line one\nline two".to_owned();

    report_of(vec![
        piped,
        finding(
            "profile/debug-info",
            Tier::Foundational,
            Severity::High,
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
    ])
}

/// The number of cells a Markdown table row carries, counting unescaped pipes.
fn unescaped_cell_count(line: &str) -> usize {
    line.replace("\\|", "\u{0}").split('|').count()
}

#[test]
fn markdown_renderer_emits_a_table_per_non_empty_tier() {
    let report = markdown_report();

    let rendered = cargo_zempic::report::markdown::render(&report);

    assert!(rendered.contains("## Tier 1: Foundational"), "{rendered}");
    assert!(rendered.contains("## Tier 4: Dependencies"), "{rendered}");
    assert!(!rendered.contains("## Tier 2"), "{rendered}");

    let table_lines: Vec<&str> = rendered
        .lines()
        .filter(|line| line.starts_with("| "))
        .collect();

    // Each tier contributes a header row, a separator row and its rows; all
    // must agree on the column count.
    let expected = unescaped_cell_count(table_lines[0]);

    for line in &table_lines {
        assert_eq!(
            unescaped_cell_count(line),
            expected,
            "a row disagrees on column count: {line}"
        );
    }
}

#[test]
fn markdown_renderer_escapes_pipes_and_newlines_in_cells() {
    let report = markdown_report();

    let rendered = cargo_zempic::report::markdown::render(&report);

    assert!(rendered.contains("a \\| b"), "{rendered}");
    assert!(rendered.contains("line one line two"), "{rendered}");
    assert!(!rendered.contains("a | b"), "{rendered}");
}

#[test]
fn markdown_renderer_retains_the_legend_verbatim() {
    let populated = markdown_report();
    let empty = report_of(Vec::new());

    for report in [&populated, &empty] {
        let rendered = cargo_zempic::report::markdown::render(report);

        assert!(
            rendered
                .lines()
                .any(|line| line == cargo_zempic::report::MAGNITUDE_LEGEND),
            "the legend must appear on one unwrapped line: {rendered}"
        );
    }
}

#[test]
fn size_figure_scanner_rejects_figures_and_accepts_counts() {
    for rejected in [
        "47%",
        "saves 12 KB",
        "12KiB",
        "3 mb",
        "1,024 symbols",
        "1_024",
        "12 345",
    ] {
        assert!(
            !common::size_figures(rejected).is_empty(),
            "`{rejected}` must be rejected"
        );
    }

    for accepted in [
        "11 findings",
        "2 high",
        "7 members",
        "cargo-zempic 0.1.0",
        "serde_json 1.0.151",
        "unset -> \"symbols\"",
        "16 -> 1",
        "3 -> \"z\"",
        "Cargo.toml:12",
        "Cargo.toml:1024",
        "opt-level = 3",
    ] {
        assert!(
            common::size_figures(accepted).is_empty(),
            "`{accepted}` must be accepted, found: {:?}",
            common::size_figures(accepted)
        );
    }
}

// MARK: Tier 1 rules

#[test]
fn strip_fires_when_symbols_are_retained() {
    use cargo_zempic::finding::{Confidence, Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("strip-none", None);
    let strip = finding_for(&findings, "profile/strip");

    assert_eq!(strip.tier, Tier::Foundational);
    assert_eq!(strip.severity, Severity::High);
    assert_eq!(strip.magnitude, Magnitude::Large);
    assert_eq!(strip.fixability, Fixability::Safe);
    assert_eq!(strip.confidence, Confidence::High);
    assert_eq!(strip.package, None);
    assert!(!strip.tradeoff.is_empty());
    assert_eq!(
        strip
            .change
            .as_ref()
            .map(|change| change.recommended.as_str()),
        Some("\"symbols\"")
    );
    assert!(strip.location.is_some(), "an explicit setting has a span");
}

#[test]
fn strip_reports_unset_on_a_bare_profile() {
    let findings = findings_for("bare-release", None);
    let strip = finding_for(&findings, "profile/strip");

    assert_eq!(
        strip.change.as_ref().map(|change| change.current.as_str()),
        Some("unset")
    );
    assert_eq!(
        strip.location, None,
        "a cargo default has no key to point at"
    );
}

#[test]
fn debug_info_fires_and_accounts_for_strip() {
    // `debug-info-enabled` keeps `strip = "symbols"`, so the detail states the
    // gain is compile time and intermediate disk.
    let stripped = findings_for("debug-info-enabled", None);
    let with_strip = finding_for(&stripped, "profile/debug-info");

    assert!(
        with_strip.detail.contains("compile time"),
        "stripped detail must name compile time: {}",
        with_strip.detail
    );
    assert_eq!(
        with_strip
            .change
            .as_ref()
            .map(|change| change.recommended.as_str()),
        Some("false")
    );

    // A fixture with `debug` enabled and no stripping gets the artifact-retention
    // wording instead.
    let workspace = common::Fixture::new("debug-info-enabled").materialize();
    let root_manifest = std::fs::read_to_string(workspace.root().join("Cargo.toml"))
        .unwrap()
        .replace("strip = \"symbols\"", "strip = \"none\"");
    std::fs::write(workspace.root().join("Cargo.toml"), root_manifest).unwrap();

    let mut config = analyze_config(&workspace, &[]);
    config.target = None;
    let report = cargo_zempic::analyze(&config).expect("the analysis succeeds");
    let unstripped = finding_for(&report.findings, "profile/debug-info");

    assert!(
        !unstripped.detail.contains("compile time"),
        "unstripped detail must not name compile time: {}",
        unstripped.detail
    );
}

#[test]
fn split_debuginfo_fires_only_on_an_apple_target() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity, Tier};

    let apple = findings_for("split-debuginfo-off", Some("aarch64-apple-darwin"));
    let finding = finding_for(&apple, "profile/split-debuginfo");

    assert_eq!(finding.tier, Tier::Foundational);
    assert_eq!(finding.severity, Severity::Low);
    assert_eq!(finding.magnitude, Magnitude::Small);
    assert_eq!(finding.fixability, Fixability::Safe);
    assert_eq!(
        finding
            .change
            .as_ref()
            .map(|change| change.recommended.as_str()),
        Some("\"packed\"")
    );

    // On a non-Apple target the rule is quiet, because the setting has no effect
    // there.
    let linux = findings_for("split-debuginfo-off", Some("x86_64-unknown-linux-gnu"));

    assert!(
        !finding_rules(&linux).contains(&"profile/split-debuginfo".to_owned()),
        "the rule must be quiet on a Linux target: {:?}",
        finding_rules(&linux)
    );
}

#[test]
fn tier_one_symbol_rules_are_quiet_when_fully_optimized() {
    let findings = findings_for("fully-optimized", None);
    let rules = finding_rules(&findings);

    for absent in [
        "profile/strip",
        "profile/debug-info",
        "profile/split-debuginfo",
    ] {
        assert!(
            !rules.contains(&absent.to_owned()),
            "`{absent}` must not fire when fully optimized: {rules:?}"
        );
    }
}

#[test]
fn rule_documentation_files_exist_for_every_registered_rule() {
    let root = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docs = root.join(cargo_zempic::rules::DOCUMENTATION_DIRECTORY);

    for entry in cargo_zempic::rules::registry() {
        let expected = docs.join(cargo_zempic::rules::documentation_path(&entry.rule_id()));

        assert!(
            expected.is_file(),
            "rule `{}` has no documentation file at `{expected}`",
            entry.id
        );
        assert!(
            !entry.documentation.is_empty(),
            "rule `{}` embeds no documentation",
            entry.id
        );
    }
}

// MARK: missing-config

#[test]
fn missing_config_fires_on_a_bare_profile_alongside_the_key_rules() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("bare-release", None);
    let missing = finding_for(&findings, "profile/missing-config");

    assert_eq!(missing.tier, Tier::Foundational);
    assert_eq!(missing.severity, Severity::Medium);
    assert_eq!(missing.magnitude, Magnitude::Moderate);
    assert_eq!(missing.fixability, Fixability::ReportOnly);
    assert_eq!(missing.change, None);
    assert_eq!(
        missing.location, None,
        "a profile with no table has no location"
    );
    assert!(!missing.tradeoff.is_empty());

    // The rule co-exists with the per-key rules rather than replacing them.
    assert!(
        finding_rules(&findings).contains(&"profile/strip".to_owned()),
        "the per-key rules must still fire: {:?}",
        finding_rules(&findings)
    );
}

#[test]
fn missing_config_is_quiet_when_a_setting_is_explicit() {
    let findings = findings_for("fully-optimized", None);

    assert!(
        !finding_rules(&findings).contains(&"profile/missing-config".to_owned()),
        "the rule must be quiet when a setting is explicit: {:?}",
        finding_rules(&findings)
    );
}

#[test]
fn missing_config_is_quiet_for_an_inherited_profile() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let root_manifest = concat!(
        "[workspace]\n",
        "members = [\"alpha\", \"beta\"]\n\n",
        "[profile.parent]\n",
        "inherits = \"release\"\n",
        "strip = \"symbols\"\n\n",
        "[profile.child]\n",
        "inherits = \"parent\"\n",
    );
    std::fs::write(workspace.root().join("Cargo.toml"), root_manifest).unwrap();

    let mut config = analyze_config(&workspace, &[]);
    config.profile = Some("child".to_owned());
    let report = cargo_zempic::analyze(&config).expect("the analysis succeeds");

    // `child` inherits its settings, so it is not missing its configuration.
    assert!(
        !finding_rules(&report.findings).contains(&"profile/missing-config".to_owned()),
        "an inherited profile is configured: {:?}",
        finding_rules(&report.findings)
    );
}

// MARK: Tier 2 compiler rules

#[test]
fn opt_level_fires_for_the_size_dimension() {
    use cargo_zempic::finding::{Fixability, Magnitude, RuntimeCost, Severity, Tier};

    let findings = findings_for("opt-level-speed", None);
    let finding = finding_for(&findings, "profile/opt-level:size");

    assert_eq!(finding.tier, Tier::Compiler);
    assert_eq!(finding.severity, Severity::High);
    assert_eq!(finding.magnitude, Magnitude::Large);
    assert_eq!(finding.fixability, Fixability::Consequential);
    assert_eq!(finding.runtime_cost, RuntimeCost::Notable);
    assert_eq!(
        finding
            .change
            .as_ref()
            .map(|change| change.recommended.as_str()),
        Some("\"z\"")
    );
    assert!(finding.location.is_some());

    assert_eq!(
        cargo_zempic::rules::documentation_path(&finding.rule),
        "profile/opt-level.md"
    );
}

#[test]
fn lto_fires_when_link_time_optimization_is_partial() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("lto-thin", None);
    let finding = finding_for(&findings, "profile/lto");

    assert_eq!(finding.tier, Tier::Compiler);
    assert_eq!(finding.severity, Severity::Medium);
    assert_eq!(finding.magnitude, Magnitude::Moderate);
    assert_eq!(finding.fixability, Fixability::Consequential);
    assert_eq!(
        finding
            .change
            .as_ref()
            .map(|change| change.recommended.as_str()),
        Some("\"fat\"")
    );
}

#[test]
fn codegen_units_fires_and_names_its_dependence_on_lto() {
    let findings = findings_for("codegen-units-many", None);
    let finding = finding_for(&findings, "profile/codegen-units");

    assert!(
        finding.detail.contains("profile/lto"),
        "the detail must name its dependence on `profile/lto`: {}",
        finding.detail
    );
    assert_eq!(
        finding
            .change
            .as_ref()
            .map(|change| change.recommended.as_str()),
        Some("1")
    );
}

#[test]
fn incremental_is_quiet_on_the_dev_profile() {
    let workspace = common::Fixture::new("incremental-enabled").materialize();
    let mut config = analyze_config(&workspace, &[]);
    config.profile = Some("dev".to_owned());

    let report = cargo_zempic::analyze(&config).expect("the analysis succeeds");

    assert!(
        !finding_rules(&report.findings).contains(&"profile/incremental".to_owned()),
        "`dev` sets incremental deliberately: {:?}",
        finding_rules(&report.findings)
    );
}

#[test]
fn dependency_opt_level_is_quiet_when_the_override_exists() {
    let findings = findings_for("opt-level-speed", None);

    // `opt-level-speed` carries the glob override, so the rule is quiet even
    // though `opt-level` is not a size level.
    assert!(
        !finding_rules(&findings).contains(&"profile/dependency-opt-level".to_owned()),
        "the glob override quiets the rule: {:?}",
        finding_rules(&findings)
    );

    let firing = findings_for("dependency-opt-level", None);

    assert!(
        finding_rules(&firing).contains(&"profile/dependency-opt-level".to_owned()),
        "without the override the rule fires: {:?}",
        finding_rules(&firing)
    );
}

#[test]
fn tier_two_rules_are_quiet_when_fully_optimized() {
    let findings = findings_for("fully-optimized", None);
    let rules = finding_rules(&findings);

    for absent in [
        "profile/opt-level:size",
        "profile/lto",
        "profile/codegen-units",
        "profile/incremental",
        "profile/dependency-opt-level",
    ] {
        assert!(
            !rules.contains(&absent.to_owned()),
            "`{absent}` must not fire when fully optimized: {rules:?}"
        );
    }
}

// MARK: Tier 3 behavioral rules

#[test]
fn panic_abort_fires_when_unwinding_is_retained() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("panic-unwind", None);
    let finding = finding_for(&findings, "profile/panic-abort");

    assert_eq!(finding.tier, Tier::Behavioral);
    assert_eq!(finding.severity, Severity::Low);
    assert_eq!(finding.magnitude, Magnitude::Small);
    assert_eq!(finding.fixability, Fixability::Consequential);
    assert_eq!(
        finding
            .change
            .as_ref()
            .map(|change| change.recommended.as_str()),
        Some("\"abort\"")
    );
    assert!(
        finding.tradeoff.contains("catch_unwind"),
        "the tradeoff must warn about `catch_unwind`: {}",
        finding.tradeoff
    );
}

#[test]
fn panic_abort_is_skipped_with_a_notice_on_the_bench_profile() {
    let workspace = common::Fixture::new("panic-unwind").materialize();
    let mut config = analyze_config(&workspace, &[]);
    config.profile = Some("bench".to_owned());

    let report = cargo_zempic::analyze(&config).expect("the analysis succeeds");

    assert!(
        !finding_rules(&report.findings).contains(&"profile/panic-abort".to_owned()),
        "the rule is skipped on `bench`: {:?}",
        finding_rules(&report.findings)
    );
    assert!(
        report.notices.iter().any(|notice| matches!(
            notice,
            cargo_zempic::report::Notice::SkippedPanicStrategy { profile } if profile == "bench"
        )),
        "the run must say the rule was skipped: {:?}",
        report.notices
    );
}

#[test]
fn panic_abort_is_skipped_for_a_profile_inheriting_from_test() {
    let workspace = common::Fixture::new("panic-unwind").materialize();

    let root_manifest = concat!(
        "[package]\n",
        "name = \"panic-unwind\"\n",
        "version = \"0.1.0\"\n",
        "edition = \"2021\"\n\n",
        "[[bin]]\n",
        "name = \"panic-unwind\"\n",
        "path = \"src/main.rs\"\n\n",
        "[profile.custom]\n",
        "inherits = \"test\"\n",
    );
    std::fs::write(workspace.root().join("Cargo.toml"), root_manifest).unwrap();

    let mut config = analyze_config(&workspace, &[]);
    config.profile = Some("custom".to_owned());
    let report = cargo_zempic::analyze(&config).expect("the analysis succeeds");

    assert!(
        !finding_rules(&report.findings).contains(&"profile/panic-abort".to_owned()),
        "a profile inheriting `test` is skipped: {:?}",
        finding_rules(&report.findings)
    );
    assert!(
        report.notices.iter().any(|notice| matches!(
            notice,
            cargo_zempic::report::Notice::SkippedPanicStrategy { .. }
        )),
        "the run must say the rule was skipped: {:?}",
        report.notices
    );
}

#[test]
fn behavioral_checks_are_quiet_on_the_dev_profile() {
    let workspace = common::Fixture::new("debug-assertions-enabled").materialize();
    let mut config = analyze_config(&workspace, &[]);
    config.profile = Some("dev".to_owned());

    let report = cargo_zempic::analyze(&config).expect("the analysis succeeds");
    let rules = finding_rules(&report.findings);

    for absent in ["profile/debug-assertions", "profile/overflow-checks"] {
        assert!(
            !rules.contains(&absent.to_owned()),
            "`{absent}` must be quiet on `dev`: {rules:?}"
        );
    }
}

#[test]
fn tier_three_rules_are_quiet_when_fully_optimized() {
    let findings = findings_for("fully-optimized", None);
    let rules = finding_rules(&findings);

    for absent in [
        "profile/panic-abort",
        "profile/debug-assertions",
        "profile/overflow-checks",
    ] {
        assert!(
            !rules.contains(&absent.to_owned()),
            "`{absent}` must not fire when fully optimized: {rules:?}"
        );
    }
}

// MARK: workspace/ignored-member-profile

#[test]
fn ignored_member_profile_fires_per_offending_member() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("member-profile-table", None);
    let finding = finding_for(&findings, "workspace/ignored-member-profile");

    assert_eq!(finding.tier, Tier::Foundational);
    assert_eq!(finding.severity, Severity::High);
    assert_eq!(finding.magnitude, Magnitude::None);
    assert_eq!(finding.fixability, Fixability::Safe);
    assert_eq!(finding.change, None);
    assert_eq!(finding.package.as_deref(), Some("member"));

    let location = finding
        .location
        .as_ref()
        .expect("the finding has a location");
    assert!(
        location.span.file.as_str().ends_with("member/Cargo.toml"),
        "the location must point at the member's manifest: {}",
        location.span.file
    );
}

#[test]
fn ignored_member_profile_fires_for_every_offending_member() {
    let findings = findings_for("member-profile-tables", None);

    let matches: Vec<&cargo_zempic::finding::Finding> = findings
        .iter()
        .filter(|finding| finding.rule.to_string() == "workspace/ignored-member-profile")
        .collect();

    assert_eq!(
        matches.len(),
        2,
        "each offending member must produce its own finding: {findings:?}"
    );

    let packages: std::collections::BTreeSet<Option<&str>> = matches
        .iter()
        .map(|finding| finding.package.as_deref())
        .collect();

    assert_eq!(
        packages,
        std::collections::BTreeSet::from([Some("alpha"), Some("beta")]),
        "both members must be named: {matches:?}"
    );

    let mut files: Vec<&str> = matches
        .iter()
        .map(|finding| {
            finding
                .location
                .as_ref()
                .expect("each finding points at its member's manifest")
                .span
                .file
                .as_str()
        })
        .collect();
    files.sort_unstable();

    assert_eq!(files.len(), 2);
    assert_ne!(
        files[0], files[1],
        "the two findings must point at distinct locations: {files:?}"
    );
    assert!(
        files[0].ends_with("alpha/Cargo.toml"),
        "the first location must name alpha's manifest: {files:?}"
    );
    assert!(
        files[1].ends_with("beta/Cargo.toml"),
        "the second location must name beta's manifest: {files:?}"
    );
}

#[test]
fn a_dimension_neutral_rule_yields_one_finding_per_run() {
    let workspace = common::Fixture::new("member-profile-table").materialize();
    let mut config = analyze_config(&workspace, &[]);
    config.dimensions = vec![Dimension::Size, Dimension::Size];

    let findings = cargo_zempic::analyze(&config)
        .expect("the analysis succeeds")
        .findings;

    let matches: Vec<&cargo_zempic::finding::Finding> = findings
        .iter()
        .filter(|finding| finding.rule.to_string() == "workspace/ignored-member-profile")
        .collect();

    assert_eq!(
        matches.len(),
        1,
        "a dimension-neutral rule must collapse to one finding across repeated \
         dimensions: {findings:?}"
    );
}

#[test]
fn ignored_member_profile_ignores_the_root_table() {
    // `fully-optimized` declares a root `[profile.release]` table and has no
    // member table, so the rule must stay quiet.
    let findings = findings_for("fully-optimized", None);

    assert!(
        !finding_rules(&findings).contains(&"workspace/ignored-member-profile".to_owned()),
        "the root table is honored and must not fire: {:?}",
        finding_rules(&findings)
    );
}

#[test]
fn ignored_member_profile_is_quiet_without_a_member_table() {
    let findings = findings_for("multi-member", None);

    assert!(
        !finding_rules(&findings).contains(&"workspace/ignored-member-profile".to_owned()),
        "a workspace with no member table must not fire: {:?}",
        finding_rules(&findings)
    );
}

// MARK: Registry totality

#[test]
fn registry_covers_every_concrete_assignment() {
    use cargo_zempic::finding::RuleId;

    let mut registered: Vec<String> = cargo_zempic::rules::registry()
        .iter()
        .map(|entry| entry.id.to_owned())
        .collect();
    registered.sort();

    let expected = [
        "advisory/lib-profile-not-applicable",
        "config/link-dead-code",
        "config/link-gc-sections",
        "config/rustflags-shadowed",
        "deps/build-only-note",
        "deps/default-features",
        "deps/duplicate-versions",
        "deps/feature-provenance",
        "deps/feature-trim",
        "deps/feature-unification",
        "deps/likely-dev-only",
        "deps/workspace-resolver",
        "lib/heavy-default-features",
        "lib/no-feature-granularity",
        "profile/codegen-units",
        "profile/debug-assertions",
        "profile/debug-info",
        "profile/dependency-opt-level",
        "profile/incremental",
        "profile/lto",
        "profile/missing-config",
        "profile/opt-level:size",
        "profile/overflow-checks",
        "profile/panic-abort",
        "profile/split-debuginfo",
        "profile/strip",
        "workspace/ignored-member-profile",
    ];

    assert_eq!(registered, expected, "the registry must hold exactly these");

    // Every concrete severity assignment has a registry entry, so an
    // assignment cannot be left unregistered.
    for (text, _) in cargo_zempic::rules::severity_assignments() {
        // A single-segment selector such as `advisory` names a family rather
        // than a rule; only a concrete identifier needs a registry entry.
        let Ok(id) = RuleId::parse(text) else {
            continue;
        };

        if id.path().len() == 1 {
            continue;
        }

        assert!(
            cargo_zempic::rules::metadata_for(&id).is_some(),
            "assignment `{text}` has no registered rule"
        );
    }
}

/// Every registered rule fires on at least one [`RULE_FIXTURES`] entry, under
/// at least one of the two pinned target triples, and leaves at least one
/// other combination quiet.
///
/// A rule registered with a severity assignment but no quiet fixture would
/// fire on everything and nobody would notice, because only the positive case
/// was ever exercised; this proves both directions for the whole registry at
/// once rather than relying on each rule's own hand-written tests to
/// remember it.
#[test]
fn every_registered_rule_has_a_firing_and_a_quiet_fixture() {
    use std::collections::BTreeMap;

    const TRIPLES: [&str; 2] = ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"];

    let mut findings_by_combination: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();

    for fixture in RULE_FIXTURES {
        for triple in TRIPLES {
            let findings = findings_for(fixture, Some(triple));
            findings_by_combination.insert((fixture, triple), finding_rules(&findings));
        }
    }

    for entry in cargo_zempic::rules::registry() {
        let fires = findings_by_combination
            .values()
            .any(|rules| rules.contains(&entry.id.to_owned()));
        let quiet = findings_by_combination
            .values()
            .any(|rules| !rules.contains(&entry.id.to_owned()));

        assert!(
            fires,
            "`{}` has no firing fixture across RULE_FIXTURES x {TRIPLES:?}",
            entry.id
        );
        assert!(
            quiet,
            "`{}` has no quiet fixture across RULE_FIXTURES x {TRIPLES:?}",
            entry.id
        );
    }
}

#[test]
fn every_registered_rule_has_a_distinct_identifier() {
    let mut ids: Vec<&str> = cargo_zempic::rules::registry()
        .iter()
        .map(|entry| entry.id)
        .collect();
    let total = ids.len();
    ids.sort();
    ids.dedup();

    assert_eq!(ids.len(), total, "no identifier may repeat");
}

#[test]
fn every_registered_rule_carries_a_tradeoff_on_every_finding_it_emits() {
    for fixture in RULE_FIXTURES {
        let findings = findings_for(fixture, None);

        for finding in &findings {
            assert!(
                !finding.tradeoff.trim().is_empty(),
                "`{fixture}` produced a finding with no tradeoff: {finding:?}"
            );
            assert!(
                !finding.title.trim().is_empty(),
                "`{fixture}` produced a finding with no title: {finding:?}"
            );
            assert!(
                !finding.detail.trim().is_empty(),
                "`{fixture}` produced a finding with no detail: {finding:?}"
            );
        }
    }
}

// MARK: config/rustflags-shadowed

#[test]
fn rustflags_shadowed_fires_when_a_target_table_outranks_build() {
    use cargo_zempic::finding::{Confidence, Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("rustflags-shadowed", Some("x86_64-unknown-linux-gnu"));
    let finding = finding_for(&findings, "config/rustflags-shadowed");

    assert_eq!(finding.tier, Tier::Compiler);
    assert_eq!(finding.severity, Severity::High);
    assert_eq!(finding.magnitude, Magnitude::None);
    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert_eq!(finding.confidence, Confidence::High);
    assert_eq!(finding.change, None);
    assert_eq!(finding.package.as_deref(), Some("rustflags-shadowed"));

    let location = finding
        .location
        .as_ref()
        .expect("the finding points at the shadowed key");
    assert!(
        location.span.file.as_str().ends_with(".cargo/config.toml"),
        "the location must point at the cargo configuration file: {}",
        location.span.file
    );
    assert_eq!(location.span.key, "build.rustflags");

    assert!(
        finding.detail.contains("build.rustflags"),
        "the detail must name the shadowed key: {}",
        finding.detail
    );
    assert!(
        finding.detail.contains("x86_64-unknown-linux-gnu"),
        "the detail must name the live target table: {}",
        finding.detail
    );

    // Plausible bug: firing without checking that the `target.*` table's
    // triple matches the one being built. `rustflags-shadowed`'s table names
    // only `x86_64-unknown-linux-gnu`, so under a foreign triple only
    // `build.rustflags` is live and nothing is shadowed.
    let foreign_triple_findings = findings_for("rustflags-shadowed", Some("aarch64-apple-darwin"));
    assert!(
        !finding_rules(&foreign_triple_findings).contains(&"config/rustflags-shadowed".to_owned()),
        "a target table naming a different triple must not shadow anything: {:?}",
        finding_rules(&foreign_triple_findings)
    );
}

#[test]
fn rustflags_shadowed_caps_confidence_for_an_unevaluated_cfg_table() {
    use cargo_zempic::finding::Confidence;

    let findings = findings_for("rustflags-cfg-table", Some("x86_64-unknown-linux-gnu"));
    let finding = finding_for(&findings, "config/rustflags-shadowed");

    assert_eq!(
        finding.confidence,
        Confidence::Medium,
        "a live `cfg(...)` table whose predicate was not evaluated must cap \
         confidence: {finding:?}"
    );
}

#[test]
fn rustflags_shadowed_is_quiet_with_a_single_source() {
    // `link-dead-code` carries only `build.rustflags`, with no `target.*`
    // table in its chain at all: no rank below it is present to be shadowed.
    let findings = findings_for("link-dead-code", Some("x86_64-unknown-linux-gnu"));

    assert!(
        !finding_rules(&findings).contains(&"config/rustflags-shadowed".to_owned()),
        "a single live source must not fire: {:?}",
        finding_rules(&findings)
    );
}

#[test]
fn rustflags_shadowed_is_quiet_without_a_cargo_configuration() {
    // `fully-optimized` carries no `.cargo/config.toml` at all.
    let findings = findings_for("fully-optimized", None);

    assert!(
        !finding_rules(&findings).contains(&"config/rustflags-shadowed".to_owned()),
        "a workspace with no cargo configuration must not fire: {:?}",
        finding_rules(&findings)
    );
}

// MARK: config/link-dead-code

/// The findings a named fixture produces under `environment`, optionally for
/// a pinned target.
fn findings_for_environment(
    fixture: &str,
    target: Option<&str>,
    environment: cargo_zempic::cargo_config::CargoEnvironment,
) -> Vec<cargo_zempic::finding::Finding> {
    let workspace = common::Fixture::new(fixture).materialize();
    let mut config = analyze_config(&workspace, &[]);
    config.target = target.map(str::to_owned);
    config.environment = environment;

    cargo_zempic::analyze(&config)
        .expect("the analysis succeeds")
        .findings
}

#[test]
fn link_dead_code_fires_on_the_live_source() {
    use cargo_zempic::finding::{Confidence, Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("link-dead-code", Some("x86_64-unknown-linux-gnu"));
    let finding = finding_for(&findings, "config/link-dead-code");

    assert_eq!(finding.tier, Tier::Compiler);
    assert_eq!(finding.severity, Severity::High);
    assert_eq!(finding.magnitude, Magnitude::None);
    assert_eq!(finding.fixability, Fixability::Safe);
    assert_eq!(finding.confidence, Confidence::High);
    assert_eq!(finding.change, None);
    assert_eq!(finding.package.as_deref(), Some("link-dead-code"));

    let location = finding
        .location
        .as_ref()
        .expect("the finding points at the key declaring the flag");
    assert_eq!(location.span.key, "build.rustflags");
    assert!(
        location.span.file.as_str().ends_with(".cargo/config.toml"),
        "the location must point at the cargo configuration file: {}",
        location.span.file
    );

    assert!(
        finding.tradeoff.contains("rebuild"),
        "the tradeoff must note the full rebuild cost: {}",
        finding.tradeoff
    );
}

#[test]
fn link_dead_code_is_report_only_from_the_environment() {
    use cargo_zempic::cargo_config::CargoEnvironment;
    use cargo_zempic::finding::Fixability;

    let workspace = common::Fixture::new("link-dead-code").materialize();
    let environment = CargoEnvironment {
        rustflags: Some("-C link-dead-code".to_owned()),
        ..CargoEnvironment::sealed(workspace.root())
    };

    let findings = findings_for_environment(
        "link-dead-code",
        Some("x86_64-unknown-linux-gnu"),
        environment,
    );
    let finding = finding_for(&findings, "config/link-dead-code");

    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(
        finding.detail.contains("RUSTFLAGS"),
        "the detail must name the environment variable: {}",
        finding.detail
    );
}

#[test]
fn link_dead_code_is_quiet_without_the_flag() {
    let findings = findings_for("fully-optimized", None);

    assert!(
        !finding_rules(&findings).contains(&"config/link-dead-code".to_owned()),
        "a workspace whose rustflags never set the flag must not fire: {:?}",
        finding_rules(&findings)
    );
}

#[test]
fn link_dead_code_ignores_a_shadowed_flag() {
    use cargo_zempic::cargo_config::CargoEnvironment;

    // The same fixture materializes a `build.rustflags` carrying the flag,
    // but under its own triple the environment outranks it, so only
    // `RUSTFLAGS` is live and `build.rustflags` never reaches the compiler.
    let workspace = common::Fixture::new("link-dead-code").materialize();
    let environment = CargoEnvironment {
        rustflags: Some("-C target-cpu=native".to_owned()),
        ..CargoEnvironment::sealed(workspace.root())
    };

    let findings = findings_for_environment(
        "link-dead-code",
        Some("x86_64-unknown-linux-gnu"),
        environment,
    );

    assert!(
        !finding_rules(&findings).contains(&"config/link-dead-code".to_owned()),
        "a flag shadowed by a higher-ranked source must not fire: {:?}",
        finding_rules(&findings)
    );
}

// MARK: config/link-gc-sections

#[test]
fn link_gc_sections_fires_when_dead_code_suppresses_the_default() {
    use cargo_zempic::finding::{Confidence, Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("link-dead-code", Some("x86_64-unknown-linux-gnu"));
    let finding = finding_for(&findings, "config/link-gc-sections");

    assert_eq!(finding.tier, Tier::Compiler);
    assert_eq!(finding.severity, Severity::Medium);
    assert_eq!(finding.magnitude, Magnitude::Moderate);
    assert_eq!(finding.fixability, Fixability::Consequential);
    assert_eq!(finding.confidence, Confidence::High);

    let change = finding
        .change
        .as_ref()
        .expect("the finding recommends the target's own section-gc flag");
    assert_eq!(change.current, "unset");
    assert_eq!(change.recommended, "-C link-arg=-Wl,--gc-sections");

    assert!(
        finding.detail.contains("config/link-dead-code"),
        "the detail must name the companion rule: {}",
        finding.detail
    );
    assert!(
        finding.tradeoff.contains("rebuild"),
        "the tradeoff must note the full rebuild cost: {}",
        finding.tradeoff
    );

    // Plausible bug: implementing §9.2's condition literally and recommending
    // a generic flag rather than the resolved target's own spelling.
    let darwin_findings = findings_for("link-dead-code", Some("aarch64-apple-darwin"));
    let darwin_finding = finding_for(&darwin_findings, "config/link-gc-sections");
    let darwin_change = darwin_finding
        .change
        .as_ref()
        .expect("the finding recommends the target's own section-gc flag");
    assert_eq!(darwin_change.recommended, "-C link-arg=-Wl,-dead_strip");
}

#[test]
fn link_gc_sections_recommends_the_apple_flag_on_an_apple_target() {
    let findings = findings_for("link-dead-code", Some("aarch64-apple-darwin"));
    let finding = finding_for(&findings, "config/link-gc-sections");

    let change = finding
        .change
        .as_ref()
        .expect("the finding recommends the target's own section-gc flag");
    assert_eq!(change.recommended, "-C link-arg=-Wl,-dead_strip");
}

#[test]
fn link_gc_sections_is_quiet_on_a_classified_target() {
    // `fully-optimized` sets no section-gc flag and no `link-dead-code`, on a
    // classified host target: rustc's own default already applies.
    let fully_optimized_findings = findings_for("fully-optimized", None);
    assert!(
        !finding_rules(&fully_optimized_findings).contains(&"config/link-gc-sections".to_owned()),
        "a classified target with no suppressing flag must not fire: {:?}",
        finding_rules(&fully_optimized_findings)
    );

    // `rustflags-shadowed` sets no `link-dead-code` either; its live source is
    // `target.x86_64-unknown-linux-gnu.rustflags`, a classified ELF target.
    let shadowed_findings = findings_for("rustflags-shadowed", Some("x86_64-unknown-linux-gnu"));
    assert!(
        !finding_rules(&shadowed_findings).contains(&"config/link-gc-sections".to_owned()),
        "a classified target with no suppressing flag must not fire: {:?}",
        finding_rules(&shadowed_findings)
    );
}

#[test]
fn link_gc_sections_caps_confidence_on_an_unclassified_target() {
    use cargo_zempic::finding::Confidence;

    // `aarch64-unknown-none` matches none of the recognized linker families.
    let findings = findings_for("rustflags-shadowed", Some("aarch64-unknown-none"));
    let finding = finding_for(&findings, "config/link-gc-sections");

    assert_eq!(finding.confidence, Confidence::Medium);
    assert_eq!(finding.change, None);
}

#[test]
fn link_gc_sections_prioritizes_dead_code_over_an_unclassified_target() {
    use cargo_zempic::finding::Confidence;

    let findings = findings_for("link-dead-code", Some("aarch64-unknown-none"));
    let finding = finding_for(&findings, "config/link-gc-sections");

    assert_eq!(finding.confidence, Confidence::High);
    assert!(
        finding.detail.contains("config/link-dead-code"),
        "the detail must name the companion rule when the dead-code flag is the true cause, \
         even on an unclassified target: {}",
        finding.detail
    );
}

// MARK: deps/workspace-resolver

#[test]
fn workspace_resolver_fires_on_a_virtual_manifest_without_a_resolver() {
    use cargo_zempic::finding::{Confidence, Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("resolver-unset", None);
    let finding = finding_for(&findings, "deps/workspace-resolver");

    assert_eq!(finding.tier, Tier::Dependencies);
    assert_eq!(finding.severity, Severity::High);
    assert_eq!(finding.magnitude, Magnitude::Moderate);
    assert_eq!(finding.fixability, Fixability::Consequential);
    assert_eq!(finding.confidence, Confidence::High);
    assert_eq!(finding.package, None);
    assert_eq!(
        finding
            .change
            .as_ref()
            .map(|change| (change.current.as_str(), change.recommended.as_str())),
        Some(("unset", "2"))
    );

    let span = &finding
        .location
        .as_ref()
        .expect("the finding carries a location")
        .span;

    assert_eq!(span.key, "workspace.resolver");
    assert_eq!(span.line, 1);
    assert!(span.file.as_str().ends_with("Cargo.toml"), "{}", span.file);
}

#[test]
fn workspace_resolver_is_quiet_when_the_workspace_sets_one() {
    let findings = findings_for("multi-member", None);

    assert!(!finding_rules(&findings).contains(&"deps/workspace-resolver".to_owned()));
}

#[test]
fn workspace_resolver_is_quiet_for_an_edition_2021_package() {
    let findings = findings_for("bare-release", None);

    assert!(!finding_rules(&findings).contains(&"deps/workspace-resolver".to_owned()));
}

// MARK: deps/duplicate-versions

#[test]
fn duplicate_versions_fires_on_an_incompatible_duplicate() {
    use cargo_zempic::finding::{Effort, Fixability, Magnitude, Severity};

    let findings = findings_for("duplicate-versions", None);
    let finding = finding_for(&findings, "deps/duplicate-versions");

    assert_eq!(finding.package, None);
    assert_eq!(finding.location, None);
    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert_eq!(finding.severity, Severity::Medium);
    assert_eq!(finding.magnitude, Magnitude::Moderate);
    assert_eq!(finding.effort, Effort::Moderate);
    assert!(finding.title.contains("`shared`"), "{}", finding.title);
    assert!(
        finding.detail.contains("`1.0.0`, `2.0.0`"),
        "{}",
        finding.detail
    );
}

#[test]
fn duplicate_versions_is_quiet_without_a_duplicate() {
    let findings = findings_for("bare-release", None);

    assert!(!finding_rules(&findings).contains(&"deps/duplicate-versions".to_owned()));
}

// MARK: deps/default-features

#[test]
fn default_features_fires_when_defaults_enable_an_optional_dependency() {
    use cargo_zempic::finding::{Confidence, Fixability, Severity};

    let findings = findings_for("default-features", None);
    let finding = finding_for(&findings, "deps/default-features");

    assert_eq!(finding.package.as_deref(), Some("default-features"));
    assert_eq!(finding.confidence, Confidence::Medium);
    assert_eq!(finding.fixability, Fixability::Consequential);
    assert_eq!(finding.severity, Severity::Medium);
    assert!(finding.detail.contains("`inner`"), "{}", finding.detail);
    assert!(
        finding
            .detail
            .contains("Resolver-v2 kind splitting is approximated"),
        "{}",
        finding.detail
    );

    let span = &finding
        .location
        .as_ref()
        .expect("the finding carries a location")
        .span;

    assert_eq!(span.key, "dependencies.heavy");
    assert_eq!(span.file.file_name(), Some("Cargo.toml"));
    // The materialized workspace is removed once `findings_for` returns, so the
    // manifest cannot be read back; only the root manifest declares the
    // `heavy` key the span resolved.
    assert_ne!(
        span.file.parent().and_then(|parent| parent.file_name()),
        Some("heavy")
    );
}

#[test]
fn default_features_is_quiet_without_optional_dependencies_behind_defaults() {
    let findings = findings_for("bare-release", None);

    assert!(!finding_rules(&findings).contains(&"deps/default-features".to_owned()));
}

// MARK: deps/feature-unification

#[test]
fn feature_unification_fires_when_another_member_requests_defaults() {
    use cargo_zempic::finding::{Confidence, Fixability};

    let findings = findings_for("feature-unification", None);
    let finding = finding_for(&findings, "deps/feature-unification");

    assert_eq!(finding.package.as_deref(), Some("alpha"));
    assert_eq!(finding.confidence, Confidence::Medium);
    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(finding.detail.contains("`beta`"), "{}", finding.detail);
    assert!(
        finding
            .detail
            .contains("Resolver-v2 kind splitting is approximated"),
        "{}",
        finding.detail
    );

    let span = &finding
        .location
        .as_ref()
        .expect("the finding carries a location")
        .span;

    assert_eq!(span.key, "dependencies.shared");
    assert!(span.file.ends_with("alpha/Cargo.toml"), "{}", span.file);

    assert!(
        findings
            .iter()
            .all(|finding| finding.package.as_deref() != Some("beta")
                || finding.rule.to_string() != "deps/feature-unification")
    );
}

#[test]
fn feature_unification_is_quiet_without_an_opt_out() {
    let findings = findings_for("default-features", None);

    assert!(!finding_rules(&findings).contains(&"deps/feature-unification".to_owned()));
}

#[test]
fn default_features_is_quiet_when_defaults_enable_no_optional_dependency() {
    let findings = findings_for("feature-unification", None);

    assert!(!finding_rules(&findings).contains(&"deps/default-features".to_owned()));
}

// MARK: deps/feature-provenance

#[test]
fn feature_provenance_fires_for_a_feature_only_a_transitive_dependency_requests() {
    use cargo_zempic::finding::{Confidence, Magnitude, Severity};

    let findings = findings_for("feature-provenance", None);
    let finding = finding_for(&findings, "deps/feature-provenance");

    assert_eq!(finding.package.as_deref(), Some("alpha"));
    assert_eq!(finding.confidence, Confidence::Medium);
    assert_eq!(finding.magnitude, Magnitude::Small);
    assert_eq!(finding.severity, Severity::Low);
    assert!(finding.title.contains("`shared`"), "{}", finding.title);
    assert!(finding.detail.contains("`extra`"), "{}", finding.detail);

    let span = &finding
        .location
        .as_ref()
        .expect("the finding carries a location")
        .span;

    assert_eq!(span.key, "dependencies.shared");
}

#[test]
fn feature_provenance_is_quiet_when_a_member_requests_the_feature() {
    let findings = findings_for("feature-unification", None);

    assert!(!finding_rules(&findings).contains(&"deps/feature-provenance".to_owned()));
}

#[test]
fn feature_unification_is_quiet_when_defaults_stay_off() {
    let findings = findings_for("feature-provenance", None);

    assert!(!finding_rules(&findings).contains(&"deps/feature-unification".to_owned()));
}

// MARK: deps/feature-trim

/// The `zempic.toml` text of the `feature-trim` fixture with the given entry
/// fields.
fn feature_trim_configuration(versions: &str, verified_through: &str, fixable: bool) -> String {
    format!(
        "schema_version = 1\n\
         \n\
         [knowledge_base.crates.metrics]\n\
         versions = \"{versions}\"\n\
         verified_through = \"{verified_through}\"\n\
         \n\
         [[knowledge_base.crates.metrics.features]]\n\
         name = \"telemetry\"\n\
         default = true\n\
         magnitude = \"moderate\"\n\
         advice = \"disable unless the service exports metrics\"\n\
         fixable = {fixable}\n"
    )
}

/// The `zempic.toml` text that overrides the embedded `regex` entry.
fn regex_override(versions: &str, verified_through: &str) -> String {
    format!(
        "schema_version = 1\n\
         \n\
         [knowledge_base.crates.regex]\n\
         versions = \"{versions}\"\n\
         verified_through = \"{verified_through}\"\n\
         \n\
         [[knowledge_base.crates.regex.features]]\n\
         name = \"unicode\"\n\
         default = true\n\
         magnitude = \"small\"\n\
         advice = \"Custom advice for the fixture.\"\n\
         fixable = false\n"
    )
}

#[test]
fn feature_trim_fires_on_a_default_feature_the_knowledge_base_flags() {
    use cargo_zempic::finding::{Confidence, Fixability, Magnitude, Severity};

    let findings = findings_for("feature-trim", None);
    let finding = finding_for(&findings, "deps/feature-trim");

    assert_eq!(finding.package.as_deref(), Some("feature-trim"));
    assert_eq!(finding.severity, Severity::Medium);
    assert_eq!(finding.magnitude, Magnitude::Moderate);
    assert_eq!(finding.confidence, Confidence::High);
    assert_eq!(finding.fixability, Fixability::Consequential);
    assert_eq!(
        finding
            .change
            .as_ref()
            .map(|change| (change.current.as_str(), change.recommended.as_str())),
        Some(("true", "false"))
    );
    assert_eq!(
        finding
            .location
            .as_ref()
            .expect("the finding carries a location")
            .span
            .key,
        "dependencies.metrics"
    );
    assert_eq!(
        finding.title,
        "Default feature `telemetry` of `metrics` can be trimmed"
    );
    assert!(
        finding
            .detail
            .contains("disable unless the service exports metrics"),
        "{}",
        finding.detail
    );
    assert!(
        !finding.detail.contains("Advice verified through"),
        "{}",
        finding.detail
    );
}

#[test]
fn feature_trim_is_quiet_outside_the_entry_version_range() {
    let configuration = feature_trim_configuration(">=2.0, <3", "2.0.0", true);
    let findings = findings_with_files("feature-trim", &[("zempic.toml", &configuration)]);

    assert!(!finding_rules(&findings).contains(&"deps/feature-trim".to_owned()));
}

#[test]
fn feature_trim_reports_a_stale_entry_as_report_only() {
    use cargo_zempic::finding::Fixability;

    let configuration = feature_trim_configuration(">=1.0, <2", "1.1.0", true);
    let findings = findings_with_files("feature-trim", &[("zempic.toml", &configuration)]);
    let finding = finding_for(&findings, "deps/feature-trim");

    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(
        finding
            .detail
            .ends_with("Advice verified through metrics 1.1.0; you have 1.2.0."),
        "{}",
        finding.detail
    );
}

#[test]
fn feature_trim_reports_an_unfixable_entry_as_report_only() {
    use cargo_zempic::finding::Fixability;

    let configuration = feature_trim_configuration(">=1.0, <2", "1.2.0", false);
    let findings = findings_with_files("feature-trim", &[("zempic.toml", &configuration)]);
    let finding = finding_for(&findings, "deps/feature-trim");

    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(
        !finding.detail.contains("Advice verified through"),
        "{}",
        finding.detail
    );
}

#[test]
fn feature_trim_is_quiet_when_the_feature_is_requested_explicitly() {
    let manifest = "[package]\nname = \"feature-trim\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                    [dependencies]\nmetrics = { path = \"vendor/metrics\", features = [\"telemetry\"] }\n\n\
                    [workspace]\nexclude = [\"vendor\"]\n";
    let findings = findings_with_files("feature-trim", &[("Cargo.toml", manifest)]);

    assert!(!finding_rules(&findings).contains(&"deps/feature-trim".to_owned()));
}

#[test]
fn feature_trim_is_quiet_when_defaults_are_disabled() {
    let manifest = "[package]\nname = \"feature-trim\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                    [dependencies]\nmetrics = { path = \"vendor/metrics\", default-features = false }\n\n\
                    [workspace]\nexclude = [\"vendor\"]\n";
    let findings = findings_with_files("feature-trim", &[("Cargo.toml", manifest)]);

    assert!(!finding_rules(&findings).contains(&"deps/feature-trim".to_owned()));
}

#[test]
fn feature_trim_is_quiet_without_dependencies() {
    let findings = findings_for("bare-release", None);

    assert!(!finding_rules(&findings).contains(&"deps/feature-trim".to_owned()));
}

#[test]
fn feature_trim_applies_the_embedded_entry() {
    use cargo_zempic::finding::{Fixability, Magnitude};

    let findings = findings_for("knowledge-override", None);
    let finding = finding_for(&findings, "deps/feature-trim");

    assert_eq!(
        finding.title,
        "Default feature `unicode` of `regex` can be trimmed"
    );
    assert_eq!(finding.fixability, Fixability::Consequential);
    assert_eq!(finding.magnitude, Magnitude::Moderate);
    assert!(
        finding.detail.contains("Embeds Unicode data tables"),
        "{}",
        finding.detail
    );
}

#[test]
fn feature_trim_honors_a_configured_override() {
    use cargo_zempic::finding::{Fixability, Magnitude};

    let configuration = regex_override(">=1.0, <2", "1.10.0");
    let findings = findings_with_files("knowledge-override", &[("zempic.toml", &configuration)]);
    let finding = finding_for(&findings, "deps/feature-trim");

    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert_eq!(finding.magnitude, Magnitude::Small);
    assert!(
        finding.detail.contains("Custom advice for the fixture."),
        "{}",
        finding.detail
    );
    assert!(
        !finding.detail.contains("Embeds Unicode data tables"),
        "{}",
        finding.detail
    );
}

#[test]
fn feature_trim_is_quiet_when_an_override_excludes_the_version() {
    let configuration = regex_override(">=2.0, <3", "2.0.0");
    let findings = findings_with_files("knowledge-override", &[("zempic.toml", &configuration)]);

    assert!(!finding_rules(&findings).contains(&"deps/feature-trim".to_owned()));
}

// MARK: deps/likely-dev-only

/// The `zempic.toml` text of the `likely-dev-only` fixture with the given
/// version range, verified release and `dev_only` flag.
fn likely_dev_only_configuration(versions: &str, verified_through: &str, dev_only: bool) -> String {
    format!(
        "schema_version = 1\n\
         \n\
         [knowledge_base.crates.bench-harness]\n\
         versions = \"{versions}\"\n\
         verified_through = \"{verified_through}\"\n\
         dev_only = {dev_only}\n"
    )
}

#[test]
fn likely_dev_only_fires_on_a_development_crate_in_normal_dependencies() {
    use cargo_zempic::finding::{Confidence, Fixability, Magnitude, Severity};

    let findings = findings_for("likely-dev-only", None);
    let finding = finding_for(&findings, "deps/likely-dev-only");

    assert_eq!(finding.package.as_deref(), Some("likely-dev-only"));
    assert_eq!(finding.severity, Severity::Medium);
    assert_eq!(finding.magnitude, Magnitude::Moderate);
    assert_eq!(finding.confidence, Confidence::Medium);
    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(finding.change.is_none());
    assert_eq!(
        finding
            .location
            .as_ref()
            .expect("the finding carries a location")
            .span
            .key,
        "dependencies.bench-harness"
    );
    assert_eq!(
        finding.title,
        "`bench-harness` is usually a development-only dependency"
    );
    assert!(
        !finding.detail.contains("Advice verified through"),
        "{}",
        finding.detail
    );

    // The same entry without the `dev_only` flag must stay quiet.
    let configuration = likely_dev_only_configuration(">=0.3, <0.4", "0.3.0", false);
    let findings = findings_with_files("likely-dev-only", &[("zempic.toml", &configuration)]);

    assert!(!finding_rules(&findings).contains(&"deps/likely-dev-only".to_owned()));
}

#[test]
fn likely_dev_only_is_quiet_under_dev_dependencies() {
    let manifest = "[package]\nname = \"likely-dev-only\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                    [dev-dependencies]\nbench-harness = { path = \"vendor/bench-harness\" }\n\n\
                    [workspace]\nexclude = [\"vendor\"]\n";
    let findings = findings_with_files("likely-dev-only", &[("Cargo.toml", manifest)]);

    assert!(!finding_rules(&findings).contains(&"deps/likely-dev-only".to_owned()));
}

#[test]
fn likely_dev_only_is_quiet_without_dependencies() {
    let findings = findings_for("bare-release", None);

    assert!(!finding_rules(&findings).contains(&"deps/likely-dev-only".to_owned()));
}

#[test]
fn likely_dev_only_notes_a_stale_entry() {
    use cargo_zempic::finding::Fixability;

    let configuration = likely_dev_only_configuration(">=0.2, <0.4", "0.2.0", true);
    let findings = findings_with_files("likely-dev-only", &[("zempic.toml", &configuration)]);
    let finding = finding_for(&findings, "deps/likely-dev-only");

    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(
        finding
            .detail
            .ends_with("Advice verified through bench-harness 0.2.0; you have 0.3.0."),
        "{}",
        finding.detail
    );
}

// MARK: deps/build-only-note

/// The `Cargo.toml` text of the `build-only-note` fixture with `extra_dependencies`
/// appended to its `[dependencies]` table and `extra_sections` after the
/// build dependencies.
fn build_only_note_manifest(extra_dependencies: &str, extra_sections: &str) -> String {
    format!(
        "[package]\nname = \"build-only-note\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n\n\
         [dependencies]\n\
         derive-helper = {{ path = \"vendor/derive-helper\" }}\n\
         runtime = {{ path = \"vendor/runtime\" }}\n\
         {extra_dependencies}\n\
         [build-dependencies]\n\
         generator = {{ path = \"vendor/generator\" }}\n\n\
         {extra_sections}\n\
         [workspace]\nexclude = [\"vendor\"]\n"
    )
}

/// The `zempic.toml` text of the `build-only-note` fixture with one entry per
/// `(crate, versions, verified_through, magnitude)` tuple.
fn build_only_note_configuration(entries: &[(&str, &str, &str, &str)]) -> String {
    let mut text = String::from("schema_version = 1\n");

    for (name, versions, verified_through, magnitude) in entries {
        text.push_str(&format!(
            "\n[knowledge_base.crates.{name}]\n\
             versions = \"{versions}\"\n\
             verified_through = \"{verified_through}\"\n\
             magnitude = \"{magnitude}\"\n"
        ));
    }

    text
}

#[test]
fn build_only_note_fires_when_heavy_crates_compile_only_for_the_host() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity};

    let findings = findings_for("build-only-note", None);
    let finding = finding_for(&findings, "deps/build-only-note");

    assert_eq!(finding.package.as_deref(), Some("build-only-note"));
    assert_eq!(finding.severity, Severity::Info);
    assert_eq!(finding.magnitude, Magnitude::None);
    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(finding.location.is_none());
    assert!(finding.change.is_none());
    assert_eq!(
        finding.title,
        "The heaviest known dependencies of `build-only-note` compile only for the host"
    );
    assert!(
        finding
            .detail
            .starts_with("`generator`, `parser` are marked heavy"),
        "{}",
        finding.detail
    );

    // Development dependencies never compile into the artifact, so declaring
    // `parser` as one adds no target path to it: `parser` stays reachable only
    // through the procedural macro and the note is unchanged.
    let manifest = build_only_note_manifest(
        "",
        "[dev-dependencies]\nparser = { path = \"vendor/parser\" }\n",
    );
    let findings = findings_with_files("build-only-note", &[("Cargo.toml", &manifest)]);
    let finding = finding_for(&findings, "deps/build-only-note");

    assert!(
        finding
            .detail
            .starts_with("`generator`, `parser` are marked heavy"),
        "{}",
        finding.detail
    );

    // Entries verified against an older release are noted as stale, in the
    // order the crates are listed.
    let configuration = build_only_note_configuration(&[
        ("generator", ">=0.9, <2", "0.9.0", "large"),
        ("parser", ">=0.9, <2", "0.9.0", "moderate"),
    ]);
    let findings = findings_with_files("build-only-note", &[("zempic.toml", &configuration)]);
    let finding = finding_for(&findings, "deps/build-only-note");

    assert!(
        finding.detail.ends_with(
            "Advice verified through generator 0.9.0; you have 1.0.0. \
             Advice verified through parser 0.9.0; you have 1.0.0."
        ),
        "{}",
        finding.detail
    );
}

#[test]
fn build_only_note_is_quiet_when_a_heavy_crate_reaches_the_artifact() {
    let rule = "deps/build-only-note".to_owned();

    // A heavy crate on the target path silences the note.
    let configuration = build_only_note_configuration(&[
        ("generator", ">=1.0, <2", "1.0.0", "large"),
        ("parser", ">=1.0, <2", "1.0.0", "large"),
        ("runtime", ">=1.0, <2", "1.0.0", "large"),
    ]);
    let findings = findings_with_files("build-only-note", &[("zempic.toml", &configuration)]);

    assert!(!finding_rules(&findings).contains(&rule));

    // A crate reached both through a procedural macro and directly still
    // reaches the artifact, whichever path is walked first.
    let manifest = build_only_note_manifest("parser = { path = \"vendor/parser\" }\n", "");
    let findings = findings_with_files("build-only-note", &[("Cargo.toml", &manifest)]);

    assert!(!finding_rules(&findings).contains(&rule));

    // The walk pushes a package's dependencies in name order and pops the last
    // one first, so `derive-helper` is expanded before `adapter` (which sorts
    // ahead of it). `parser` is therefore reached on the host path first and on
    // the target path second; a visited set keyed by package id alone would
    // discard the second visit and report the note.
    let manifest = build_only_note_manifest("", "").replace("runtime = { path", "adapter = { path");
    let adapter = "[package]\nname = \"adapter\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n\
                   [dependencies]\nparser = { path = \"../parser\" }\n";
    let findings = findings_with_files(
        "build-only-note",
        &[
            ("Cargo.toml", &manifest),
            ("vendor/runtime/Cargo.toml", adapter),
        ],
    );

    assert!(!finding_rules(&findings).contains(&rule));
}

#[test]
fn build_only_note_is_quiet_without_heavy_crates() {
    let rule = "deps/build-only-note".to_owned();

    let findings = findings_for("bare-release", None);

    assert!(!finding_rules(&findings).contains(&rule));

    // Dependencies that are not heavy leave nothing to note.
    let configuration = build_only_note_configuration(&[
        ("generator", ">=1.0, <2", "1.0.0", "small"),
        ("parser", ">=1.0, <2", "1.0.0", "small"),
    ]);
    let findings = findings_with_files("build-only-note", &[("zempic.toml", &configuration)]);

    assert!(!finding_rules(&findings).contains(&rule));
}

// MARK: advisory/lib-profile-not-applicable

#[test]
fn lib_profile_not_applicable_replaces_profile_rules_for_a_library() {
    use cargo_zempic::finding::{Fixability, Magnitude, Severity, Tier};

    let findings = findings_for("library-only", None);
    let finding = finding_for(&findings, "advisory/lib-profile-not-applicable");

    assert_eq!(finding.package.as_deref(), Some("library-only"));
    assert_eq!(finding.tier, Tier::Advanced);
    assert_eq!(finding.severity, Severity::Info);
    assert_eq!(finding.magnitude, Magnitude::None);
    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(finding.title.contains("`release`"), "{}", finding.title);
    assert!(
        finding_rules(&findings)
            .iter()
            .all(|rule| !rule.starts_with("profile/") && !rule.starts_with("config/")),
        "{findings:?}"
    );
}

#[test]
fn lib_profile_not_applicable_is_quiet_for_a_linked_package() {
    let findings = findings_for("bare-release", None);

    assert!(!finding_rules(&findings).contains(&"advisory/lib-profile-not-applicable".to_owned()));
}

// MARK: lib/heavy-default-features

/// The `Cargo.toml` text of the `heavy-default-features` fixture with the given
/// `[dependencies]`-style sections and `[features]` body.
fn heavy_default_features_manifest(dependency_sections: &str, features: &str) -> String {
    format!(
        "[package]\nname = \"heavy-default-features\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [lib]\npath = \"src/lib.rs\"\n\n\
         {dependency_sections}\n\
         [features]\n{features}\n\
         [workspace]\nexclude = [\"vendor\"]\n"
    )
}

/// The `zempic.toml` text of the `heavy-default-features` fixture with the
/// given entry fields; `magnitude` is omitted when `None`.
fn heavy_default_features_configuration(
    versions: &str,
    verified_through: &str,
    magnitude: Option<&str>,
) -> String {
    let magnitude = magnitude
        .map(|text| format!("magnitude = \"{text}\"\n"))
        .unwrap_or_default();

    format!(
        "schema_version = 1\n\
         \n\
         [knowledge_base.crates.engine]\n\
         versions = \"{versions}\"\n\
         verified_through = \"{verified_through}\"\n\
         {magnitude}"
    )
}

/// The findings a named fixture produces after `files` overwrite or extend it,
/// with the analysis pinned to `target`.
fn findings_with_files_for_target(
    fixture: &str,
    files: &[(&str, &str)],
    target: &str,
) -> Vec<cargo_zempic::finding::Finding> {
    let workspace = common::Fixture::new(fixture).materialize();

    for (relative_path, contents) in files {
        std::fs::write(workspace.root().join(relative_path), contents)
            .expect("the fixture file is written");
    }

    let mut config = analyze_config(&workspace, &[]);
    config.target = Some(target.to_owned());

    cargo_zempic::analyze(&config)
        .expect("the analysis succeeds")
        .findings
}

#[test]
fn heavy_default_features_fires_when_defaults_enable_a_heavy_dependency() {
    use cargo_zempic::finding::{Confidence, Fixability, Magnitude, Severity};

    let findings = findings_for("heavy-default-features", None);
    let finding = finding_for(&findings, "lib/heavy-default-features");

    assert_eq!(finding.package.as_deref(), Some("heavy-default-features"));
    assert_eq!(finding.severity, Severity::Medium);
    assert_eq!(finding.magnitude, Magnitude::Large);
    assert_eq!(finding.confidence, Confidence::High);
    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(finding.change.is_none());
    assert_eq!(
        finding
            .location
            .as_ref()
            .expect("the finding carries a location")
            .span
            .key,
        "features.default"
    );
    assert_eq!(
        finding.title,
        "Default features of `heavy-default-features` enable the heavy dependency `engine`"
    );
    assert!(
        finding.detail.contains("marked large"),
        "{}",
        finding.detail
    );
    assert!(
        !finding.detail.contains("Advice verified through"),
        "{}",
        finding.detail
    );

    // The finding carries the entry's magnitude, not a fixed one.
    let configuration =
        heavy_default_features_configuration(">=0.4, <0.5", "0.4.0", Some("moderate"));
    let findings =
        findings_with_files("heavy-default-features", &[("zempic.toml", &configuration)]);
    let finding = finding_for(&findings, "lib/heavy-default-features");

    assert_eq!(finding.magnitude, Magnitude::Moderate);
    assert!(
        finding.detail.contains("marked moderate"),
        "{}",
        finding.detail
    );

    // An entry verified against an older release is noted as stale.
    let configuration = heavy_default_features_configuration(">=0.3, <0.5", "0.3.0", Some("large"));
    let findings =
        findings_with_files("heavy-default-features", &[("zempic.toml", &configuration)]);
    let finding = finding_for(&findings, "lib/heavy-default-features");

    assert!(
        finding
            .detail
            .ends_with("Advice verified through engine 0.3.0; you have 0.4.0."),
        "{}",
        finding.detail
    );

    // A renamed dependency is matched by the key the feature table names.
    let manifest = heavy_default_features_manifest(
        "[dependencies]\nmotor = { package = \"engine\", path = \"vendor/engine\", optional = true }\n",
        "default = [\"fast\"]\nfast = [\"dep:motor\"]\n",
    );
    let findings = findings_with_files("heavy-default-features", &[("Cargo.toml", &manifest)]);
    let finding = finding_for(&findings, "lib/heavy-default-features");

    assert_eq!(
        finding.title,
        "Default features of `heavy-default-features` enable the heavy dependency `motor`"
    );
    assert!(
        finding.detail.contains("resolves to `engine` 0.4.0"),
        "{}",
        finding.detail
    );

    // A target-specific dependency is reported when the analyzed target
    // resolves it.
    let manifest = heavy_default_features_manifest(
        "[target.'cfg(windows)'.dependencies]\nengine = { path = \"vendor/engine\", optional = true }\n",
        "default = [\"fast\"]\nfast = [\"dep:engine\"]\n",
    );
    let findings = findings_with_files_for_target(
        "heavy-default-features",
        &[("Cargo.toml", &manifest)],
        "x86_64-pc-windows-msvc",
    );

    assert!(finding_rules(&findings).contains(&"lib/heavy-default-features".to_owned()));
}

#[test]
fn heavy_default_features_is_quiet_for_a_light_dependency() {
    let rule = "lib/heavy-default-features".to_owned();

    let configuration = heavy_default_features_configuration(">=0.4, <0.5", "0.4.0", Some("small"));
    let findings =
        findings_with_files("heavy-default-features", &[("zempic.toml", &configuration)]);

    assert!(!finding_rules(&findings).contains(&rule));

    // An entry without a crate-level magnitude is not heavy.
    let configuration = heavy_default_features_configuration(">=0.4, <0.5", "0.4.0", None);
    let findings =
        findings_with_files("heavy-default-features", &[("zempic.toml", &configuration)]);

    assert!(!finding_rules(&findings).contains(&rule));

    // An entry whose range excludes the resolved version does not apply.
    let configuration = heavy_default_features_configuration(">=0.5, <0.6", "0.5.0", Some("large"));
    let findings =
        findings_with_files("heavy-default-features", &[("zempic.toml", &configuration)]);

    assert!(!finding_rules(&findings).contains(&rule));
}

#[test]
fn heavy_default_features_is_quiet_when_defaults_do_not_reach_it() {
    let rule = "lib/heavy-default-features".to_owned();

    let manifest = heavy_default_features_manifest(
        "[dependencies]\nengine = { path = \"vendor/engine\", optional = true }\n",
        "default = []\nfast = [\"dep:engine\"]\n",
    );
    let findings = findings_with_files("heavy-default-features", &[("Cargo.toml", &manifest)]);

    assert!(!finding_rules(&findings).contains(&rule));

    // A default set that enables only other features does not reach it either.
    let manifest = heavy_default_features_manifest(
        "[dependencies]\nengine = { path = \"vendor/engine\", optional = true }\n",
        "default = [\"other\"]\nother = []\nfast = [\"dep:engine\"]\n",
    );
    let findings = findings_with_files("heavy-default-features", &[("Cargo.toml", &manifest)]);

    assert!(!finding_rules(&findings).contains(&rule));

    // An optional build dependency is not compiled into consumers.
    let manifest = heavy_default_features_manifest(
        "[build-dependencies]\nengine = { path = \"vendor/engine\", optional = true }\n",
        "default = [\"fast\"]\nfast = [\"dep:engine\"]\n",
    );
    let findings = findings_with_files("heavy-default-features", &[("Cargo.toml", &manifest)]);

    assert!(!finding_rules(&findings).contains(&rule));

    // A dependency the analyzed target does not resolve has no known version.
    let manifest = heavy_default_features_manifest(
        "[target.'cfg(windows)'.dependencies]\nengine = { path = \"vendor/engine\", optional = true }\n",
        "default = [\"fast\"]\nfast = [\"dep:engine\"]\n",
    );
    let findings = findings_with_files_for_target(
        "heavy-default-features",
        &[("Cargo.toml", &manifest)],
        "x86_64-unknown-linux-gnu",
    );

    assert!(!finding_rules(&findings).contains(&rule));
}

#[test]
fn heavy_default_features_is_quiet_for_a_library_without_dependencies() {
    let rule = "lib/heavy-default-features".to_owned();

    let findings = findings_for("library-only", None);

    assert!(!finding_rules(&findings).contains(&rule));

    // A package that produces a linked artifact is not a library.
    let findings = findings_with_files(
        "heavy-default-features",
        &[("src/main.rs", "fn main() {}\n")],
    );

    assert!(!finding_rules(&findings).contains(&rule));
}

// MARK: lib/no-feature-granularity

#[test]
fn no_feature_granularity_fires_for_a_library_with_ten_dependencies() {
    use cargo_zempic::finding::{Fixability, Severity};

    let findings = findings_for("no-feature-granularity", None);
    let finding = finding_for(&findings, "lib/no-feature-granularity");

    assert_eq!(finding.package.as_deref(), Some("lib-root"));
    assert_eq!(finding.severity, Severity::Medium);
    assert_eq!(finding.fixability, Fixability::ReportOnly);
    assert!(
        finding.detail.contains("10 direct normal dependencies"),
        "{}",
        finding.detail
    );
}

#[test]
fn no_feature_granularity_is_quiet_for_a_library_without_dependencies() {
    let findings = findings_for("library-only", None);

    assert!(!finding_rules(&findings).contains(&"lib/no-feature-granularity".to_owned()));
}

// MARK: Knowledge base

#[test]
fn embedded_knowledge_base_parses_with_the_curated_crates() {
    let base = cargo_zempic::knowledge::KnowledgeBase::embedded()
        .expect("the embedded knowledge base parses");

    assert_eq!(
        base.crates().keys().collect::<Vec<_>>(),
        [
            "bindgen",
            "clap",
            "criterion",
            "env_logger",
            "image",
            "insta",
            "mockall",
            "pretty_assertions",
            "proptest",
            "quickcheck",
            "regex",
            "reqwest",
            "rstest",
            "syn",
            "tokio",
        ]
    );
}

#[test]
fn embedded_knowledge_base_entries_are_verified_within_their_ranges() {
    let base = cargo_zempic::knowledge::KnowledgeBase::embedded()
        .expect("the embedded knowledge base parses");

    for (name, entry) in base.crates() {
        assert!(entry.versions.matches(&entry.verified_through), "{name}");
    }
}

// MARK: Toolchain

#[test]
fn active_prefers_the_workspace_toolchain_file() {
    let directory = tempfile::TempDir::new().unwrap();
    std::fs::write(
        directory.path().join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"nightly\"\n",
    )
    .unwrap();

    let root = camino::Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();

    assert_eq!(
        cargo_zempic::toolchain::active(&root),
        cargo_zempic::toolchain::ToolchainChannel::Nightly
    );
}

#[test]
fn embedded_knowledge_base_advice_carries_no_size_figure() {
    let base = cargo_zempic::knowledge::KnowledgeBase::embedded()
        .expect("the embedded knowledge base parses");

    for (name, entry) in base.crates() {
        for feature in &entry.features {
            common::assert_no_size_figures(&format!("{name}/{}", feature.name), &feature.advice);
        }
    }
}
