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
