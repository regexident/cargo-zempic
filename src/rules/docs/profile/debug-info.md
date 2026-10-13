# `profile/debug-info`

The `debug` setting decides how much debug information the compiler writes into
the artifact. It accepts `0`, `1` and `2`, the strings `"none"`, `"limited"`,
`"full"`, `"line-directives-only"` and `"line-tables-only"`, and `false` and
`true` as aliases for `0` and `2`.

A release build that enables `debug` retains line tables, type information and
variable descriptions. When the artifact is not going to be debugged as built,
that data is dead weight.

Setting `debug = false` stops the compiler producing debug information, so none
is written. The change cannot alter the program's behavior.

## Tradeoff

A release binary built without debug information cannot be stepped through,
and a crash report cannot be mapped back to source lines. A project that needs
release-profile debugging should keep `debug` enabled and rely on stripping or
on a separate symbol file instead.

## Interaction with stripping

When `strip` already removes symbols (as `"symbols"` or `true`), the linked
artifact is stripped anyway, so disabling `debug` no longer saves size. It still
saves compile time and the intermediate disk the compiler writes before the
linker drops it. The finding's detail states which of the two benefits applies.
