//! Booleans, ids and token lists.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Boolean`: `true` or `false`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_boolean(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `Id`: an XML name / id token.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_id(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `IdList`: whitespace-separated id references.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_id_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `TokenList`: whitespace-separated tokens.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_token_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `CommaTokenList`: comma-separated tokens.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_comma_token_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `UrlTokenList`: whitespace-separated URLs.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_url_token_list(value: &str) -> Option<String> {
    let _ = value;
    None
}
