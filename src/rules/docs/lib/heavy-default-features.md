# `lib/heavy-default-features`

A library's `default` feature set is paid by every consumer that does not opt
out of it. When that set enables an optional dependency that is costly to
compile, each binary that links the library compiles the dependency too,
whether or not the binary uses the code that needs it. The library's author
does not see that cost, because the dependency is optional in the library's own
manifest.

For each selected package that produces no linked artifact, the rule expands
the package's `default` feature through its feature table, following `dep:`
references, forwarded features and implicit optional-dependency features. Each
optional normal dependency the expansion enables is resolved to a package, and
the knowledge base is consulted for an entry matching the resolved version. A
dependency is heavy when its entry records a moderate or large crate-level
magnitude, and the finding carries that magnitude. A dependency without an
entry, without a magnitude, or with a small one does not fire.

The embedded knowledge base can be extended or overridden from `zempic.toml`
under `[knowledge_base.crates.<name>]`. When the resolved version is newer than
the release the entry was verified against, the finding ends with a note naming
both releases.

An optional dependency that the analyzed feature selection and target do not
resolve does not fire either, since its version is unknown and no entry can be
matched to it.

## Tradeoff

Removing the dependency from the default set is a breaking change for consumers
that rely on it without naming the feature: their build either fails to
compile or silently loses the behavior the dependency provided. The finding is
report-only, because moving a dependency out of the defaults is a decision
about the library's public interface.

## Scope

The rule applies to selected packages that produce no linked artifact, and a
binary, a `cdylib` or a `staticlib` is quiet. Only optional dependencies of
kind normal are examined; build and development dependencies are not. Only
crates the knowledge base holds a heavy entry for can produce a finding, and
the rule emits one finding per heavy dependency, located at the package's
`features.default` key.
