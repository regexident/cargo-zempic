# cargo-zempic

`cargo-zempic` is report-only static analysis that recommends cargo and rustc settings which produce a smaller Rust artifact.

## What it does

`zempic` reads three things and nothing else: the workspace manifests, the cargo configuration files, and the dependency graph cargo resolves.

It does not build. It never invokes the compiler to produce an artifact, and it never reads a compiled artifact. Every verdict therefore comes from what the configuration says, not from measuring what came out of the linker. Each finding states an ordinal magnitude (a judgement about how much a change is worth making), and each renderer says as much in its magnitude legend, since there is no measurement behind it.

Recommendations are only ever reported. `zempic` writes no file, edits no
manifest and applies no change; the reader decides what to act on.

## Installation

```plain
cargo install cargo-zempic
```

## Usage

```plain
cargo zempic check
```

Cargo invokes a subcommand as `cargo-zempic zempic check`, injecting its own
name as the first argument, so both spellings produce the same report.

The report groups findings by tier, in the order foundational, compiler, behavioral, dependencies and advanced. Each finding names its rule, severity and magnitude. A right-hand column beside them shows `current -> recommended` for a finding that recommends a setting change, and the file the finding concerns for one that carries no change. The line below carries effort, runtime cost and fixability, followed by the detail and the tradeoff the change carries. After the findings come a section for recommendations that disagree between dimensions, a count summary and the magnitude legend. The JSON document adds two fields the human report does not carry, the package and the confidence of each finding. Configuration notices go to standard error so that a piped report on standard output stays clean; they cover an expired or undocumented ignore, and a rule skipped because cargo would reject the setting in the analyzed profile.

## Options

Every option belongs to `check`.

| Option | Value | Meaning |
| --- | --- | --- |
| `-O`, `--optimize-for` | `DIM` | The optimization objective to answer. `size` is the only implemented dimension and the default when unset; the value may be repeated or comma-separated. |
| `--manifest-path` | `PATH` | The manifest to resolve from, or the current directory's when unset. |
| `-p`, `--package` | `SPEC` | Package specs narrowing the analysis to named workspace members. |
| `--workspace`, `--all` | — | Analyze every workspace member. |
| `--exclude` | `SPEC` | Package specs to omit from a workspace-wide analysis. |
| `--profile` | `NAME` | The cargo profile whose settings govern the analyzed artifacts. Defaults to `release`, or to `[analysis] profile` in `zempic.toml`. |
| `-F`, `--features` | `LIST` | Features to activate during resolution; may be repeated or comma-separated. |
| `--all-features` | — | Activate every feature of every selected package. |
| `--no-default-features` | — | Disable the default feature set. |
| `--target` | `TRIPLE` | The target triple to resolve for, or the host's when unset. |
| `--format` | `FMT` | `human` (default), `json` or `markdown`. Overrides `[report] format`. |
| `--fail-on` | `SEVERITY` | `never` (default), `low`, `medium` or `high`. Overrides `[report] fail_on`. |
| `-q`, `--quiet` | — | Render one line per finding, dropping the header, the tier headings and each finding's detail, tradeoff and metadata line. The conflict blocks, the count summary and the magnitude legend remain. The JSON and Markdown documents have one shape each, so it does not affect them. |
| `--color` | `WHEN` | `auto` (default), `always` or `never`. `auto` honors `NO_COLOR` and `CLICOLOR` and otherwise follows whether standard output is a terminal. |
| `--ignore` | `SELECTOR` | A selector whose matching rules are suppressed. |
| `--only` | `SELECTOR` | Restrict the run to matching rules. A rule named here is an explicit request, so it is reported regardless of `--ignore` and regardless of `--no-include-advisory`. |
| `--include-advisory` | — | Include advisory and report-only findings. |
| `--no-include-advisory` | — | Suppress advisory notes and report-only findings. |
| `--allow-nightly` | — | Raise nightly-only findings from report-only to consequential. |

`--ignore`, `--only`, `--include-advisory`, `--no-include-advisory` and `--allow-nightly` override the corresponding `zempic.toml` setting for one run, and an unstated one leaves the file's value in force.

A selector names rules by path, such as `profile/lto`, `profile` or a bare `deps`, and may carry a dimension qualifier.

