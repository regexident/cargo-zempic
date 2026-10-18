# `deps/build-only-note`

Build scripts and procedural macros run while the package compiles. Cargo
builds them, and everything they depend on, for the host machine rather than
for the target the artifact runs on. A crate reached only through such an edge
is part of the build and not of the artifact: it affects compile time, but
removing or trimming it does not make the artifact smaller or faster.

This rule consults the knowledge base for crates marked heavy, meaning a
crate-level magnitude of moderate or large. For each selected package that
produces a linked artifact, it walks the resolved dependency graph and tracks
whether each crate is reached through a build dependency or a procedural macro.
When at least one heavy crate is reached and every heavy crate is reached that
way, the rule reports them together in one note. When the resolved version is
newer than the release an entry was verified against, the note ends with a
statement naming both releases.

The note reports a non-finding. It recommends no change, and instead tells the
reader which heavy dependencies are not worth optimizing for the sake of the
artifact.

## Tradeoff

None, since the note points optimization effort away from dependencies that do
not reach the artifact.

## Scope

The rule follows normal and build dependencies of packages that produce a
linked artifact. Development dependencies are not followed, since they are not
part of the artifact. A heavy crate that is reached both through a host path and
through a target path keeps the note silent, because it does reach the artifact.
Packages that produce no linked artifact are not examined.
