# `profile/dependency-opt-level`

By default a release build compiles dependencies with the same `opt-level` as
the crate itself. When a project lowers its own `opt-level` to debug its code,
that also lowers the dependencies', which are usually not being debugged.

A per-package override narrows this: `[profile.release.package."*"]` with
`opt-level = "z"` optimizes the dependencies for size while leaving the
workspace's own members on the profile's setting. The glob applies to every
dependency.

## Tradeoff

The override applies to dependencies and not to workspace members, so it does
not change how the project's own code is optimized. It also changes exactly the
crates the project does not control, so a dependency that misbehaves under size
optimization would have to be excluded by name.
