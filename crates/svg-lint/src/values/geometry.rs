//! Transform lists and preserveAspectRatio.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Transform`: a transform list using these functions.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_transform(value: &str, functions: &[&str]) -> Option<String> {
    let _ = (value, functions);
    None
}

/// `PreserveAspectRatio`: alignment plus optional meet/slice.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_preserve_aspect_ratio(
    value: &str,
    alignments: &[&str],
    meet_or_slice: &[&str],
) -> Option<String> {
    let _ = (value, alignments, meet_or_slice);
    None
}
