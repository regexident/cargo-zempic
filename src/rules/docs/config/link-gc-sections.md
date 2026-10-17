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
