# `profile/panic-abort`

The `panic` setting chooses what the program does when a panic occurs. It
accepts `"unwind"` (the default) and `"abort"`. Unwinding carries the machinery
that walks the stack and runs destructors; aborting stops the process
immediately and needs none of it.

Setting `panic = "abort"` lets the compiler drop that machinery. The setting
applies to the whole workspace: `panic` cannot be set per package, because it
changes the ABI of every crate linked into the artifact.

## Caveats

The setting is ignored for test and bench targets, which always unwind so that a
failing assertion can be reported. Cargo also rejects `panic` in the `test` and
`bench` profiles outright, so it can only be set on a profile a test binary does
not inherit.

## Tradeoff

Aborting loses unwinding, so a `catch_unwind` anywhere in the program or its
dependencies no longer recovers from a panic and the process aborts instead.
zempic does not read source and cannot tell whether that affects this workspace,
so the finding states this warning unconditionally.

## Scope

The rule is skipped when the analyzed profile is `test` or `bench`, or inherits
from either, because cargo rejects the setting there. An aborting profile has no
way to run the test harness, so there is nothing to recommend.
