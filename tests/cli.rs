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

/// The rendered `check` report as text.
fn report(args: &[&str]) -> String {
    let output = binary().args(args).output().expect("the command runs");

    assert!(output.status.success(), "the run failed: {output:?}");

    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// The value `text`'s header line for `label` carries after the label and its
/// padding.
fn header_value<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(label))
        .map(str::trim_start)
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
        .stderr(predicate::str::starts_with("`--bin` is not supported"));
}

#[test]
fn check_on_a_fixture_reports_the_workspace_and_profile() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let host = host_target_triple();

    let text = report(&[
        "check",
        "--manifest-path",
        workspace.manifest_path().as_str(),
    ]);

    for (label, expected) in [
        ("  workspace: ", workspace.root().as_str()),
        ("  profile: ", "release"),
        ("  optimizing for: ", "size"),
        ("  target: ", host.as_str()),
    ] {
        let value = header_value(&text, label)
            .unwrap_or_else(|| panic!("the `{label}` header line is missing: {text}"));

        assert!(
            value.starts_with(expected),
            "the `{label}` line must carry `{expected}` after the label: {value:?}"
        );
    }
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

    let text = report(&[
        "check",
        "--manifest-path",
        workspace.manifest_path().as_str(),
        "--profile",
        "small",
    ]);

    let value = header_value(&text, "  profile: ")
        .unwrap_or_else(|| panic!("the `profile:` header line is missing: {text}"));

    assert!(value.starts_with("small"), "{value:?}");
    assert!(text.contains("(inherits: release)"), "{text}");
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
    // `bare-release` fires the most rules, so it is where a §2 violation in a
    // rule's prose would appear; `multi-member` covers the empty-report path.
    for fixture in ["bare-release", "multi-member"] {
        let workspace = common::Fixture::new(fixture).materialize();
        let manifest = workspace.manifest_path();

        // Every renderer's output is scanned: the human form, the JSON document
        // and the Markdown document all carry the numbers policy.
        for (label, args) in [
            (
                "human",
                vec![
                    "check",
                    "--manifest-path",
                    manifest.as_str(),
                    "--format",
                    "human",
                    "--color",
                    "never",
                ],
            ),
            (
                "json",
                vec![
                    "check",
                    "--manifest-path",
                    manifest.as_str(),
                    "--format",
                    "json",
                ],
            ),
            (
                "markdown",
                vec![
                    "check",
                    "--manifest-path",
                    manifest.as_str(),
                    "--format",
                    "markdown",
                ],
            ),
        ] {
            let output = binary().args(&args).output().expect("the command runs");

            assert!(output.status.success(), "{label} run failed: {output:?}");

            let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
            let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");

            common::assert_no_size_figures(&format!("{label} stdout"), &stdout);
            common::assert_no_size_figures(&format!("{label} stderr"), &stderr);
        }
    }
}

#[test]
fn rule_documentation_carries_no_size_figure() {
    let docs = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("rules")
        .join("docs");

    // The directory appears with the first rule, so its absence is a pass.
    let Ok(entries) = std::fs::read_dir(&docs) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }

        let text = std::fs::read_to_string(&path).expect("the doc file is readable");

        common::assert_no_size_figures(&path.display().to_string(), &text);
    }
}

#[test]
fn gating_arguments_are_accepted() {
    let workspace = common::Fixture::new("fully-optimized").materialize();

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
    let workspace = common::Fixture::new("fully-optimized").materialize();

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

#[test]
fn unknown_output_argument_values_exit_two_naming_the_value() {
    for (flag, value) in [
        ("--format", "nonsense"),
        ("--fail-on", "critical"),
        ("--color", "sometimes"),
    ] {
        binary()
            .args(["check", flag, value])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(value));
    }
}

#[test]
fn format_argument_selects_the_renderer() {
    let workspace = common::Fixture::new("multi-member").materialize();
    let manifest = workspace.manifest_path();

    let json = binary()
        .args([
            "check",
            "--manifest-path",
            manifest.as_str(),
            "--format",
            "json",
        ])
        .output()
        .expect("the json run succeeds");
    let json = String::from_utf8(json.stdout).expect("stdout is UTF-8");

    assert!(
        json.trim_start().starts_with('{') && json.contains("\"schema_version\": 1"),
        "`--format json` must emit JSON: {json:?}"
    );

    let markdown = binary()
        .args([
            "check",
            "--manifest-path",
            manifest.as_str(),
            "--format",
            "markdown",
        ])
        .output()
        .expect("the markdown run succeeds");
    let markdown = String::from_utf8(markdown.stdout).expect("stdout is UTF-8");

    assert!(
        markdown.trim_start().starts_with("# cargo-zempic"),
        "`--format markdown` must emit a Markdown document: {markdown:?}"
    );
    assert!(markdown.contains("ordinal judgement"), "{markdown:?}");
}

