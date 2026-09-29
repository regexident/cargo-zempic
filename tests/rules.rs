use cargo_zempic::ZempicError;
use cargo_zempic::manifest::{KeySpan, ManifestDocument};
use cargo_zempic::workspace::{
    ArtifactKind, MetadataQuery, PackageSelection, classify_package, host_target_triple,
    load_metadata, select_packages,
};

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
