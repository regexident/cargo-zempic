# `deps/feature-trim`

A dependency declared with its default features enabled pulls in every feature
of its default set. Some of those features add code most users never call, and
a crate's own documentation rarely says which. This rule consults the knowledge
base, a curated list of crates and the default features worth turning off, and
reports each default feature an entry flags when the declaration does not ask
for it.

An entry applies to a resolved version only when the version satisfies the
entry's version range. A dependency resolved outside the range is never
reported, because advice written for one release line says nothing reliable
about another. Each entry also records the newest release its advice was
verified against. When the resolved version is newer, the finding is still
emitted, but it is downgraded to `ReportOnly` and ends with a note naming the
verified release and the one in use, so a stale entry is never offered as an
automatic fix.

An entry marks each feature as `fixable` or not. A feature whose advice cannot
be applied by editing the manifest alone is reported as `ReportOnly` as well,
whatever its version.

A feature the declaration requests by name, directly or through another
requested feature, is wanted and does not fire. A declaration that already sets
`default-features = false` is quiet too.

The embedded knowledge base can be extended or overridden from `zempic.toml`
under `[knowledge_base.crates.<name>]`. A configured entry replaces the
embedded entry of the same name entirely, including its version range, so an
override whose range excludes the resolved version silences the crate.

## Tradeoff

Disabling defaults removes every default feature, not only the one reported.
Each default feature the code still needs must be re-enabled by name, and a
feature that is missing shows up as a compile error or, for behavior selected
at runtime, as a changed result. The magnitude of a finding is the entry's
estimate for the one feature it names.

## Scope

The rule reads the normal dependencies of the selected packages that declare
default features and resolve to a package for the analyzed target. Build and
development dependencies are not examined, and neither are dependencies that
the resolved target platform does not use. Only crates the knowledge base holds
an entry for can produce a finding.
