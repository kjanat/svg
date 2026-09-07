//! Parsing helpers over MDN browser-compat-data, used to derive baseline and
//! per-browser support facts.
//!
//! These operate on the raw compat JSON so the LSP can reconcile baseline /
//! support at runtime against the same data the catalog was built from.

use crate::compat_model::Baseline as BaselineStatus;

/// A single browser's `version_added`, resolved to a comparable form.
///
/// # Examples
///
/// ```rust
/// let version = svg_data::compat_parse::BrowserVersion::Version("124".to_owned());
/// assert!(matches!(version, svg_data::compat_parse::BrowserVersion::Version(_)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserVersion {
    /// Supported, but the first version is unknown.
    Unknown,
    /// Supported since the given version string.
    Version(String),
}

/// Per-browser `version_added` for the four displayed desktop browser products.
///
/// # Examples
///
/// ```rust
/// let versions = svg_data::compat_parse::BrowserVersions {
///     chrome: Some(svg_data::compat_parse::BrowserVersion::Unknown),
///     edge: None,
///     firefox: None,
///     safari: None,
/// };
/// assert!(versions.chrome.is_some());
/// ```
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
///
/// # Examples
///
/// ```rust
/// let compat = serde_json::json!({"support":{"chrome":{"version_added":"1"}}});
/// let versions = svg_data::compat_parse::extract_browser_versions(&compat).expect("support");
/// assert!(versions.chrome.is_some());
/// ```
#[must_use]
pub fn extract_browser_versions(compat: &serde_json::Value) -> Option<BrowserVersions> {
    let support = compat.get("support")?.as_object()?;
    Some(BrowserVersions {
        chrome: support.get("chrome").and_then(browser_version_from_support),
        edge: support.get("edge").and_then(browser_version_from_support),
        firefox: support
            .get("firefox")
            .and_then(browser_version_from_support),
        safari: support.get("safari").and_then(browser_version_from_support),
    })
}

fn browser_version_from_support(value: &serde_json::Value) -> Option<BrowserVersion> {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .find_map(browser_version_from_support_statement);
    }
    browser_version_from_support_statement(value)
}

fn browser_version_from_support_statement(value: &serde_json::Value) -> Option<BrowserVersion> {
    match value.get("version_added")? {
        serde_json::Value::String(version) => Some(BrowserVersion::Version(version.clone())),
        serde_json::Value::Bool(true) | serde_json::Value::Null => Some(BrowserVersion::Unknown),
        _ => None,
    }
}

/// Resolve a feature's web-platform baseline from compat + web-features data.
///
/// # Examples
///
/// ```rust
/// let compat = serde_json::json!({});
/// assert!(svg_data::compat_parse::resolve_baseline(&compat, None, "svg.elements.svg").is_none());
/// ```
#[must_use]
pub fn resolve_baseline(
    compat: &serde_json::Value,
    wf_features: Option<&serde_json::Value>,
    compat_key: &str,
) -> Option<BaselineStatus> {
    let _ = compat;
    crate::compat_model::resolve_baseline(wf_features, compat_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_browser_versions_from_support_block() {
        let compat = serde_json::json!({
            "support": {
                "chrome": { "version_added": "50" },
                "edge": { "version_added": true },
                "firefox": { "version_added": false },
                "safari": { "version_added": null }
            }
        });

        let Some(versions) = extract_browser_versions(&compat) else {
            panic!("versions should parse");
        };
        assert_eq!(
            versions.chrome,
            Some(BrowserVersion::Version("50".to_owned()))
        );
        assert_eq!(versions.edge, Some(BrowserVersion::Unknown));
        assert_eq!(versions.firefox, None);
        assert_eq!(versions.safari, Some(BrowserVersion::Unknown));
    }

    #[test]
    fn extracts_first_supported_statement_from_arrays() {
        let compat = serde_json::json!({
            "support": {
                "chrome": [
                    { "version_added": false },
                    { "version_added": "80" }
                ]
            }
        });

        let Some(versions) = extract_browser_versions(&compat) else {
            panic!("versions should parse");
        };
        assert_eq!(
            versions.chrome,
            Some(BrowserVersion::Version("80".to_owned()))
        );
    }

    #[test]
    fn resolves_baseline_from_web_features_by_compat_key() {
        let compat = serde_json::json!({});
        let wf = serde_json::json!({
            "svg": {
                "compat_features": ["svg.elements.rect", "svg.elements.rect.width"],
                "status": {
                    "baseline": "high",
                    "baseline_high_date": "2022-07-15",
                    "baseline_low_date": "2020-01-15",
                    "by_compat_key": {
                        "svg.elements.rect.width": {
                            "baseline": "low",
                            "baseline_low_date": "2025-05-01"
                        }
                    }
                }
            }
        });

        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.rect.width"),
            crate::compat_model::parse_baseline(
                &serde_json::json!({"baseline":"low","baseline_low_date":"2025-05-01"})
            )
        );
        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.rect"),
            crate::compat_model::parse_baseline(
                &serde_json::json!({"baseline":"high","baseline_high_date":"2022-07-15","baseline_low_date":"2020-01-15"})
            )
        );
    }

    #[test]
    fn qualified_baseline_date_yields_year_and_qualifier() {
        // web-features writes approximate baseline dates with a `≤` prefix; the
        // year must survive and the qualifier must be captured (not dropped).
        let compat = serde_json::json!({});
        let wf = serde_json::json!({
            "svg": {
                "compat_features": ["svg.elements.rect"],
                "status": {
                    "baseline": "high",
                    "baseline_high_date": "\u{2264}2020-01-05",
                    "baseline_low_date": "\u{2264}2018-01-05"
                }
            }
        });

        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.rect"),
            crate::compat_model::parse_baseline(
                &serde_json::json!({"baseline":"high","baseline_high_date":"≤2020-01-05","baseline_low_date":"≤2018-01-05"})
            )
        );
    }

    #[test]
    fn resolves_limited_baseline_from_false_status() {
        let compat = serde_json::json!({});
        let wf = serde_json::json!({
            "feature": {
                "compat_features": ["svg.elements.a.referrerPolicy"],
                "status": {
                    "baseline": false
                }
            }
        });

        assert_eq!(
            resolve_baseline(&compat, Some(&wf), "svg.elements.a.referrerPolicy"),
            crate::compat_model::parse_baseline(&serde_json::json!({"baseline":false}))
        );
    }
}
