# `deps/duplicate-versions`

Cargo resolves a crate to one version per semver-compatible range. Two
versions are compatible when their leftmost non-zero component is equal: the
major version for `1.x` and later, the minor version for `0.x` releases and the
patch version for `0.0.x` releases. When the graph needs a crate in two
incompatible ranges, cargo keeps both, and each is compiled and linked
separately. The crate's code, and the code of anything it pulls in, is present
in the artifact once per version.

The rule groups the packages in the resolved dependency graph by name and
fires once for every name that appears in more than one compatibility class.
The detail lists every resolved version of that name. Versions in the same
class, such as `1.0.0` and `1.2.0`, never fire, because cargo unifies them into
one.

## Tradeoff

Unifying on one version can require upgrading or downgrading the crates that
depend on it, which can change their behavior. The saving is the duplicated
code and compile time of the extra version, and the cost is the dependency
changes needed to reach a single range. The rule leaves that direction to the
author, because it depends on which dependents can move.

## Scope

The rule reads the resolved graph filtered to the target platform, so a
duplicate that exists only on another platform is not reported. Only packages
that appear in the resolved graph count, and a package present in the lock file
but unused for the target does not. The rule is not gated on which artifacts the
workspace produces, and it fires once per workspace for each duplicated name.
