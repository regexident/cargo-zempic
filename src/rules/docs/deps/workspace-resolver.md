# `deps/workspace-resolver`

Cargo's feature resolver decides which features of a dependency are enabled
when several parts of the graph use it. Resolver `"1"` unifies a dependency's
features across its normal, build and dev uses, so a feature requested only by
a build script, a proc-macro host dependency or a test reaches the dependency
that is linked into the artifact. Resolver `"2"` keeps those kinds apart, and
only features requested by normal dependencies of the built target are enabled
for it.

The effective resolver is the root manifest's `workspace.resolver` when
present, else its `package.resolver`, else the value implied by the root
package's edition: editions 2015 and 2018 imply `"1"`, edition 2021 implies
`"2"` and edition 2024 implies `"3"`. A virtual manifest has no edition, so
without an explicit `workspace.resolver` it resolves to `"1"` whatever the
editions of its members are. Members' editions never select the workspace's
resolver, which is why a virtual manifest needs the key written out.

Resolver `"2"` is the lowest value that separates dependency kinds. Resolver
`"3"` adds only version-selection behavior, preferring versions compatible with
the declared minimum Rust version, and changes nothing about how features
unify, so the rule recommends `"2"`.

## Tradeoff

Feature sets of dependencies shared between normal, build and dev kinds stop
unifying. A build that relied on a feature leaking across kinds can fail to
compile until that feature is requested where it is used. The fix is a
one-line key in the root manifest, and the cost is the compile errors it can
surface, rather than any change at runtime.

## Scope

The rule reads the root manifest only, and fires once per workspace. It is not
gated on which artifacts the workspace produces, because the resolver governs
every package in the workspace. An edition 2021 or later root package with no
explicit key is quiet, as is any root manifest that sets `"2"` or `"3"`.