#[test]
fn color_never_emits_no_escape_sequence() {
    let workspace = common::Fixture::new("multi-member").materialize();

    let output = binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
            "--color",
            "never",
        ])
        .output()
        .expect("the run succeeds");

    assert!(
        !output.stdout.contains(&0x1b),
        "`--color never` must emit no escape sequence"
    );
}

#[test]
fn configured_format_is_honored_and_overridden_by_the_argument() {
    let workspace = common::Fixture::new("fully-optimized").materialize();

    std::fs::write(
        workspace.root().join("zempic.toml"),
        "schema_version = 1\n\n[report]\nformat = \"json\"\n",
    )
    .unwrap();

    // The configured format must be honored by a bare run, so the effective
    // format is resolved from the file under a command line that names none.
    let configured = binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
        ])
        .output()
        .expect("the configured run succeeds");
    let configured = String::from_utf8(configured.stdout).expect("stdout is UTF-8");

    assert!(
        configured.trim_start().starts_with('{') && configured.contains("\"schema_version\""),
        "the configured format must be JSON: {configured:?}"
    );

    binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
            "--format",
            "human",
        ])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("No findings."));
}

#[test]
fn fail_on_never_is_the_default() {
    let workspace = common::Fixture::new("bare-release").materialize();

    // `bare-release` reports high-severity findings and has no `zempic.toml`,
    // so a zero exit without `--fail-on` shows that no threshold applies by
    // default.
    binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
        ])
        .assert()
        .code(0);

    // The same invocation must gate under an explicit threshold, which proves
    // the fixture would fail a run if a default threshold were applied.
    binary()
        .args([
            "check",
            "--manifest-path",
            workspace.manifest_path().as_str(),
            "--fail-on",
            "high",
        ])
        .assert()
        .code(1);
}

#[test]
fn configured_profile_is_honored_and_overridden_by_the_argument() {
    let workspace = common::Fixture::new("multi-member").materialize();

    // A declared non-built-in profile must carry `inherits`, so the fixture
    // gains a table the base fixture lacks by writing into the materialized
    // workspace rather than checking in another directory.
    let root_manifest = concat!(
        "[workspace]\n",
        "members = [\"alpha\", \"beta\"]\n\n",
        "[profile.custom]\n",
        "inherits = \"release\"\n",
    );
    std::fs::write(workspace.root().join("Cargo.toml"), root_manifest).unwrap();
    std::fs::write(
        workspace.root().join("zempic.toml"),
        "schema_version = 1\n\n[analysis]\nprofile = \"custom\"\n",
    )
    .unwrap();

    // A bare run analyzes the configured profile, which only reaches the
    // resolution if `--profile` carries no default.
    let configured = report(&[
        "check",
        "--manifest-path",
        workspace.manifest_path().as_str(),
    ]);
    let profile = header_value(&configured, "  profile: ")
        .unwrap_or_else(|| panic!("the `profile:` header line is missing: {configured}"));

    assert!(profile.starts_with("custom"), "{profile:?}");

    // The argument merges over the configured profile.
    let overridden = report(&[
        "check",
        "--manifest-path",
        workspace.manifest_path().as_str(),
        "--profile",
        "release",
    ]);
    let profile = header_value(&overridden, "  profile: ")
        .unwrap_or_else(|| panic!("the `profile:` header line is missing: {overridden}"));

    assert!(profile.starts_with("release"), "{profile:?}");
}

#[test]
fn configured_dimensions_are_honored() {
    let workspace = common::Fixture::new("multi-member").materialize();

    std::fs::write(
        workspace.root().join("zempic.toml"),
        "schema_version = 1\n\n[analysis]\noptimize_for = [\"size\"]\n",
    )
    .unwrap();

    let text = report(&[
        "check",
        "--manifest-path",
        workspace.manifest_path().as_str(),
    ]);
    let dimensions = header_value(&text, "  optimizing for: ")
        .unwrap_or_else(|| panic!("the `optimizing for:` header line is missing: {text}"));

    assert!(dimensions.starts_with("size"), "{dimensions:?}");
}
