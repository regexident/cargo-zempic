use cargo_zempic::ZempicError;
use cargo_zempic::manifest::{KeySpan, ManifestDocument};
use cargo_zempic::workspace::{
    MetadataQuery, PackageSelection, host_target_triple, load_metadata, select_packages,
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
