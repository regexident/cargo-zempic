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
