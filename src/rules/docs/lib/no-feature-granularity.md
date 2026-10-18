# `lib/no-feature-granularity`

A library is compiled into every binary that links it, and each direct
dependency it declares is compiled along with it. When the library declares no
features, a consumer cannot switch any of those dependencies off: the whole set
is built whether or not the consumer uses the code that needs it. Gating
dependencies behind optional features lets a consumer take only the parts it
uses.

For each selected package that produces no linked artifact, the rule counts the
direct dependencies of kind normal, target-specific ones included, and reads the
package's feature map from `cargo metadata`. That map lists the implicit feature
of every optional dependency, so a package whose only features are implicit ones
counts as having features and does not fire. The rule fires for a package with
at least ten such dependencies and an empty feature map.

## Tradeoff

Gating dependencies behind features adds conditional compilation and a feature
matrix to test. Each feature is also public API, since removing one later breaks
the consumers that name it. The finding is report-only because choosing which
dependencies deserve a feature depends on how the library is used.

## Scope

The rule applies to selected packages that produce no linked artifact, and a
binary, a `cdylib` or a `staticlib` is quiet. Development and build dependencies
are not counted, because they are not compiled into a consumer. A library with
fewer than ten direct normal dependencies is quiet whether or not it declares
features. The finding carries no location, because the absence of a `[features]`
table has no key to point at.
