//! Lossless Web Features facts shared by catalog generation and runtime parsing.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A recognized upstream Baseline tier, independent of dates and BCD flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BaselineTier {
    /// Widely Available (`"high"` upstream).
    Widely,
    /// Newly Available (`"low"` upstream).
    Newly,
    /// Non-Baseline (`false` upstream), including discouraged features.
    Limited,
}

/// Known inexactness qualifier on a date or browser version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BaselineQualifier {
    /// On or before the given date/version.
    Before,
    /// On or after the given date/version.
    After,
    /// Approximately the given date/version.
    Approximately,
}

/// A full upstream date, with a parsed value only when its meaning is recognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BaselineDate<S = String> {
    /// Original upstream string, including any qualifier or malformed input.
    pub raw: S,
    /// Valid calendar date in YYYY-MM-DD form.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<S>,
    /// Recognized comparison qualifier; unknown prefixes are never approximated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<BaselineQualifier>,
}

/// Why an upstream status did not yield a recognized tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BaselineDiagnostic {
    /// The status block has dates but no baseline field.
    Missing,
    /// The baseline field is present but not false, low, or high.
    Unrecognized,
}

/// Baseline facts. A missing recognized status is never Limited availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Baseline<S = String> {
    /// Recognized tier, independently of whether either date can be parsed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BaselineTier>,
    /// Original baseline field encoded as JSON, preserving null and invalid types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_status: Option<S>,
    /// Parsing diagnostic, separate from the upstream status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_diagnostic: Option<BaselineDiagnostic>,
    /// Optional milestone when the feature became Newly Available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_date: Option<BaselineDate<S>>,
    /// Optional milestone when the feature became Widely Available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high_date: Option<BaselineDate<S>>,
}

impl<S> Baseline<S> {
    /// No upstream facts or inferred tier.
    pub const EMPTY: Self = Self {
        status: None,
        raw_status: None,
        status_diagnostic: None,
        low_date: None,
        high_date: None,
    };
}

impl<S: AsRef<str>> BaselineDate<S> {
    /// Borrow the retained strings without losing raw input.
    #[must_use]
    pub fn as_ref(&self) -> BaselineDate<&str> {
        BaselineDate {
            raw: self.raw.as_ref(),
            date: self.date.as_ref().map(AsRef::as_ref),
            qualifier: self.qualifier,
        }
    }

    /// Derive a coarse year for presentation only when a date was recognized.
    #[must_use]
    pub fn year(&self) -> Option<u16> {
        self.date.as_ref()?.as_ref().get(..4)?.parse().ok()
    }
}

impl<S: AsRef<str>> Baseline<S> {
    /// Borrow either generated static facts or owned runtime facts uniformly.
    #[must_use]
    pub fn as_ref(&self) -> Baseline<&str> {
        Baseline {
            status: self.status,
            raw_status: self.raw_status.as_ref().map(AsRef::as_ref),
            status_diagnostic: self.status_diagnostic,
            low_date: self.low_date.as_ref().map(BaselineDate::as_ref),
            high_date: self.high_date.as_ref().map(BaselineDate::as_ref),
        }
    }

    /// The date of the recognized tier, without inventing a missing milestone.
    #[must_use]
    pub const fn milestone(&self) -> Option<&BaselineDate<S>> {
        match self.status {
            Some(BaselineTier::Widely) => self.high_date.as_ref(),
            Some(BaselineTier::Newly) => self.low_date.as_ref(),
            _ => None,
        }
    }
}

/// Feature-scoped `WebDX` discouragement; independent of Baseline and BCD deprecation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Discouraged<S = String, L = Vec<S>> {
    /// Web Features identity, including when facts are aggregated across SVG contexts.
    pub feature_id: S,
    /// BCD compatibility key through which this feature applies.
    pub compat_key: S,
    /// Scope of the advice (always the whole Web Features feature).
    pub scope: DiscouragementScope,
    /// Human-readable feature name when provided upstream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_name: Option<S>,
    /// Upstream explanation, rendered as text.
    pub reason: S,
    /// Original HTML explanation, retained for consumers but not rendered as trusted HTML.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_html: Option<S>,
    /// Supporting specification or vendor notices.
    pub according_to: L,
    /// Alternative Web Features identifiers.
    pub alternatives: L,
    /// Optional upstream removal date; no inferred removal policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removal_date: Option<S>,
}

