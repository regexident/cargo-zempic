# `profile/overflow-checks`

The `overflow-checks` setting decides whether integer arithmetic is checked for
overflow at run time. When on, an overflowing operation panics; when off, it
wraps silently. Cargo's `dev` profile sets it on and a release-like profile
leaves it off.

A release-like profile that has `overflow-checks = true` compiles the checks
into the shipped artifact. Setting it to `false` drops them.

## Tradeoff

With the checks removed, arithmetic overflow no longer panics; it wraps, and a
miscalculation that would have stopped the program continues with a wrong value.
A project that depends on overflow trapping to reject bad input should keep the
checks on, or perform the check explicitly.

## Scope

The rule does not apply to `dev`, `test` or `bench`, where the checks are
deliberate.
