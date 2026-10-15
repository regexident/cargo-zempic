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
