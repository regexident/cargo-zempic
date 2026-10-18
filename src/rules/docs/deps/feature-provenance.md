# `deps/feature-provenance`

Cargo enables a feature on a dependency when any requester in the graph asks for
it, so a feature can be compiled into the artifact without anything in the
workspace having asked for it. When a workspace member declares a dependency
and a second dependency in the graph requests an extra feature of the same
crate, the member's build carries that feature although neither its manifest nor
its own features mention it.

For each direct normal dependency of a selected package, the rule compares two
sets. The enabled set is the feature list the resolved graph reports for the
dependency's package. The requested set is what the workspace members ask for:
every member's direct declaration of that package contributes `default` when it
uses default features and the features it lists, and a member feature that
forwards a feature with `name/feature` or `name?/feature` contributes that
feature once the member feature is enabled. Each request is expanded through
the dependency's feature table, so a requested feature also covers the features
and optional dependencies it enables. Enabled features outside the requested set
are reported once per dependency, and the detail lists them.

The verdict is capped at medium confidence. Cargo's resolver version 2 keeps
the features of normal, build and dev dependencies apart, and `cargo metadata`
does not reproduce that separation: it reports one unified feature set. The
requester behind a reported feature can therefore be a build script or a test
only, in which case the feature never reaches the artifact. The detail says so
in a closing sentence.

## Tradeoff

Removing a feature that only another dependency requests means dropping or
replacing that dependency, or changing the way it is declared. The dependency
may provide functionality the code relies on. The saving is the code behind the
feature, and the cost is that change, which the rule leaves to the author
because the rule cannot name the requester.

## Scope

The rule reads the direct normal dependencies of each selected package,
including target-specific ones, and ignores dev and build dependencies. A
dependency the resolved graph does not reach for the target platform is quiet,
as is one whose enabled features every workspace member requests. The finding
points at the dependency's key in the `[dependencies]` table, and a
target-specific dependency carries no location. The rule is not gated on which
artifacts the workspace produces.
