//! Parsing helpers over MDN browser-compat-data, used to derive baseline and
//! per-browser support facts.
//!
//! These operate on the raw compat JSON so the LSP can reconcile baseline /
//! support at runtime against the same data the catalog was built from.

use crate::effective_compat::{BrowserFlag, BrowserVersion};

/// Parse complete BCD support statements, preserving unknown and unsupported states.
#[must_use]
pub fn extract_browser_support(
    compat: &serde_json::Value,
) -> Option<crate::effective_compat::BrowserSupport> {
    use crate::effective_compat::BrowserSupport;
    let support = compat.get("support")?.as_object()?;
    Some(BrowserSupport {
        chrome: support.get("chrome").and_then(browser_version),
        edge: support.get("edge").and_then(browser_version),
        firefox: support.get("firefox").and_then(browser_version),
        safari: support.get("safari").and_then(browser_version),
    })
}
fn browser_version(value: &serde_json::Value) -> Option<crate::effective_compat::BrowserVersion> {
    let statement = if let Some(items) = value.as_array() {
        // Prefer a current, unqualified implementation. Retain a historical or
        // gated statement when that is all upstream knows about this browser.
        items
            .iter()
            .find(|v| {
                v.get("version_removed")
                    .is_none_or(serde_json::Value::is_null)
                    && v.get("flags").is_none()
                    && v.get("prefix").is_none()
                    && v.get("version_added")
                        .is_some_and(|v| v != false && !v.is_null())
            })
            .or_else(|| items.iter().find(|v| v.is_object()))?
    } else {
        value
    };
    if !statement.is_object() {
        return None;
    }
    let text = |name| {
        statement
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let raw = statement.get("version_added");
    let added = text("version_added");
    let removed = text("version_removed");
    Some(BrowserVersion {
        supported: raw.and_then(|v| v.as_bool().or_else(|| v.as_str().map(|_| true))),
        version_qualifier: added.as_deref().and_then(version_qualifier),
        version_removed_qualifier: removed.as_deref().and_then(version_qualifier),
        version_added: added,
        version_removed: removed,
        partial_implementation: statement
            .get("partial_implementation")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        prefix: text("prefix"),
        alternative_name: text("alternative_name"),
        notes: match statement.get("notes") {
            Some(serde_json::Value::String(s)) => vec![s.clone()],
            Some(serde_json::Value::Array(a)) => a
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        },
        flags: statement
            .get("flags")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| {
                Some(BrowserFlag {
                    name: v.get("name")?.as_str()?.to_owned(),
                    kind: v
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    value_to_set: v
                        .get("value_to_set")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                })
            })
            .collect(),
    })
}
fn version_qualifier(version: &str) -> Option<crate::BaselineQualifier> {
    if version.starts_with(['≤', '<']) {
        Some(crate::BaselineQualifier::Before)
    } else if version.starts_with(['≥', '>']) {
        Some(crate::BaselineQualifier::After)
    } else if version.starts_with(['~', '≈']) {
        Some(crate::BaselineQualifier::Approximately)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_states_and_caveats_are_not_collapsed() -> Result<(), Box<dyn std::error::Error>> {
        let raw = serde_json::json!({"support":{
            "chrome":{"version_added":"≤50", "version_removed":"70", "flags":[{"name":"demo"}], "notes":["first", "second"], "prefix":"-x-", "partial_implementation":true},
            "edge":{"version_added":true}, "firefox":{"version_added":false}, "safari":{"version_added":null}
        }});
        let s = extract_browser_support(&raw).ok_or("support")?;
        let chrome = s.chrome.ok_or("chrome")?;
        assert_eq!(
            chrome.version_qualifier,
            Some(crate::BaselineQualifier::Before)
        );
        assert_eq!(chrome.version_removed.as_deref(), Some("70"));
        assert_eq!(chrome.notes, ["first", "second"]);
        assert_eq!(chrome.flags[0].name, "demo");
        assert_eq!(s.edge.ok_or("edge")?.supported, Some(true));
        assert_eq!(s.firefox.ok_or("firefox")?.supported, Some(false));
        assert_eq!(s.safari.ok_or("safari")?.supported, None);
        Ok(())
    }

    #[test]
    fn resolves_baseline_from_web_features_by_compat_key() {
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
            crate::compat_model::resolve_baseline(Some(&wf), "svg.elements.rect.width"),
            crate::compat_model::parse_baseline(
                &serde_json::json!({"baseline":"low","baseline_low_date":"2025-05-01"})
            )
        );
        assert_eq!(
            crate::compat_model::resolve_baseline(Some(&wf), "svg.elements.rect"),
            crate::compat_model::parse_baseline(
                &serde_json::json!({"baseline":"high","baseline_high_date":"2022-07-15","baseline_low_date":"2020-01-15"})
            )
        );
    }

    #[test]
    fn qualified_baseline_date_yields_year_and_qualifier() {
        // web-features writes approximate baseline dates with a `≤` prefix; the
        // year must survive and the qualifier must be captured (not dropped).
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
            crate::compat_model::resolve_baseline(Some(&wf), "svg.elements.rect"),
            crate::compat_model::parse_baseline(
                &serde_json::json!({"baseline":"high","baseline_high_date":"≤2020-01-05","baseline_low_date":"≤2018-01-05"})
            )
        );
    }

    #[test]
    fn resolves_limited_baseline_from_false_status() {
        let wf = serde_json::json!({
            "feature": {
                "compat_features": ["svg.elements.a.referrerPolicy"],
                "status": {
                    "baseline": false
                }
            }
        });

        assert_eq!(
            crate::compat_model::resolve_baseline(Some(&wf), "svg.elements.a.referrerPolicy"),
            crate::compat_model::parse_baseline(&serde_json::json!({"baseline":false}))
        );
    }
}
