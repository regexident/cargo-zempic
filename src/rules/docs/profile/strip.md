# `profile/strip`

Controls how much cargo asks the linker to remove from the emitted binary. The
values are `"none"` (keep everything), `"debuginfo"` (drop debug information,
keep symbols) and `"symbols"` (drop both), with `true` and `false` as aliases for
`"symbols"` and `"none"`.

By default a release build retains the symbol table and the string data the
symbols point at. They are not needed to run the program, so a shipping binary
carries weight that serves only a debugger.

Setting `strip = "symbols"` lets the linker discard that data. The change is
safe: it cannot alter what the program does, only what a debugger or a profiler
can show about it.

## Tradeoff

Stripping symbols removes function names from backtraces, so a panic report
points at addresses rather than names, and a profiler's output loses its labels.
A project that debugs its release binaries in place can keep
`strip = "debuginfo"` to retain symbols while still dropping debug information,
or leave stripping to a separate packaging step.

## Interaction

`profile/debug-info` covers the `debug` setting, which decides whether the
compiler emits debug information at all. Stripping and disabling debug
information are independent: stripping removes information already produced,
while disabling the `debug` setting stops producing it. A project may want
either or both.
