//! Numbers, integers and number-or-percentage values.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Number`: a `<number>`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_number(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `Integer`: an `<integer>`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_integer(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `NumberOrPercentage`: a `<number>` or `<percentage>`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_number_or_percentage(value: &str) -> Option<String> {
    let _ = value;
    None
}
