# `deps/default-features`

A dependency declared without `default-features = false` receives the features
its manifest lists under `default`. Those features can enable optional
dependencies of the dependency, directly with `dep:name`, through
`name/feature`, through the implicit feature an optional dependency creates, or
through any chain of other features. Every optional dependency enabled this way
is compiled and linked into the artifact whether or not the code uses it.

The rule expands the dependency's `default` feature through its feature table
and fires once per direct dependency of a selected package when the expansion
enables at least one optional dependency. The detail names those optional
dependencies. The weak form `name?/feature` enables nothing by itself, because
it forwards a feature only when `name` is enabled by other means. A default set
that enables only plain features does not fire, because the rule has no
evidence that those features cost anything.

The verdict is capped at medium confidence. Cargo's resolver version 2 keeps
the features of normal, build and dev dependencies apart, and `cargo metadata`
does not reproduce that separation: it reports one unified feature set. The
optional dependency the rule names can therefore be enabled for a build script
or a test only, in which case it never reaches the artifact. The detail says so
in a closing sentence.

## Tradeoff

Setting `default-features = false` removes every default feature, including
ones the code may use. Each one still needed must be re-enabled by name in the
dependency's `features` list. The saving is the optional dependencies and their
code, and the cost is that missing features surface as compile errors until they
are listed.

## Scope

The rule reads the direct normal dependencies of each selected package,
including target-specific ones, and ignores dev and build dependencies. A
dependency already declared with `default-features = false` is quiet, as is one
the resolved graph does not reach for the target platform. The finding points
at the dependency's key in the `[dependencies]` table, and a target-specific
dependency carries no location. The rule is not gated on which artifacts the
workspace produces.
