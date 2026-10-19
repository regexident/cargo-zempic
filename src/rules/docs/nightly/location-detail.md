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