A nightly finding reports its fixability as `report-only` until `--allow-nightly` is given, and as `consequential` from then on. The advisory gate never hides such a finding either way, since report-only describes an acceptance the reader has not yet given rather than advisory content; `--no-include-advisory` therefore leaves nightly findings visible.

Cargo's target-selection flags (`--bin`, `--bins`, `--example`, `--examples`, `--bench`, `--benches`, `--test`, `--tests` and `--all-targets`) are rejected with a diagnostic, because a setting applies per package and per profile rather than per target. Narrow an analysis with `--package` instead.

## Configuration

`zempic.toml` is read from the workspace root, then from each selected package directory, each file merged over the ones before it. `.zempic.toml` is used when `zempic.toml` is absent. An absent file is not an error; an unreadable, unparseable or wrongly versioned one is.

Unknown keys are rejected, so a misspelt setting fails loudly rather than being silently ignored.

```toml
schema_version = 1

[analysis]
optimize_for = ["size"]
profile = "release"
include_advisory = true
allow_nightly = false

[report]
format = "human"
fail_on = "never"

[[ignore]]
rule = "profile/lto"
reason = "link times in CI exceed our budget"
packages = ["*"]
expires = "2026-12-31"

[knowledge_base.crates.internal-http]
versions = ">=1.0, <3.0"
verified_through = "2.4.1"
magnitude = "moderate"
dev_only = false

[[knowledge_base.crates.internal-http.features]]
name = "telemetry"
default = true
magnitude = "moderate"
advice = "disable unless the service exports metrics"
fixable = true
```

`format` accepts `human`, `json` and `markdown`; `fail_on` accepts `never`, `low`, `medium` and `high`; `optimize_for` currently accepts only `size`. The parser recognizes these values and no others: an unrecognized one is a configuration error, which exits `2` rather than drawing a warning.

An `[[ignore]]` entry takes `rule`, and optionally `reason`, `packages` and `expires`. An ignore without a reason draws a notice; an ignore past its `expires` date stops suppressing and draws a notice. `packages` scopes an ignore to the named packages, with `"*"` matching every package.

A `[knowledge_base.crates.<name>]` entry extends the embedded knowledge base, or replaces the entry of that crate name entirely. It takes `versions` and a `verified_through` version that must satisfy it, and optionally `magnitude`, `dev_only` and `features`; each feature takes `name`, `magnitude` and `advice`, and optionally `default` and `fixable`. Advice is matched against the resolved version, so advice written for one release line is never applied to another.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | The analysis completed and no finding met the `--fail-on` threshold. |
| `1` | The analysis completed and at least one finding was at or above the threshold. The full report is still printed: the exit code is a signal, not a substitute for output. |
| `2` | Any failure to produce a report. Representative causes are unparseable arguments, a rejected target-selection flag, an ignored or member selection error, an unparseable manifest, a failed metadata resolution, a present but unreadable or invalid `zempic.toml`, a rule violated at assembly time (for example a finding that states no tradeoff), and a failure to render the report. |

An `Info` finding never gates at any threshold: an informational note must not break a pipeline.

## Limitations

**Report-only.** `zempic` recommends and never applies. There is no command that edits a manifest or a configuration file.

**No size figures are reported.** `zempic` does not build and does not read compiled artifacts, so it reports no measurement of an artifact. Magnitude is an ordinal judgement, not a measurement, and each renderer states this in its legend.

**Nightly findings are hidden on non-nightly toolchains.** The remedies they describe rest on unstable compiler flags, so they are only reported for a nightly channel. The channel is read from the workspace's `rust-toolchain.toml` when that file names one, and otherwise from the `rustc` on `PATH`; an unrecognized channel is treated as stable. Where they are reported, they stay `report-only` until `--allow-nightly` accepts them.

**Feature rules cap confidence.** Cargo's feature resolver keeps the features of normal, build and dev dependencies apart, while `cargo metadata` unifies those features and does not reproduce the separation. The dependency-kind rules `deps/default-features`, `deps/feature-unification`, `deps/feature-provenance` and `deps/feature-trim` may therefore concern a feature that only a build script or a test reaches, so their findings are capped below full confidence and say so in their detail.

## Rule reference

Every rule, with the settings it concerns, the tradeoff the change carries and the references behind it, is documented in [`docs/rules.md`](docs/rules.md).
