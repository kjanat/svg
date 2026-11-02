//! Keyword-enumerated value spaces.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Enum`: one of a fixed keyword set.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_enum(value: &str, values: &[&str]) -> Option<String> {
    let _ = (value, values);
    None
}
