# `config/rustflags-shadowed`

Cargo resolves `rustflags` from exactly one of three ranks, in this order:

1. The `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` environment variable, present
   even when empty.
2. Every `target.<triple>.rustflags` table matching the triple being built,
   concatenated with every `target.'cfg(...)'.rustflags` table found anywhere
   in the configuration chain.
3. The `build.rustflags` key.

Cargo never merges flags across these ranks. It takes the first one present in
the list above and ignores the rest outright, including every entry a lower
rank declares. Appending a flag to `build.rustflags` while a `target.*` table
is present therefore has no effect at all: cargo never reads it, so the flag
never reaches the compiler regardless of how it is spelled or how many flags
sit beside it.

A `config.toml` is read bottom-up relative to the package directory: the
nearest file's keys win over a more distant one's for the same rank, and the
walk ends at `$CARGO_HOME/config.toml`. A rank can therefore be set in one
file and shadowed by another rank set in a different file entirely, and the
finding still fires, because what matters is which rank wins across the whole
chain rather than which single file is read.

## Unevaluated `cfg` predicates

This tool evaluates no `cfg(...)` predicate. A `target.'cfg(windows)'.rustflags`
table counts as present rank-two content on every triple, whether or not the
predicate would actually hold for the triple being built. When such a table is
among the entries contributing to the winning rank, the verdict that a lower
rank is shadowed still holds, since a live `cfg` table necessarily outranks
`build.rustflags` regardless of what it evaluates to. The finding is reported at
medium rather than high confidence, because the condition proving dominance is
one this tool did not actually check.

## Scope limits on `target.*` rustflags

A `target.<triple>.rustflags` or `target.'cfg(...)'.rustflags` table does not
apply to a build script or a procedural macro, both of which build for the
host machine rather than for the triple under analysis, unless
`target-applies-to-host` and `host-config` are both set to opt into that
behavior. A shadowed finding concerns only the triple cargo resolves the
package's linked artifact for, not every artifact cargo builds along the way.

## Tradeoff

Removing or relocating the inert entry changes the artifact not at all, since
cargo was already ignoring it, which is why the finding carries no magnitude.
What the finding corrects is the belief that the inert flags are taking effect.
Any change to the rustflags that do take effect invalidates the whole build
cache, because rustc treats a rustflags change as invalidating every cached
artifact rather than only the ones the flag affects, so acting on this finding
costs a full rebuild.

## No single remedy

The rule recommends no particular fix, because the three outcomes are equally
defensible and only the user knows which one matches their intent: move the
shadowed flags into the rank that wins so they take effect; remove the inert
entry so the configuration states only what actually happens; or leave both as
they are, if the inert entry documents an alternative the user wants kept
nearby for later. `config/rustflags-shadowed` is fixed at `ReportOnly` for this
reason, since there is no safe default to apply automatically.
