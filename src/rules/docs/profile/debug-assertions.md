# `profile/debug-assertions`

Debug assertions are checks the standard library and many crates compile in only
when the `debug-assertions` setting is on: `debug_assert!`, overflow checks in
iterator adapters, and internal consistency checks. Cargo's `dev` profile sets
it on; a release-like profile leaves it off.

A release-like profile that has `debug-assertions = true` compiles those checks
into the shipped artifact. Setting it to `false` drops them.

## Tradeoff

The checks that are removed catch bugs: an invariant that a debug assertion
guards will go unnoticed in the release build. That is the point of the setting
in a release profile, where the checks are understood to be a development aid,
but a project that relies on them for validation should keep them on.

## Scope

The rule does not apply to `dev`, `test` or `bench`, where debug assertions are
deliberate.
