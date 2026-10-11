//! Rules about cargo profile settings and workspace profile placement.
//!
//! Every compiled artifact is built under exactly one profile, and the profile's
//! settings decide its size and behavior. Cargo fills any setting a manifest
//! leaves unset from a built-in default, so a key absent from `Cargo.toml` still
//! has an effective value; a rule therefore distinguishes an explicitly set
//! value from a default rather than reading the manifest text alone.
//!
//! Cargo honors a `[profile.*]` table only in the workspace root manifest, so a
//! member's own table is inert. The rules that report a profile setting read the
//! root manifest and are gated on the selection producing at least one linked
//! artifact, because a profile does not govern an `rlib` or a procedural macro.
//! The one rule about a member's own table lives here too, since it is about the
//! same table cargo parses and has no other module to live in.
