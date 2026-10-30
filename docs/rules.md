# Rule reference

This reference is generated from the rule documentation embedded in the binary.

---

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

---

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

---

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

---

# `profile/missing-config`

A release profile with no explicit settings falls back to cargo's built-in
defaults, which favor short compiles over a small artifact: symbols are kept,
debug information is off, link-time optimization is off, and the optimization
level targets speed rather than size.

This finding points at the absent configuration rather than at one setting. The
per-key findings in the same report name the individual settings to set, each of
them a one-line manifest edit, so this rule explains why several of them appear
at once: nobody ever configured the profile.

## Tradeoff

Configuring the profile changes only how a release build is produced, not what
the program does, so there is nothing to lose by it. A project that has
deliberately chosen cargo's defaults can suppress this rule, though a profile
with no settings is more often one that was never considered.

---

# `profile/opt-level`

The `opt-level` setting chooses how hard the compiler optimizes. It accepts `0`
to `3` and the strings `"s"` (optimize for size) and `"z"` (optimize for size
without loop vectorization).

The release default is `3`, which targets speed. When the goal is a small
artifact, `"z"` asks the compiler to prefer size over speed, disabling loop
vectorization that tends to enlarge code. `"s"` is the middle ground: size
optimization while keeping vectorization.

## Tradeoff

Optimizing for size can make compute-heavy code slower. `"z"` is the stronger
choice and the more pronounced slowdown; `"s"` keeps more of the speed. A
latency-critical binary should stay on `3` and look for size elsewhere, or
apply `"z"` only to the code that is not on the hot path.

## Dimension

This rule is dimensioned, because the recommendation differs by optimization
objective. The size objective is identified as `profile/opt-level:size`. A speed
objective would recommend the opposite value and carries its own identifier.

---

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

---

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

---

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

---

# `profile/dependency-opt-level`

By default a release build compiles dependencies with the same `opt-level` as
the crate itself. When a project lowers its own `opt-level` to debug its code,
that also lowers the dependencies', which are usually not being debugged.

A per-package override narrows this: `[profile.release.package."*"]` with
`opt-level = "z"` optimizes the dependencies for size while leaving the
workspace's own members on the profile's setting. The glob applies to every
dependency.

## Tradeoff

The override applies to dependencies and not to workspace members, so it does
not change how the project's own code is optimized. It also changes exactly the
crates the project does not control, so a dependency that misbehaves under size
optimization would have to be excluded by name.

---

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

---

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

---

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

---

# `nightly/location-detail`

A panic report prints the file and line where the panic was raised. The
compiler records those source locations in the panic payload, and every
artifact that can panic carries them, whether or not a panic is ever expected.
`-Z location-detail` chooses how much of that information the compiler
records; `none` tells it to record nothing.

## Mechanism

Cargo passes a `-Z` flag through to the compiler as a rustflag exactly like a
`-C` flag, so the option is configured next to the other rustflags in a
`config.toml`, usually under `[build]`:

```toml
[build]
rustflags = ["-Z", "location-detail=none"]
```

The flag is unstable: rustc accepts it only on a nightly toolchain. On a stable
or beta channel it is rejected, which is why this rule reports nothing there
rather than recommending a flag that cannot be built with.

The rule reads the rustflags cargo actually applies, not every occurrence in
the configuration chain. A `-Z location-detail=none` declared in a rank another
rank shadows is inert: the build does not carry it, so the finding still
reports, and its detail says that the option is declared in a rank cargo does
not apply and has to be moved to, or added to, the rank it does. The finding
names `build.rustflags` in the nearest configuration file that declares the
key, counting from the package directory outwards with `$CARGO_HOME/config.toml`
last, whatever rank that file's entry belongs to. When the resolution has no
live entry (an environment-supplied `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`,
or a chain that reaches no configuration file), it names `build.rustflags` in
the workspace root's `.cargo/config.toml`, the file a remedy would create.

The option is considered configured only in its valued `none` spelling. A bare
`-Z location-detail` selects rustc's own default, which keeps the full
location, and a value other than `none` keeps more of it than the total
omission this rule asks for, so both states still fire.

