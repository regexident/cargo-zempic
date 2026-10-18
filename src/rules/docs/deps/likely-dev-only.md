# `deps/likely-dev-only`

Test frameworks, benchmark harnesses and snapshot tools exist to support
development. A crate of that kind declared under `[dependencies]` is compiled
into every build of the package, shipped or not, although shipped code almost
never calls it. This rule consults the knowledge base, which marks such crates
as `dev_only`, and reports each one a selected package declares as a normal
dependency.

The embedded knowledge base marks `criterion`, `proptest`, `insta`,
`quickcheck`, `pretty_assertions`, `mockall` and `rstest` this way. The list can
be extended or overridden from `zempic.toml` under
`[knowledge_base.crates.<name>]` with `dev_only = true`, and a configured entry
replaces the embedded entry of the same name entirely.

An entry applies to a resolved version only when the version satisfies the
entry's version range. A dependency resolved outside the range is never
reported. When the resolved version is newer than the release the entry was
verified against, the finding ends with a note naming both releases.

The finding is medium-confidence and report-only. The rule cannot see source,
so it cannot tell a harness used only by tests from one that shipped code
depends on, and the fix is never applied automatically.

## Tradeoff

Moving the crate to `[dev-dependencies]` removes it from normal builds. Any
code outside tests, benchmarks and examples that uses it stops compiling until
that code is gated behind a feature or moved. A crate that is genuinely part of
the shipped functionality is a false positive and can be silenced by
overriding its entry in `zempic.toml`.

## Scope

The rule reads the normal dependencies of the selected packages that resolve to
a package for the analyzed target. Development and build dependencies are not
examined, and neither are dependencies that the resolved target platform does
not use. Only crates the knowledge base marks `dev_only` can produce a finding.