/// Scope of `WebDX` discouragement metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscouragementScope {
    /// Advice applies to the named Web Features feature.
    Feature,
}

/// Parse only full, valid calendar dates with known prefixes; preserve every raw string.
#[must_use]
pub fn parse_baseline_date(raw: &str) -> BaselineDate {
    let mut result = BaselineDate {
        raw: raw.to_owned(),
        date: None,
        qualifier: None,
    };
    let prefixes = [
        ("<=", BaselineQualifier::Before),
        (">=", BaselineQualifier::After),
        ("≤", BaselineQualifier::Before),
        ("≥", BaselineQualifier::After),
        ("<", BaselineQualifier::Before),
        (">", BaselineQualifier::After),
        ("~", BaselineQualifier::Approximately),
        ("≈", BaselineQualifier::Approximately),
    ];
    let (date, qualifier) = prefixes
        .iter()
        .find_map(|(prefix, qualifier)| {
            raw.strip_prefix(prefix)
                .map(|date| (date, Some(*qualifier)))
        })
        .unwrap_or((raw, None));
    if valid_date(date) {
        result.date = Some(date.to_owned());
        result.qualifier = qualifier;
    }
    result
}

fn valid_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let Some(year) = date.get(..4).and_then(|s| s.parse::<u16>().ok()) else {
        return false;
    };
    let Some(month) = date.get(5..7).and_then(|s| s.parse::<u8>().ok()) else {
        return false;
    };
    let Some(day) = date.get(8..).and_then(|s| s.parse::<u8>().ok()) else {
        return false;
    };
    let days = match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    year > 0 && day > 0 && day <= days
}

/// Import upstream tiers without calculating eligibility or requiring dates.
#[must_use]
pub fn parse_baseline(status: &Value) -> Option<Baseline> {
    let raw = status.get("baseline");
    let date = |key| {
        status.get(key).map(|value| {
            value.as_str().map_or_else(
                || BaselineDate {
                    raw: value.to_string(),
                    date: None,
                    qualifier: None,
                },
                parse_baseline_date,
            )
        })
    };
    let low_date = date("baseline_low_date");
    let high_date = date("baseline_high_date");
    if raw.is_none() && low_date.is_none() && high_date.is_none() {
        return None;
    }
    let tier = match raw {
        Some(Value::Bool(false)) => Some(BaselineTier::Limited),
        Some(Value::String(s)) if s == "low" => Some(BaselineTier::Newly),
        Some(Value::String(s)) if s == "high" => Some(BaselineTier::Widely),
        _ => None,
    };
    Some(Baseline {
        status: tier,
        raw_status: raw.map(Value::to_string),
        status_diagnostic: if tier.is_some() {
            None
        } else if raw.is_none() {
            Some(BaselineDiagnostic::Missing)
        } else {
            Some(BaselineDiagnostic::Unrecognized)
        },
        low_date,
        high_date,
    })
}

/// Resolve per-compat-key status first, including an explicitly empty override.
#[must_use]
pub fn resolve_baseline(features: Option<&Value>, compat_key: &str) -> Option<Baseline> {
    let feature = features?
        .as_object()?
        .values()
        .find(|feature| matches_key(feature, compat_key))?;
    let status = feature.get("status")?;
    parse_baseline(
        status
            .get("by_compat_key")
            .and_then(|keys| keys.get(compat_key))
            .unwrap_or(status),
    )
}

fn matches_key(feature: &Value, compat_key: &str) -> bool {
    feature
        .get("compat_features")
        .and_then(Value::as_array)
        .is_some_and(|keys| keys.iter().any(|key| key.as_str() == Some(compat_key)))
}

