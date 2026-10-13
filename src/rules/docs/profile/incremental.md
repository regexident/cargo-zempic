# `profile/incremental`

The `incremental` setting enables incremental compilation, which speeds up
rebuilds by reusing work from a previous build. It is `true` in cargo's `dev`
profile, where a fast edit-compile loop matters.

In a release-like profile the default is `false`, and that is the right default:
incremental compilation trades runtime speed for rebuild speed, and a release
artifact is what the user ships. A release-like profile that turns `incremental`
on keeps the extra bookkeeping in the build.

## Tradeoff

Setting `incremental = false` makes release rebuilds slower, because each build
starts from scratch. That is the intended trade for a profile whose output is
shipped rather than iterated on.

## Scope

This rule does not apply to the `dev`, `test` or `bench` profiles, where
incremental compilation is deliberate and useful.
