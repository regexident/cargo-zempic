//! The optimization objectives an analysis can be run for.
//!
//! Cargo's profile surface optimizes for speed by default; producing a small
//! artifact is a distinct objective that requires its own settings. A
//! [`Dimension`] names such an objective so that a run is explicit about which
//! one it is answering, and a request can carry a dimension that is rejected
//! when it is one this crate does not implement.

/// An optimization objective an analysis answers.
///
/// The enum is closed so that a requested dimension is known to be one the
/// crate implements, rather than an unrecognized name that would silently
/// select nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dimension {
    /// Producing the smallest possible artifact.
    Size,
}

impl Dimension {
    /// The name this dimension is selected by on the command line and reported
    /// under.
    pub fn as_str(&self) -> &'static str {
        match self {
            Dimension::Size => "size",
        }
    }

    /// The dimension a name selects, or [`None`] when no dimension carries it.
    ///
    /// Only [`Dimension::Size`] is implemented, so any other name, including
    /// one cargo or a future version might recognize, is reported as
    /// unrecognized rather than defaulted.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "size" => Some(Dimension::Size),
            _ => None,
        }
    }
}