/// Retain all applicable feature identities and advice, without changing their statuses.
#[must_use]
pub fn resolve_discouraged(features: Option<&Value>, compat_key: &str) -> Vec<Discouraged> {
    let Some(features) = features.and_then(Value::as_object) else {
        return Vec::new();
    };
    features
        .iter()
        .filter(|(_, feature)| matches_key(feature, compat_key))
        .filter_map(|(id, feature)| {
            let advice = feature.get("discouraged")?.as_object()?;
            let string = |key| advice.get(key).and_then(Value::as_str).map(str::to_owned);
            let list = |key| {
                advice
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default()
            };
            Some(Discouraged {
                feature_id: id.clone(),
                compat_key: compat_key.to_owned(),
                scope: DiscouragementScope::Feature,
                feature_name: feature
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                reason: string("reason").unwrap_or_default(),
                reason_html: string("reason_html"),
                according_to: list("according_to"),
                alternatives: list("alternatives"),
                removal_date: string("removal_date"),
            })
        })
        .collect()
}

/// Preserve the existing aggregate policy without classifying unknown data as Limited.
pub fn merge_baseline(existing: &mut Option<Baseline>, incoming: Option<Baseline>) {
    let Some(new) = incoming else {
        return;
    };
    let rank = |baseline: &Baseline| match baseline.status {
        Some(BaselineTier::Limited) => 0,
        Some(BaselineTier::Newly) => 1,
        Some(BaselineTier::Widely) => 2,
        None => 3,
    };
    let should_replace = existing.as_ref().is_none_or(|current| {
        rank(&new) < rank(current)
            || (rank(&new) == rank(current)
                && new.milestone().and_then(|date| date.date.as_ref())
                    > current.milestone().and_then(|date| date.date.as_ref()))
    });
    if should_replace {
        *existing = Some(new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_fixtures_preserve_imported_facts_through_serialization()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/web-features.json"))?;
        for case in fixture["cases"].as_array().ok_or("fixture cases")? {
            let baseline = parse_baseline(&case["input"]);
            let serialized = serde_json::to_value(&baseline)?;
            assert_eq!(serialized, case["expected"], "{}", case["name"]);
            let restored: Option<Baseline> = serde_json::from_value(serialized)?;
            assert_eq!(restored, baseline, "{}", case["name"]);
        }
        Ok(())
    }

    #[test]
    fn per_key_status_is_authoritative_even_when_empty_or_unknown()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/web-features.json"))?;
        let features = Some(&fixture["features"]);
        let baseline = resolve_baseline(features, "svg.elements.rect").ok_or("fixture value")?;
        assert_eq!(baseline.status, Some(BaselineTier::Widely));
        assert_eq!(
            baseline.low_date.ok_or("fixture value")?.date.as_deref(),
            Some("2020-01-15")
        );
        assert_eq!(
            baseline.high_date.ok_or("fixture value")?.date.as_deref(),
            Some("2022-07-15")
        );
        let width = resolve_baseline(features, "svg.elements.rect.width").ok_or("fixture value")?;
        assert_eq!(width.status, Some(BaselineTier::Newly));
        assert!(width.low_date.is_none());
        assert!(width.high_date.is_none());
        assert!(resolve_baseline(features, "svg.elements.rect.opacity").is_none());
        let height =
            resolve_baseline(features, "svg.elements.rect.height").ok_or("fixture value")?;
        assert!(height.status.is_none());
        assert_eq!(height.raw_status.as_deref(), Some("null"));
        Ok(())
    }

    #[test]
    fn discouragement_retains_feature_scope_without_changing_baseline()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/web-features.json"))?;
        let features = Some(&fixture["features"]);
        let advice = resolve_discouraged(features, "svg.elements.legacy");
        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].feature_id, "legacy-svg");
        assert_eq!(advice[0].compat_key, "svg.elements.legacy");
        assert_eq!(advice[0].scope, DiscouragementScope::Feature);
        assert_eq!(advice[0].according_to, ["https://example.com/retirement"]);
        assert_eq!(advice[0].alternatives, ["svg"]);
        assert_eq!(
            advice[0].reason_html.as_deref(),
            Some("Use a <em>modern</em> SVG feature.")
        );
        let restored: Vec<Discouraged> = serde_json::from_value(serde_json::to_value(&advice)?)?;
        assert_eq!(restored, advice);
        assert_eq!(
            resolve_baseline(features, "svg.elements.legacy")
                .ok_or("fixture value")?
                .status,
            Some(BaselineTier::Limited)
        );
        assert_eq!(resolve_discouraged(features, "svg.elements.limited"), []);
        assert_eq!(resolve_discouraged(features, "svg.elements.deprecated"), []);
        Ok(())
    }
}
