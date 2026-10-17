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

Applying this rule's fix means removing the flag from the file that sets it.
That is only possible when the file is one the project under analysis owns —
a chain file inside the workspace root. A `RUSTFLAGS` or
`CARGO_ENCODED_RUSTFLAGS` environment variable sets no file at all, and a
chain file above the workspace root belongs to the user's machine rather than
to the project, so in both cases the finding still fires but drops to
report-only: the change is real, but it is the user's to make, not a safe
automated edit.

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
