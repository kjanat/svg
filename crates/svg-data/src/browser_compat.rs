//! Browser support facts shared by catalog generation and runtime refresh.
//! All products and support statements are retained. Selection is a presentation operation.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The literal `false` alternative in BCD's version union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "bool", into = "bool")]
pub struct Unsupported;

impl JsonSchema for Unsupported {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Unsupported".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "boolean", "const": false})
    }
}

impl TryFrom<bool> for Unsupported {
    type Error = &'static str;
    fn try_from(value: bool) -> Result<Self, Self::Error> {
        if value {
            Err("version_added must be a string or false")
        } else {
            Ok(Self)
        }
    }
}

impl From<Unsupported> for bool {
    fn from(_: Unsupported) -> Self {
        false
    }
}

/// BCD's `string | false` version value, including qualified versions and `preview`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VersionAdded<S = String> {
    /// Original version string.
    Version(S),
    /// Explicitly unsupported.
    Unsupported(Unsupported),
}

/// BCD's two flag categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FlagKind {
    /// A browser preference.
    Preference,
    /// A command-line/runtime flag.
    RuntimeFlag,
}

impl FlagKind {
    /// Upstream spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::RuntimeFlag => "runtime_flag",
        }
    }
}

/// A complete BCD flag declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrowserFlag<S = String> {
    /// Required upstream category.
    #[serde(rename = "type")]
    pub kind: FlagKind,
    /// Flag or preference name.
    pub name: S,
    /// Required setting value, if supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_to_set: Option<S>,
}

/// One complete support statement. Collections normalize upstream singleton/array forms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(bound = "S: JsonSchema, Strings: JsonSchema + Default, Flags: JsonSchema + Default")]
pub struct BrowserVersion<S = String, Strings = Vec<S>, Flags = Vec<BrowserFlag<S>>> {
    /// Upstream version union. Absent/null/invalid input remains unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_added: Option<VersionAdded<S>>,
    /// First version without support, retaining its qualifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_removed: Option<S>,
    /// Last version with support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_last: Option<S>,
    /// Required prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<S>,
    /// Name used by this implementation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative_name: Option<S>,
    /// All flag declarations.
    #[serde(default)]
    pub flags: Flags,
    /// Implementation tracking links.
    #[serde(default)]
    pub impl_url: Strings,
    /// Whether mandatory behavior is missing.
    #[serde(default)]
    pub partial_implementation: bool,
    /// Original upstream notes, with HTML retained as data.
    #[serde(default)]
    pub notes: Strings,
}

impl<S: AsRef<str>, Strings, Flags> BrowserVersion<S, Strings, Flags> {
    /// Explicit support state, independent of the display version.
    #[must_use]
    pub const fn supported(&self) -> Option<bool> {
        match self.version_added {
            Some(VersionAdded::Version(_)) => Some(true),
            Some(VersionAdded::Unsupported(_)) => Some(false),
            None => None,
        }
    }

    /// Original version string when present.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        match &self.version_added {
            Some(VersionAdded::Version(v)) => Some(v.as_ref()),
            _ => None,
        }
    }
}

/// Every upstream product, including mobile devices, runtimes and future product IDs.
pub type BrowserSupport = BTreeMap<String, Vec<BrowserVersion>>;

/// Parse all BCD products and history without selecting a display statement.
#[must_use]
pub fn extract_browser_support(compat: &Value) -> Option<BrowserSupport> {
    let support = compat.get("support")?.as_object()?;
    Some(
        support
            .iter()
            .map(|(id, value)| {
                let values = value
                    .as_array()
                    .map_or_else(|| vec![value], |v| v.iter().collect());
                (
                    id.clone(),
                    values.into_iter().filter_map(parse_statement).collect(),
                )
            })
            .collect(),
    )
}

fn parse_statement(value: &Value) -> Option<BrowserVersion> {
    let mut value = value.as_object()?.clone();
    for key in ["notes", "impl_url"] {
        if let Some(Value::String(text)) = value.get(key) {
            value.insert(key.to_owned(), serde_json::json!([text]));
        }
    }
    // An unknown version must not turn an otherwise useful statement into an unsupported one.
    if value
        .get("version_added")
        .is_some_and(|v| !v.is_string() && v != false)
    {
        value.remove("version_added");
    }
    serde_json::from_value(Value::Object(value)).ok()
}