## Interaction with `profile/panic-abort`

`profile/panic-abort` reports the profile setting that removes unwinding. This
rule is about what a panic report contains when a panic happens anyway, so the
two are independent: aborting does not remove the location strings, and
dropping them does not change how a panic stops the process.

Both this rule and `nightly/panic-immediate-abort` edit the same rustflags
array, and the two changes compose, since one configures what a panic report
prints and the other how the panic stops the program. Their conditions never
overlap in the other direction, because each rule reads its own flag name and
ignores the other's.

## Tradeoff

A panic report loses its source locations, so a crash no longer names the file
and line it came from, which makes a production crash far harder to place. The
flag is unstable, so a nightly upgrade may change or remove it, and the flag
has to be revisited then. Like any rustflags change, adding it invalidates the
whole build cache.

## Reference

- [The unstable book: `-Z location-detail`](https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/location-detail.html)

---

# `nightly/panic-immediate-abort`

A panic normally unwinds: the runtime walks the stack, runs each destructor it
passes, and then terminates the process. The standard library carries the
machinery that walk needs. `-Z panic-immediate-abort` makes the panic path
abort instead, so the process stops where the panic is raised and none of that
machinery is needed.

## Mechanism

Cargo passes a `-Z` flag through to the compiler as a rustflag, so the option is
configured next to the other rustflags in a `config.toml`:

```toml
[build]
rustflags = ["-Z", "panic-immediate-abort"]
```

The flag is unstable: rustc accepts it only on a nightly toolchain. It also
changes the strategy the standard library is compiled with, so the flag alone
does nothing unless the standard library is built for it. That is what a
`-Z build-std` build is for: cargo compiles the standard library from source
with the same rustflags, so the panic path in `std` aborts as well.

The rule reads the rustflags cargo actually applies, not every occurrence in
the configuration chain. A `-Z panic-immediate-abort` declared in a rank
another rank shadows is inert, and the build does not carry it. The finding
names `build.rustflags` in the nearest configuration file that declares the
key, counting from the package directory outwards with `$CARGO_HOME/config.toml`
last, whatever rank that file's entry belongs to. When the resolution has no
live entry (an environment-supplied `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`,
or a chain that reaches no configuration file), it names `build.rustflags` in
the workspace root's `.cargo/config.toml`, the file a remedy would create.

The profile's own `panic = "abort"` setting is a different lever: it selects
whether the code being compiled unwinds, while this flag selects how the
standard library's panic path behaves. A profile that already aborts still has
an unwinding standard library, so the two do not substitute for each other.

## Interaction with `nightly/location-detail`

Both rules edit the same rustflags array, and the two changes compose, since one
configures how a panic stops the program and the other what a panic report
prints when one happens. Each rule reads its own flag name and ignores the
other's, so configuring one silences only that rule.

## Tradeoff

A panic aborts without unwinding, so `catch_unwind` never recovers from it and
destructors of the values on the stack do not run, which skips any cleanup they
perform. The technique requires a `-Z build-std` build, which compiles the
standard library from source and therefore takes considerably longer than a
prebuilt one, and the unstable flag may break across nightly releases.

## Reference

