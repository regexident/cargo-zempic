# `profile/lto`

Link-time optimization lets the compiler optimize across crate boundaries
rather than one crate at a time. The `lto` setting accepts `false` and `"off"`
(disabled), `"thin"` (across the local crate's codegen units) and `true` and
`"fat"` (across the whole dependency graph).

The release default is `false`. Cross-crate inlining and dead-code elimination
remove code that a per-crate build must keep, so `"fat"` is the setting that
delivers the whole-graph win; `"thin"` is a partial version that keeps link
times shorter.

## Tradeoff

Link time rises substantially with `"fat"`, because the whole graph is
optimized at once. `"thin"` is the compromise when a full-graph link is too
slow. Link time is a build-time cost only; the linked artifact does not run any
differently beyond being smaller.
