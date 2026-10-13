# `profile/split-debuginfo`

The `split-debuginfo` setting decides whether debug information is written
inside the linked artifact or into separate files beside it. The values are
`"off"` (keep it in the artifact), `"packed"` (a directory of object files) and
`"unpacked"` (one file per object).

Cargo's default depends on the target. On Apple platforms the `dev` profile
defaults to `"unpacked"`; everywhere else, and for `release`, it defaults to
`"off"`. When it resolves to `"off"` while debug information is enabled, the
debug information is linked into the artifact.

Setting `split-debuginfo = "packed"` moves debug information out of the linked
artifact without discarding it, so the artifact the user ships is smaller and
the information remains available for debugging.

## Tradeoff

This recommendation is platform-specific. It matters on Apple targets, where
the default can leave debug information in the binary, and on platforms whose
default is already `"off"` there is nothing to change. Moving debug information
into side files also means those files must be kept alongside the binary for a
debugger to find it.
