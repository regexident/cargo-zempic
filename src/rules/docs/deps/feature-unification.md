# `deps/feature-unification`

Cargo enables a feature on a dependency when any requester in the graph asks for
it. A package that declares a dependency with `default-features = false` opts
out of that dependency's default features, but the opt-out only holds when no
other declaration of the same dependency requests them. When another workspace
member, or a dependency reached through the graph, declares it with default
features, cargo unifies the two and the default features are compiled and linked
anyway.

The rule inspects each direct normal dependency of a selected package that is
declared with `default-features = false`, and fires when the resolved graph
lists `default` among the features enabled on the resolved package. The detail
names the workspace members whose own declaration of that package requests the
default features. When no member does, the detail says that a dependency outside
the members' own declarations requested them.

The verdict is capped at medium confidence. Cargo's resolver version 2 keeps
the features of normal, build and dev dependencies apart, and `cargo metadata`
does not reproduce that separation: it reports one unified feature set. The
requester that enables `default` can therefore be a build script or a test
only, in which case the defaults never reach the artifact. The detail says so in
a closing sentence.

## Tradeoff

Restoring the opt-out means changing the other declaration that requests the
default features. That declaration belongs to a crate that may rely on the
functionality the defaults provide, so removing them can break it. The saving is
the code behind the default features, and the cost is the change to the
requesting declaration, which the rule leaves to the author.

## Scope

The rule reads the direct normal dependencies of each selected package,
including target-specific ones, and ignores dev and build dependencies. A
dependency declared with default features is quiet, as is one the resolved graph
does not reach for the target platform. The finding points at the dependency's
key in the `[dependencies]` table of the opting-out package, and a
target-specific dependency carries no location. The finding is reported on the
package that opted out, never on the package that requested the defaults. The
rule is not gated on which artifacts the workspace produces.