/// Choose the current unrestricted implementation for concise display, retaining history elsewhere.
#[must_use]
pub fn select_statement(statements: &[BrowserVersion]) -> Option<&BrowserVersion> {
    statements
        .iter()
        .find(|v| {
            v.supported() == Some(true)
                && v.version_removed.is_none()
                && v.flags.is_empty()
                && v.prefix.is_none()
                && v.alternative_name.is_none()
                && !v.partial_implementation
        })
        .or_else(|| {
            statements
                .iter()
                .find(|v| v.supported() == Some(true) && v.version_removed.is_none())
        })
        .or_else(|| statements.first())
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn full_history_and_every_product_survive_parsing() -> TestResult {
        let raw: Value = serde_json::from_str(include_str!("fixtures/browser-support.json"))?;
        let support = extract_browser_support(&raw).ok_or("support")?;
        assert_eq!(support.len(), 5);
        let chrome = support.get("chrome").ok_or("chrome")?;
        assert_eq!(chrome.len(), 2);
        let old = &chrome[0];
        assert_eq!(old.version(), Some("≤20"));
        assert_eq!(old.version_removed.as_deref(), Some("70"));
        assert_eq!(old.version_last.as_deref(), Some("69"));
        assert_eq!(old.prefix.as_deref(), Some("-webkit-"));
        assert_eq!(old.alternative_name.as_deref(), Some("oldFeature"));
        assert_eq!(old.flags[0].kind, FlagKind::Preference);
        assert_eq!(old.flags[0].value_to_set.as_deref(), Some("true"));
        assert_eq!(old.flags[1].kind, FlagKind::RuntimeFlag);
        assert_eq!(old.impl_url.len(), 2);
        assert_eq!(old.notes, ["First caveat", "A <code>second</code> caveat"]);
        assert!(old.partial_implementation);
        assert_eq!(
            select_statement(chrome).and_then(BrowserVersion::version),
            Some("80")
        );
        assert_eq!(
            support["safari_ios"][0].impl_url,
            ["https://example.com/ios"]
        );
        assert_eq!(support["future_device"][0].supported(), Some(false));
        assert_eq!(support["firefox"][0].version(), Some("preview"));
        let encoded = serde_json::to_value(&support)?;
        assert_eq!(encoded["future_device"][0]["version_added"], false);
        assert_eq!(serde_json::from_value::<BrowserSupport>(encoded)?, support);
        Ok(())
    }

    #[test]
    fn exact_web_features_support_does_not_inherit_the_feature_summary() -> TestResult {
        let raw: Value = serde_json::from_str(include_str!("fixtures/browser-support.json"))?;
        let features = serde_json::json!({"fixture": {"compat_features": ["svg.elements.rect"], "status": raw["status"]}});
        let baseline = crate::compat_model::resolve_baseline(Some(&features), "svg.elements.rect")
            .ok_or("baseline")?;
        assert_eq!(
            baseline.support.as_ref().ok_or("Web Features support")?["chrome"],
            "80"
        );
        assert_eq!(
            baseline.support.as_ref().ok_or("Web Features support")?["safari_ios"],
            "18.4"
        );
        let mut empty = features;
        empty["fixture"]["status"]["by_compat_key"]["svg.elements.rect"] =
            serde_json::json!({"support": {}});
        let baseline = crate::compat_model::resolve_baseline(Some(&empty), "svg.elements.rect")
            .ok_or("empty support retained")?;
        assert!(baseline.support.ok_or("empty support")?.is_empty());
        assert_eq!(baseline.status, None);
        Ok(())
    }

    #[test]
    fn version_and_flag_types_reject_invalid_upstream_values() {
        assert!(serde_json::from_value::<VersionAdded>(serde_json::json!(true)).is_err());
        assert!(
            serde_json::from_value::<BrowserFlag>(serde_json::json!({"name":"missing-category"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<BrowserFlag>(
                serde_json::json!({"name":"bad-category", "type":"other"})
            )
            .is_err()
        );
    }
}
