# `profile/codegen-units`

The `codegen-units` setting caps how many units the compiler splits a crate into
for parallel code generation. The release default is `16`, which compiles
quickly but gives each unit less code to optimize.

Setting `codegen-units = 1` lets the optimizer see the whole crate at once. This
setting delivers little on its own; its value comes from link-time optimization,
which needs a single codegen unit to act across crate boundaries. With several
units, cross-unit optimization is limited. Read
[`profile/lto`](lto.md) alongside this rule, since the two are usually applied
together and `codegen-units = 1` is what lets LTO do its job.

## Tradeoff

Compiling a crate as one unit removes the parallelism, so the build is slower.
The artifact behaves the same and is only smaller.
