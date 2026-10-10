//! End-to-end tests of the `check` command's rendered report and exit codes.
//!
//! Every case drives the built binary rather than the library so that the
//! argument grammar, the error-to-exit-code mapping and the rendered header are
//! exercised as a user invokes them.

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

/// A command for the built `cargo-zempic` binary.
fn binary() -> Command {
    Command::cargo_bin("cargo-zempic").expect("the binary is built")
}

/// The headers the human renderer emits, in the order it emits them.
fn header(label: &str) -> String {
    format!("{label:>14}")
}

#[test]
fn both_invocation_forms_produce_identical_output() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let manifest = workspace.manifest_path();

    let injected = binary()
        .args(["zempic", "check", "--manifest-path", manifest.as_str()])
        .output()
        .expect("the injected form runs");
    let direct = binary()
        .args(["check", "--manifest-path", manifest.as_str()])
        .output()
        .expect("the direct form runs");

    assert!(injected.status.success(), "injected form: {:?}", injected);
    assert!(direct.status.success(), "direct form: {:?}", direct);
    assert_eq!(injected.stdout, direct.stdout);
}

#[test]
fn bare_invocation_exits_two_with_usage() {
    binary()
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn rejected_flag_exits_two_with_the_specific_diagnostic() {
    binary()
        .args(["check", "--bin", "foo"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("error: `--bin` is not supported"));
}

#[test]
fn check_on_a_fixture_reports_the_workspace_and_profile() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let host = host_target_triple();

    let assert = binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
        ])
        .assert()
        .code(0);

    assert.stdout(
        predicate::str::contains(format!(
            "{} {}",
            header("workspace:"),
            workspace.root().as_str()
        ))
        .and(predicate::str::contains("(2 members, 2 selected)"))
        .and(predicate::str::contains(format!(
            "{} {}",
            header("profile:"),
            "release"
        )))
        .and(predicate::str::contains("optimizing for: size"))
        .and(predicate::str::contains(format!(
            "{} {}",
            header("target:"),
            host
        ))),
    );
}

/// The host target triple as `rustc -vV` reports it.
///
/// The binary resolves for the host when no `--target` is given, so the case
/// asserts against the machine's own triple rather than a fixed architecture.
fn host_target_triple() -> String {
    let output = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("rustc runs");
    let stdout = String::from_utf8(output.stdout).expect("rustc output is UTF-8");

    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("rustc reports a host triple")
        .to_owned()
}

#[test]
fn check_reports_the_inherits_chain_of_a_custom_profile() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let root_manifest = concat!(
        "[workspace]\n",
        "members = [\"alpha\", \"beta\"]\n\n",
        "[profile.small]\n",
        "inherits = \"release\"\n",
    );
    std::fs::write(workspace.root().join("Cargo.toml"), root_manifest).unwrap();

    let assert = binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
            "--profile",
            "small",
        ])
        .assert()
        .code(0);

    assert.stdout(
        predicate::str::contains(format!("{} {}", header("profile:"), "small"))
            .and(predicate::str::contains("(inherits: release)")),
    );
}

#[test]
fn unparseable_manifest_exits_two_naming_the_file() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let root_manifest = workspace.root().join("Cargo.toml");
    std::fs::write(&root_manifest, "not valid toml =").unwrap();

    binary()
        .args(["check", "--manifest-path", root_manifest.as_str()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Cargo.toml"));
}

#[test]
fn unknown_package_exits_two() {
    let workspace = common::Fixture::new("multi-member").materialize();

    binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
            "--package",
            "nonexistent",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("nonexistent"));
}

#[test]
fn output_contains_no_size_figure() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let output = binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
        ])
        .output()
        .expect("the command runs");

    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");

    // The forbidden literals include a `%` and byte-unit suffixes so a numeric
    // magnitude would fail the case even when phrased as a size.
    for (name, text) in [("stdout", &stdout), ("stderr", &stderr)] {
        for forbidden in ["%", "KB", "KiB", "MB", "MiB", "GB", "GiB", "bytes"] {
            assert!(
                !text.contains(forbidden),
                "{name} must not contain `{forbidden}`: {text}"
            );
        }
    }
}

#[test]
fn gating_arguments_are_accepted() {
    let workspace = common::Fixture::new("multi-member").materialize();

    binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
            "--ignore",
            "profile/lto",
            "--only",
            "profile",
            "--no-include-advisory",
        ])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("No findings."));
}

#[test]
fn malformed_selector_exits_two_naming_the_value() {
    binary()
        .args(["check", "--ignore", "Profile/LTO"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Profile/LTO"));
}

#[test]
fn check_reports_that_there_are_no_findings() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let assert = binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
        ])
        .assert()
        .code(0);

    assert.stdout(
        predicate::str::contains("No findings.")
            .and(predicate::str::contains("%").not())
            .and(predicate::str::contains("KB").not())
            .and(predicate::str::contains("KiB").not())
            .and(predicate::str::contains("MB").not())
            .and(predicate::str::contains("MiB").not())
            .and(predicate::str::contains("GB").not())
            .and(predicate::str::contains("GiB").not())
            .and(predicate::str::contains("bytes").not()),
    );
}

#[test]
fn expired_ignore_warns_on_stderr_and_still_exits_zero() {
    let workspace = common::Fixture::new("multi-member").materialize();

    std::fs::write(
        workspace.root().join("zempic.toml"),
        "schema_version = 1\n\n[[ignore]]\nrule = \"profile/lto\"\nexpires = \"2020-01-01\"\n",
    )
    .unwrap();

    binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
        ])
        .assert()
        .code(0)
        .stderr(
            predicate::str::contains("profile/lto")
                .and(predicate::str::contains("expired"))
                .and(predicate::str::contains("reason")),
        );
}

#[test]
fn invalid_configuration_exits_two_naming_the_file() {
    let workspace = common::Fixture::new("multi-member").materialize();

    std::fs::write(
        workspace.root().join("zempic.toml"),
        "schema_version = 1\n\n[analysis]\nnonsense = true\n",
    )
    .unwrap();

    binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("zempic.toml").and(predicate::str::contains("nonsense")));
}