- [The unstable book: `-Z panic-immediate-abort`](https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/panic-immediate-abort.html)
- [Cargo: `-Z build-std`](https://doc.rust-lang.org/cargo/reference/unstable.html#build-std)

---

# `workspace/ignored-member-profile`

Cargo honors a `[profile.*]` table only in the workspace root manifest. A
`[profile.*]` table written in a member's own `Cargo.toml` has no effect, since
cargo ignores it and the settings it declares never reach the artifact.

The table is silently inert, which is what makes it worth reporting. The
manifest looks configured, and a reader has no way to tell from the file that
the settings are doing nothing. Moving the table to the workspace root
`Cargo.toml` makes it take effect.

## Tradeoff

Removing the table changes nothing about the artifact, because cargo already
ignores it, which is why the finding carries no magnitude. What it warns about
is the belief that the table was working, not a size that was lost. Move the
settings to the root before removing the member's table.

## Scope

The rule reads the selected members only, and skips the workspace root manifest,
whose own table is the one cargo honors.

---

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

---

# `config/rustflags-shadowed`

Cargo resolves `rustflags` from exactly one of three ranks, in this order:

1. The `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` environment variable, present
   even when empty.
2. Every `target.<triple>.rustflags` table matching the triple being built,
   concatenated with every `target.'cfg(...)'.rustflags` table found anywhere
   in the configuration chain. The `CARGO_TARGET_<TRIPLE>_RUSTFLAGS`
   environment variable is part of this rank: cargo appends its flags after the
   matching triple table's.
3. The `build.rustflags` key. The `CARGO_BUILD_RUSTFLAGS` environment variable
   is part of this rank: cargo appends its flags to `build.rustflags`.

Cargo never merges flags across these ranks. It takes the first one present in
the list above and ignores the rest outright, including every entry a lower
rank declares. Appending a flag to `build.rustflags` while a `target.*` table
is present therefore has no effect at all, since cargo never reads it, so the
flag never reaches the compiler regardless of how it is spelled or how many
flags sit beside it. An environment-mapped entry that a higher rank shadows is
reported the same way, without a file location, since it is declared in no
file.

A `config.toml` is read bottom-up relative to the package directory: the
nearest file's keys win over a more distant one's for the same rank, and the
walk ends at `$CARGO_HOME/config.toml`. A rank can therefore be set in one
file and shadowed by another rank set in a different file entirely, and the
finding still fires, because what matters is which rank wins across the whole
chain rather than which single file is read.

## Unevaluated `cfg` predicates

This tool evaluates no `cfg(...)` predicate. A `target.'cfg(windows)'.rustflags`
table counts as present rank-two content on every triple, whether or not the
predicate would actually hold for the triple being built. When such a table is
among the entries contributing to the winning rank, the verdict that a lower
rank is shadowed still holds, since a live `cfg` table necessarily outranks
`build.rustflags` regardless of what it evaluates to. The finding is reported at
medium rather than high confidence, because the condition proving dominance is
one this tool did not actually check.

## Scope limits on `target.*` rustflags

A `target.<triple>.rustflags` or `target.'cfg(...)'.rustflags` table does not
apply to a build script or a procedural macro, both of which build for the
host machine rather than for the triple under analysis, unless
`target-applies-to-host` and `host-config` are both set to opt into that
behavior. A shadowed finding concerns only the triple cargo resolves the
package's linked artifact for, not every artifact cargo builds along the way.

## Tradeoff

Removing or relocating the inert entry changes the artifact not at all, since
cargo was already ignoring it, which is why the finding carries no magnitude.
What the finding corrects is the belief that the inert flags are taking effect.
Any change to the rustflags that do take effect invalidates the whole build
cache, because rustc treats a rustflags change as invalidating every cached
artifact rather than only the ones the flag affects, so acting on this finding
costs a full rebuild.

## No single remedy

The rule recommends no particular fix, because the three outcomes are equally
defensible and only the user knows which one matches their intent: move the
shadowed flags into the rank that wins so they take effect; remove the inert
entry so the configuration states only what actually happens; or leave both as
they are, if the inert entry documents an alternative the user wants kept
nearby for later. `config/rustflags-shadowed` is fixed at `ReportOnly` for this
reason, since there is no safe default to apply automatically.

---

# `config/link-dead-code`

`-C link-dead-code` tells the linker to keep every symbol the compiler
produced, including a function or static nothing in the program ever
reaches. Rustc's own coverage instrumentation sets it internally: a coverage
run needs the linker to retain the symbols its instrumentation reads, even
the ones the program's own logic never calls, or the coverage report would
have nothing to attribute to them. Outside a coverage run the flag has no
comparable purpose, since it only stops the linker from doing what it would do
by default and discarding what nothing reaches. A release build that sets it is
therefore almost always inheriting a coverage configuration that was never meant
to govern that build.

## Reading the live rustflags

A `config.toml` can declare `-C link-dead-code` in a rank cargo never reads:
a `build.rustflags` entry beneath a live `target.*` table is set in the file
but never applied, exactly the condition `config/rustflags-shadowed` reports.
Reading the flag out of every entry in the chain, rather than out of the
already-resolved live rustflags, would report a release build as retaining
dead code when the compiler never actually saw the flag. It would also do so
alongside `config/rustflags-shadowed` correctly reporting that same entry as
inert, leaving two findings about the same key that cannot both be acted on.
This rule therefore reads only the rustflags cargo's own precedence resolved
as live, never a shadowed entry's flags.

## Report-only sources

Applying this rule's fix means removing the flag from the file that sets it,
which is only possible when the file is one the project under analysis owns, a
chain file inside the workspace root. A `RUSTFLAGS`,
`CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS` or
`CARGO_TARGET_<TRIPLE>_RUSTFLAGS` environment variable sets no file at all, and
a chain file above the workspace root belongs to the user's machine rather than
to the project. In both cases the finding still fires but drops to report-only:
the change is real, but it is the user's to make, not a safe automated edit.

## Effect on section garbage collection

Setting `-C link-dead-code` also suppresses rustc's own default request that
the linker discard unreferenced sections, because the flag exists to keep
instrumentation-only symbols alive and a linker cannot tell those apart from
anything else nothing calls. Removing the flag is therefore also what restores
section garbage collection. `config/link-gc-sections` reports that consequence
separately, so the linker-level effect is visible even to a reader who only
sees one of the two findings.

## Tradeoff

The flag increases the artifact's size for no benefit outside a coverage run,
but removing it returns the artifact to what it would have been without the
flag, rather than producing some new, smaller result. There is no class of
saving beyond undoing a mistake, so the finding carries no magnitude.
Removing the flag is still a rustflags change, and rustc treats any rustflags
change as invalidating the whole build cache rather than only the artifacts
the flag affects, so acting on this finding costs a full rebuild.

---

# `config/link-gc-sections`

Linker section garbage collection discards a function or a piece of static
data the linker can prove nothing in the final artifact references. The
request is platform-specific: an ELF linker wants `--gc-sections`, Apple's
`ld` wants `-dead_strip`, the MSVC linker wants `/OPT:REF`, and `wasm-ld`
wants `--gc-sections` of its own. All four are spelled through a `-C
link-arg=...` rustflag, because none of them is something `rustc` exposes a
dedicated codegen option for.

## A correctly configured project

Rustc already passes the flag its own linker wants, on every platform it
knows how to drive a linker for, without cargo or the user asking for it.
The one thing that stops rustc from doing so is `-C link-dead-code`, which
exists for coverage instrumentation and, as a side effect, suppresses this
exact default so the linker keeps everything the instrumentation might read.
A project that sets no explicit section-garbage-collection flag on a
classified target is not missing anything, because the default is already in
effect. This rule therefore never fires on the mere absence of an explicit flag
for a target whose linker it can name. It fires only when something has turned
the default off, or when the target's linker is one this tool cannot name at
all and so cannot claim a default for.

## Overlap with `config/link-dead-code`

When `-C link-dead-code` is live, both rules fire on the same flag, and that is
intentional rather than a duplicate. `config/link-dead-code` reports the flag
itself and the coverage-instrumentation purpose it almost certainly outlived,
while `config/link-gc-sections` reports the consequence on the linker and names
the flag that would restore section garbage collection without touching `-C
link-dead-code` at all. Removing `-C link-dead-code` is also what restores the
default, so acting on the first finding resolves the second one too, and the
two findings describe one cause from two angles a reader can act on
independently.

## Unclassified targets

A target this tool's linker-family table does not cover might still default
to section garbage collection, or might not; nothing here claims to know
rustc's behavior for a linker it cannot name. The finding still fires,
because the user should know the question is open, but at medium confidence
and with no recommended flag, since recommending one would mean guessing at
a spelling this tool does not actually know.

## Tradeoff

The remedy is platform-specific: `--gc-sections` on ELF, `-dead_strip` on
Mach-O, `/OPT:REF` under MSVC, `--gc-sections` again under `wasm-ld`. Adding
any of them is still a rustflags change, and rustc treats a rustflags change
as invalidating the whole build cache rather than only the artifacts the
flag affects, so acting on this finding costs a full rebuild.

---

# `deps/workspace-resolver`

Cargo's feature resolver decides which features of a dependency are enabled
when several parts of the graph use it. Resolver `"1"` unifies a dependency's
features across its normal, build and dev uses, so a feature requested only by
a build script, a proc-macro host dependency or a test reaches the dependency
that is linked into the artifact. Resolver `"2"` keeps those kinds apart, and
only features requested by normal dependencies of the built target are enabled
for it.

The effective resolver is the root manifest's `workspace.resolver` when
present, else its `package.resolver`, else the value implied by the root
package's edition: editions 2015 and 2018 imply `"1"`, edition 2021 implies
`"2"` and edition 2024 implies `"3"`. A virtual manifest has no edition, so
without an explicit `workspace.resolver` it resolves to `"1"` whatever the
editions of its members are. Members' editions never select the workspace's
resolver, which is why a virtual manifest needs the key written out.

Resolver `"2"` is the lowest value that separates dependency kinds. Resolver
`"3"` adds only version-selection behavior, preferring versions compatible with
the declared minimum Rust version, and changes nothing about how features
unify, so the rule recommends `"2"`.

## Tradeoff

Feature sets of dependencies shared between normal, build and dev kinds stop
unifying. A build that relied on a feature leaking across kinds can fail to
compile until that feature is requested where it is used. The fix is a
one-line key in the root manifest, and the cost is the compile errors it can
surface, rather than any change at runtime.

## Scope

The rule reads the root manifest only, and fires once per workspace. It is not
gated on which artifacts the workspace produces, because the resolver governs
every package in the workspace. An edition 2021 or later root package with no
explicit key is quiet, as is any root manifest that sets `"2"` or `"3"`.

---

# `deps/default-features`

A dependency declared without `default-features = false` receives the features
its manifest lists under `default`. Those features can enable optional
dependencies of the dependency, directly with `dep:name`, through
`name/feature`, through the implicit feature an optional dependency creates, or
through any chain of other features. Every optional dependency enabled this way
is compiled and linked into the artifact whether or not the code uses it.

The rule expands the dependency's `default` feature through its feature table
and fires once per direct dependency of a selected package when the expansion
enables at least one optional dependency. The detail names those optional
dependencies. The weak form `name?/feature` enables nothing by itself, because
it forwards a feature only when `name` is enabled by other means. A default set
that enables only plain features does not fire, because the rule has no
evidence that those features cost anything.

The verdict is capped at medium confidence. Cargo's resolver version 2 keeps
the features of normal, build and dev dependencies apart, and `cargo metadata`
does not reproduce that separation: it reports one unified feature set. The
optional dependency the rule names can therefore be enabled for a build script
or a test only, in which case it never reaches the artifact. The detail says so
in a closing sentence.

## Tradeoff

Setting `default-features = false` removes every default feature, including
ones the code may use. Each one still needed must be re-enabled by name in the
dependency's `features` list. The saving is the optional dependencies and their
code, and the cost is that missing features surface as compile errors until they
are listed.

## Scope

The rule reads the direct normal dependencies of each selected package,
including target-specific ones, and ignores dev and build dependencies. A
dependency already declared with `default-features = false` is quiet, as is one
the resolved graph does not reach for the target platform. The finding points
at the dependency's key in the `[dependencies]` table, and a target-specific
dependency carries no location. The rule is not gated on which artifacts the
workspace produces.

---

# `deps/duplicate-versions`

Cargo resolves a crate to one version per semver-compatible range. Two
versions are compatible when their leftmost non-zero component is equal: the
major version for `1.x` and later, the minor version for `0.x` releases and the
patch version for `0.0.x` releases. When the graph needs a crate in two
incompatible ranges, cargo keeps both, and each is compiled and linked
separately. The crate's code, and the code of anything it pulls in, is present
in the artifact once per version.

The rule groups the packages in the resolved dependency graph by name and
fires once for every name that appears in more than one compatibility class.
The detail lists every resolved version of that name. Versions in the same
class, such as `1.0.0` and `1.2.0`, never fire, because cargo unifies them into
one.

## Tradeoff

Unifying on one version can require upgrading or downgrading the crates that
depend on it, which can change their behavior. The saving is the duplicated
code and compile time of the extra version, and the cost is the dependency
changes needed to reach a single range. The rule leaves that direction to the
author, because it depends on which dependents can move.

## Scope

The rule reads the resolved graph filtered to the target platform, so a
duplicate that exists only on another platform is not reported. Only packages
that appear in the resolved graph count, and a package present in the lock file
but unused for the target does not. The rule is not gated on which artifacts the
workspace produces, and it fires once per workspace for each duplicated name.

---

# `deps/feature-unification`

Cargo enables a feature on a dependency when any requester in the graph asks for
it. A package that declares a dependency with `default-features = false` opts
out of that dependency's default features, but the opt-out only holds when no
other declaration of the same dependency requests them. When another workspace
member, or a dependency reached through the graph, declares it with default
features, cargo unifies the two and the default features are compiled and linked
anyway.

The rule inspects each direct normal dependency of a selected package that is
declared with `default-features = false`, and fires when the resolved graph
lists `default` among the features enabled on the resolved package. The detail
names the workspace members whose own declaration of that package requests the
default features. When no member does, the detail says that a dependency outside
the members' own declarations requested them.

The verdict is capped at medium confidence. Cargo's resolver version 2 keeps
the features of normal, build and dev dependencies apart, and `cargo metadata`
does not reproduce that separation: it reports one unified feature set. The
requester that enables `default` can therefore be a build script or a test
only, in which case the defaults never reach the artifact. The detail says so in
a closing sentence.

## Tradeoff

Restoring the opt-out means changing the other declaration that requests the
default features. That declaration belongs to a crate that may rely on the
functionality the defaults provide, so removing them can break it. The saving is
the code behind the default features, and the cost is the change to the
requesting declaration, which the rule leaves to the author.

## Scope

The rule reads the direct normal dependencies of each selected package,
including target-specific ones, and ignores dev and build dependencies. A
dependency declared with default features is quiet, as is one the resolved graph
does not reach for the target platform. The finding points at the dependency's
key in the `[dependencies]` table of the opting-out package, and a
target-specific dependency carries no location. The finding is reported on the
package that opted out, never on the package that requested the defaults. The
rule is not gated on which artifacts the workspace produces.

---

# `deps/feature-provenance`

Cargo enables a feature on a dependency when any requester in the graph asks for
it, so a feature can be compiled into the artifact without anything in the
workspace having asked for it. When a workspace member declares a dependency
and a second dependency in the graph requests an extra feature of the same
crate, the member's build carries that feature although neither its manifest nor
its own features mention it.

For each direct normal dependency of a selected package, the rule compares two
sets. The enabled set is the feature list the resolved graph reports for the
dependency's package. The requested set is what the workspace members ask for:
every member's direct declaration of that package contributes `default` when it
uses default features and the features it lists, and a member feature that
forwards a feature with `name/feature` or `name?/feature` contributes that
feature once the member feature is enabled. Each request is expanded through
the dependency's feature table, so a requested feature also covers the features
and optional dependencies it enables. Enabled features outside the requested set
are reported once per dependency, and the detail lists them.

The verdict is capped at medium confidence. Cargo's resolver version 2 keeps
the features of normal, build and dev dependencies apart, and `cargo metadata`
does not reproduce that separation: it reports one unified feature set. The
requester behind a reported feature can therefore be a build script or a test
only, in which case the feature never reaches the artifact. The detail says so
in a closing sentence.

## Tradeoff

Removing a feature that only another dependency requests means dropping or
replacing that dependency, or changing the way it is declared. The dependency
may provide functionality the code relies on. The saving is the code behind the
feature, and the cost is that change, which the rule leaves to the author
because the rule cannot name the requester.

## Scope

The rule reads the direct normal dependencies of each selected package,
including target-specific ones, and ignores dev and build dependencies. A
dependency the resolved graph does not reach for the target platform is quiet,
as is one whose enabled features every workspace member requests. The finding
points at the dependency's key in the `[dependencies]` table, and a
target-specific dependency carries no location. The rule is not gated on which
artifacts the workspace produces.

---

# `deps/feature-trim`

A dependency declared with its default features enabled pulls in every feature
of its default set. Some of those features add code most users never call, and
a crate's own documentation rarely says which. This rule consults the knowledge
base, a curated list of crates and the default features worth turning off, and
reports each default feature an entry flags when the declaration does not ask
for it.

An entry applies to a resolved version only when the version satisfies the
entry's version range. A dependency resolved outside the range is never
reported, because advice written for one release line says nothing reliable
about another. Each entry also records the newest release its advice was
verified against. When the resolved version is newer, the finding is still
emitted, but it is downgraded to `ReportOnly` and ends with a note naming the
verified release and the one in use, so a stale entry is never offered as an
automatic fix.

An entry marks each feature as `fixable` or not. A feature whose advice cannot
be applied by editing the manifest alone is reported as `ReportOnly` as well,
whatever its version.

A feature the declaration requests by name, directly or through another
requested feature, is wanted and does not fire. A declaration that already sets
`default-features = false` is quiet too.

The embedded knowledge base can be extended or overridden from `zempic.toml`
under `[knowledge_base.crates.<name>]`. A configured entry replaces the
embedded entry of the same name entirely, including its version range, so an
override whose range excludes the resolved version silences the crate.

## Tradeoff

Disabling defaults removes every default feature, not only the one reported.
Each default feature the code still needs must be re-enabled by name, and a
feature that is missing shows up as a compile error or, for behavior selected
at runtime, as a changed result. The magnitude of a finding is the entry's
estimate for the one feature it names.

## Scope

The rule reads the normal dependencies of the selected packages that declare
default features and resolve to a package for the analyzed target. Build and
development dependencies are not examined, and neither are dependencies that
the resolved target platform does not use. Only crates the knowledge base holds
an entry for can produce a finding.

---

# `deps/likely-dev-only`

Test frameworks, benchmark harnesses and snapshot tools exist to support
development. A crate of that kind declared under `[dependencies]` is compiled
into every build of the package, shipped or not, although shipped code almost
never calls it. This rule consults the knowledge base, which marks such crates
as `dev_only`, and reports each one a selected package declares as a normal
dependency.

The embedded knowledge base marks `criterion`, `proptest`, `insta`,
`quickcheck`, `pretty_assertions`, `mockall` and `rstest` this way. The list can
be extended or overridden from `zempic.toml` under
`[knowledge_base.crates.<name>]` with `dev_only = true`, and a configured entry
replaces the embedded entry of the same name entirely.

An entry applies to a resolved version only when the version satisfies the
entry's version range. A dependency resolved outside the range is never
reported. When the resolved version is newer than the release the entry was
verified against, the finding ends with a note naming both releases.

The finding is medium-confidence and report-only. The rule cannot see source,
so it cannot tell a harness used only by tests from one that shipped code
depends on, and the fix is never applied automatically.

## Tradeoff

Moving the crate to `[dev-dependencies]` removes it from normal builds. Any
code outside tests, benchmarks and examples that uses it stops compiling until
that code is gated behind a feature or moved. A crate that is genuinely part of
the shipped functionality is a false positive and can be silenced by
overriding its entry in `zempic.toml`.

## Scope

The rule reads the normal dependencies of the selected packages that resolve to
a package for the analyzed target. Development and build dependencies are not
examined, and neither are dependencies that the resolved target platform does
not use. Only crates the knowledge base marks `dev_only` can produce a finding.

---

# `deps/build-only-note`

Build scripts and procedural macros run while the package compiles. Cargo
builds them, and everything they depend on, for the host machine rather than
for the target the artifact runs on. A crate reached only through such an edge
is part of the build and not of the artifact: it affects compile time, but
removing or trimming it does not make the artifact smaller or faster.

This rule consults the knowledge base for crates marked heavy, meaning a
crate-level magnitude of moderate or large. For each selected package that
produces a linked artifact, it walks the resolved dependency graph and tracks
whether each crate is reached through a build dependency or a procedural macro.
When at least one heavy crate is reached and every heavy crate is reached that
way, the rule reports them together in one note. When the resolved version is
newer than the release an entry was verified against, the note ends with a
statement naming both releases.

The note reports a non-finding. It recommends no change, and instead tells the
reader which heavy dependencies are not worth optimizing for the sake of the
artifact.

## Tradeoff

None, since the note points optimization effort away from dependencies that do
not reach the artifact.

## Scope

The rule follows normal and build dependencies of packages that produce a
linked artifact. Development dependencies are not followed, since they are not
part of the artifact. A heavy crate that is reached both through a host path and
through a target path keeps the note silent, because it does reach the artifact.
Packages that produce no linked artifact are not examined.

---

# `lib/heavy-default-features`

A library's `default` feature set is paid by every consumer that does not opt
out of it. When that set enables an optional dependency that is costly to
compile, each binary that links the library compiles the dependency too,
whether or not the binary uses the code that needs it. The library's author
does not see that cost, because the dependency is optional in the library's own
manifest.

For each selected package that produces no linked artifact, the rule expands
the package's `default` feature through its feature table, following `dep:`
references, forwarded features and implicit optional-dependency features. Each
optional normal dependency the expansion enables is resolved to a package, and
the knowledge base is consulted for an entry matching the resolved version. A
dependency is heavy when its entry records a moderate or large crate-level
magnitude, and the finding carries that magnitude. A dependency without an
entry, without a magnitude, or with a small one does not fire.

The embedded knowledge base can be extended or overridden from `zempic.toml`
under `[knowledge_base.crates.<name>]`. When the resolved version is newer than
the release the entry was verified against, the finding ends with a note naming
both releases.

An optional dependency that the analyzed feature selection and target do not
resolve does not fire either, since its version is unknown and no entry can be
matched to it.

## Tradeoff

Removing the dependency from the default set is a breaking change for consumers
that rely on it without naming the feature: their build either fails to
compile or silently loses the behavior the dependency provided. The finding is
report-only, because moving a dependency out of the defaults is a decision
about the library's public interface.

## Scope

The rule applies to selected packages that produce no linked artifact, and a
binary, a `cdylib` or a `staticlib` is quiet. Only optional dependencies of
kind normal are examined; build and development dependencies are not. Only
crates the knowledge base holds a heavy entry for can produce a finding, and
the rule emits one finding per heavy dependency, located at the package's
`features.default` key.

---

# `lib/no-feature-granularity`

A library is compiled into every binary that links it, and each direct
dependency it declares is compiled along with it. When the library declares no
features, a consumer cannot switch any of those dependencies off: the whole set
is built whether or not the consumer uses the code that needs it. Gating
dependencies behind optional features lets a consumer take only the parts it
uses.

For each selected package that produces no linked artifact, the rule counts the
direct dependencies of kind normal, target-specific ones included, and reads the
package's feature map from `cargo metadata`. That map lists the implicit feature
of every optional dependency, so a package whose only features are implicit ones
counts as having features and does not fire. The rule fires for a package with
at least ten such dependencies and an empty feature map.

## Tradeoff

Gating dependencies behind features adds conditional compilation and a feature
matrix to test. Each feature is also public API, since removing one later breaks
the consumers that name it. The finding is report-only because choosing which
dependencies deserve a feature depends on how the library is used.

## Scope

The rule applies to selected packages that produce no linked artifact, and a
binary, a `cdylib` or a `staticlib` is quiet. Development and build dependencies
are not counted, because they are not compiled into a consumer. A library with
fewer than ten direct normal dependencies is quiet whether or not it declares
features. The finding carries no location, because the absence of a `[features]`
table has no key to point at.
