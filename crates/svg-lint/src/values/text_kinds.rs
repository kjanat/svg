//! Language tags, media types and queries, referrer policies, file names.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `LanguageTag`: a BCP 47 tag or empty.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_language_tag(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `MediaType`: a MIME type.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_media_type(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `MediaQueryList`: a media query list.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_media_query_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `ReferrerPolicy`: a referrer policy token.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_referrer_policy(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `SuggestedFileName`: a download file name.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_suggested_file_name(value: &str) -> Option<String> {
    let _ = value;
    None
}
