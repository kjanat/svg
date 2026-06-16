//! Parsing helpers over MDN browser-compat-data, used to derive baseline and
//! per-browser support facts.
//!
//! These operate on the raw compat JSON so the LSP can reconcile baseline /
//! support at runtime against the same data the catalog was built from.

use crate::BaselineStatus;

/// A single browser's `version_added`, resolved to a comparable form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserVersion {
    /// Supported, but the first version is unknown.
    Unknown,
    /// Supported since the given version string.
    Version(String),
}

/// Per-browser `version_added` for the four tracked engines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserVersions {
    /// Chrome support.
    pub chrome: Option<BrowserVersion>,
    /// Edge support.
    pub edge: Option<BrowserVersion>,
    /// Firefox support.
    pub firefox: Option<BrowserVersion>,
    /// Safari support.
    pub safari: Option<BrowserVersion>,
}

/// Extract per-browser support from a compat record's `support` block.
#[must_use]
pub const fn extract_browser_versions(compat: &serde_json::Value) -> Option<BrowserVersions> {
    let _ = compat;
    // Populated by the extraction pipeline; `None` until it lands.
    None
}

/// Resolve a feature's web-platform baseline from compat + web-features data.
#[must_use]
pub const fn resolve_baseline(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> Option<BaselineStatus> {
    let _ = (compat, wf_features, compat_key);
    None
}
