//! Path data and animation number/coordinate lists.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `PathData`: SVG path data.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_path_data(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `SemicolonNumberList`: semicolon-separated numbers.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_semicolon_number_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `CoordinatePair`: one coordinate pair.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_coordinate_pair(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `CoordinatePairList`: semicolon-separated coordinate pairs.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_coordinate_pair_list(value: &str) -> Option<String> {
    let _ = value;
    None
}
