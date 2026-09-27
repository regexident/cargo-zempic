#![allow(dead_code)]

//! Materializing checked-in fixtures into throwaway cargo workspaces.
//!
//! A fixture under `tests/fixtures/<name>/` carries only the files that express
//! the condition under test. [`Fixture::materialize`] copies such a directory
//! into a fresh [`tempfile::TempDir`] and fills in the files a cargo workspace
//! needs but the fixture deliberately omits, so a fixture that is a single
//! `Cargo.toml` still resolves. Every Rust integration-test binary compiles this
//! module independently, so items used by one binary but not another would warn
//! as dead code under the `-D warnings` clippy gate; the crate-level allowance
//! above suppresses that across all binaries.

use std::path::Path;

/// A named fixture directory checked in under `tests/fixtures/`.
pub struct Fixture {
    name: String,
}

impl Fixture {
    /// Names the checked-in fixture directory `tests/fixtures/<name>/`.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
        }
    }

    /// Copies the fixture into a fresh temporary directory and completes it.
    ///
    /// The returned [`MaterializedWorkspace`] owns the temporary directory, so
    /// the workspace lives for as long as the value does.
    ///
    /// # Panics
    ///
    /// Panics when the fixture directory does not exist, because a missing
    /// fixture is an authoring error rather than a condition under test.
    pub fn materialize(&self) -> MaterializedWorkspace {
        let source = camino::Utf8PathBuf::from_path_buf(fixture_root().join(&self.name))
            .expect("fixture path is not UTF-8");

        assert!(
            source.is_dir(),
            "fixture `{}` is missing at `{}`",
            self.name,
            source.as_str()
        );

        let directory = tempfile::TempDir::new().expect("temporary directory");
        let root = camino::Utf8PathBuf::from_path_buf(directory.path().to_path_buf())
            .expect("temporary directory path is not UTF-8");

        copy_tree(&source, &root);
        complete_workspace(&root);

        MaterializedWorkspace { directory, root }
    }
}

/// A completed workspace living in a temporary directory.
///
/// Holds the [`tempfile::TempDir`] that backs [`root`](Self::root), so the
/// directory is removed only when this value is dropped.
pub struct MaterializedWorkspace {
    directory: tempfile::TempDir,
    root: camino::Utf8PathBuf,
}

impl MaterializedWorkspace {
    /// The workspace root inside the temporary directory.
    pub fn root(&self) -> &camino::Utf8Path {
        &self.root
    }

    /// The path of the workspace's root `Cargo.toml`.
    pub fn manifest_path(&self) -> camino::Utf8PathBuf {
        self.root.join("Cargo.toml")
    }
}

/// Fills in the files a cargo workspace needs but its manifests omit.
///
/// Applies to the workspace root manifest and to every package manifest
/// reachable from it. Existing files are never overwritten and directory
/// creation is idempotent, so a fixture that deliberately provides a source
/// file or a member manifest is left exactly as authored.
pub fn complete_workspace(root: &camino::Utf8Path) {
    let root_manifest = root.join("Cargo.toml");
    let names = discover_package_names(root, &root_manifest);

    for name in &names {
        let package_root = root.join(name);
        std::fs::create_dir_all(&package_root).expect("member directory");

        let manifest_path = package_root.join("Cargo.toml");
        if !manifest_path.exists() {
            std::fs::write(&manifest_path, member_manifest(name)).expect("member manifest");
        }
    }

    let mut manifests = vec![root_manifest];

    for name in &names {
        manifests.push(root.join(name).join("Cargo.toml"));
    }

    for manifest_path in manifests {
        complete_package_sources(&manifest_path);
    }
}

/// Returns the package directories under `root`.
///
/// A manifest with a `[package]` table names its own directory. A virtual
/// manifest names its members under `[workspace].members`; a member with no
/// directory yet is still reported so completion can create it.
fn discover_package_names(
    root: &camino::Utf8Path,
    manifest_path: &camino::Utf8Path,
) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(manifest_path) else {
        return Vec::new();
    };

    let Ok(document) = text.parse::<toml_edit::DocumentMut>() else {
        return Vec::new();
    };

    if document.get("package").is_some() {
        return vec![String::new()];
    }

    let mut names = Vec::new();

    if let Some(members) = document
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(|members| members.as_array())
    {
        for member in members.iter().filter_map(|value| value.as_str()) {
            let relative = member.trim_matches('/');

            let package_root = if member.starts_with('/') {
                camino::Utf8PathBuf::from(relative)
            } else {
                root.join(relative)
            };

            let manifest = package_root.join("Cargo.toml");

            if manifest.exists() {
                if let Some(package) = std::fs::read_to_string(&manifest)
                    .ok()
                    .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
                    .and_then(|document| {
                        document
                            .get("package")
                            .and_then(|package| package.get("name"))
                            .and_then(|name| name.as_str().map(str::to_owned))
                    })
                {
                    names.push(package);
                    continue;
                }
            }

            names.push(member.trim_matches('/').to_owned());
        }
    }

    names
}

/// Creates the source files the package manifest at `manifest_path` implies.
///
/// A `[lib]` table or the absence of both `[[bin]]` and `[lib]` implies a
/// library target; a `[[bin]]` table or the absence of both implies a binary
/// target. Existing files are never overwritten.
fn complete_package_sources(manifest_path: &camino::Utf8Path) {
    let Ok(text) = std::fs::read_to_string(manifest_path) else {
        return;
    };

    let Ok(document) = text.parse::<toml_edit::DocumentMut>() else {
        return;
    };

    let Some(package_root) = manifest_path.parent() else {
        return;
    };

    let library = document.get("lib");
    let binary = document.get("bin");

    if library.is_some() || binary.is_none() {
        create_if_absent(&package_root.join("src").join("lib.rs"), "");
    }

    if binary.is_some() || library.is_none() {
        create_if_absent(&package_root.join("src").join("main.rs"), "fn main() {}\n");
    }
}

/// Writes `contents` to `path` only when no file is already there.
fn create_if_absent(path: &camino::Utf8Path, contents: &str) {
    if path.exists() {
        return;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("source directory");
    }

    std::fs::write(path, contents).expect("source file");
}

/// Recursively copies `source` into `destination`.
///
/// Files are copied to their same relative path. Symbolic links are copied by
/// value: the target's contents are read and written as a regular file.
fn copy_tree(source: &camino::Utf8Path, destination: &camino::Utf8Path) {
    std::fs::create_dir_all(destination).expect("destination directory");

    let entries = std::fs::read_dir(source).expect("read fixture directory");

    for entry in entries {
        let entry = entry.expect("fixture directory entry");
        let name = entry.file_name();
        let target = destination.join(name.to_string_lossy().as_ref());
        let file_type = entry.file_type().expect("fixture entry type");

        if file_type.is_dir() {
            let source_child = source.join(name.to_string_lossy().as_ref());

            copy_tree(&source_child, &target);
        } else {
            std::fs::copy(entry.path(), target.as_std_path()).expect("copy fixture file");
        }
    }
}

/// The manifest text for a workspace member created from a bare path.
fn member_manifest(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n")
}

/// The repository's `tests/fixtures/` directory.
///
/// Located relative to this crate's manifest directory so materialization is
/// independent of the directory the test runner is invoked from.
fn fixture_root() -> &'static Path {
    static FIXTURE_ROOT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

    FIXTURE_ROOT.get_or_init(|| {
        let crate_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        crate_root.join("tests").join("fixtures")
    })
}
