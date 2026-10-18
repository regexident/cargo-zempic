# `advisory/lib-profile-not-applicable`

A cargo profile governs a linked artifact: a binary, a `cdylib` or a
`staticlib`. A library built as an `rlib`, or a procedural macro, is not linked
by cargo. Its code is compiled under the profile of whichever binary eventually
links it, so the analyzed profile does not shape the library's own artifact.

For each selected package that produces no linked artifact, the rule emits one
informational note naming the package and the analyzed profile. The `profile/*`
and `config/*` rules return nothing for such a package, and without the note a
reader would see no profile finding and could not tell whether the profile was
sound or simply not examined.

## Tradeoff

The note recommends no change and carries no magnitude. It exists to explain an
absence: the profile and rustflags rules were not run for this package, because
nothing they report on would reach its artifact.

## Scope

The rule reads every selected package and fires once for each one whose targets
produce no linked artifact. A package that produces a binary, a `cdylib` or a
`staticlib` is quiet, and so is a selection made only of such packages. The
dependency and library rules still apply to a package this note covers. The
finding carries no location, because the note is about the package and not about
a manifest key.
