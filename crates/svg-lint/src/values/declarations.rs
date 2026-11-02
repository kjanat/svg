//! CSS declaration lists (`style`).
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `CssDeclarationList`: a CSS declaration list.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_css_declaration_list(value: &str) -> Option<String> {
    let _ = value;
    None
}
