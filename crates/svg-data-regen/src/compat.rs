//! Browser-compat-data extraction for objective catalog facts.
//!
//! The fetched support statements remain the source of truth, but BCD's key
//! shapes do not line up 1:1 with the catalog. This module therefore owns an
//! explicit compatibility-normalization layer: mapping BCD spellings to
//! canonical catalog names and classifying non-catalog subfeatures. That policy
//! lives here instead of being mixed into the semantic catalog assembly path.
//!
//! # Sources parsed
//!
//! Latest published package `data.json` (via unpkg, falling back to the npm
//! registry `https://registry.npmjs.org/<package>`):
//!
//! - `@mdn/browser-compat-data` [`data.json`][bcd] — browser support.
//! - `web-features` [`data.json`][webfeatures] — Baseline status.
//!
//! [bcd]: https://unpkg.com/@mdn/browser-compat-data/data.json
//! [webfeatures]: https://unpkg.com/web-features/data.json

use std::collections::{BTreeMap, btree_map::Entry};

use serde_json::Value;

use crate::{
    catalog::{
        CatalogBaselineStatus, CatalogBrowserSupport, CatalogCompatFacts, CatalogCompatProvenance,
        CatalogCompatSubfeature, CatalogCompatSubfeatureKind, CatalogPackageSource,
    },
    fetch,
    npm::package_source,
    util::boxed,
};

const BCD_PACKAGE: &str = "@mdn/browser-compat-data";
const WEB_FEATURES_PACKAGE: &str = "web-features";

use crate::Fallible;

/// Objective compat facts collected from BCD and web-features.
pub struct CompatCatalog {
    /// Source package versions and URLs.
    pub provenance: CatalogCompatProvenance,
    /// Facts keyed by SVG element name.
    pub elements: BTreeMap<String, CatalogCompatFacts>,
    /// Attribute facts keyed by canonical SVG attribute name.
    pub attributes: BTreeMap<String, CompatAttribute>,
}

/// Objective compat facts plus BCD-derived applicability for one attribute.
pub struct CompatAttribute {
    /// Objective facts from `/svg/global_attributes`, when BCD has a global record.
    pub global_facts: Option<CatalogCompatFacts>,
    /// Objective facts from `/svg/elements/<element>/<attribute>`.
    pub element_facts: BTreeMap<String, CatalogCompatFacts>,
}

impl CompatAttribute {
    /// Element names where BCD defines this as an element-local attribute.
    pub fn bearers(&self) -> impl Iterator<Item = &String> {
        self.element_facts.keys()
    }

    /// Whether BCD defines this under `/svg/global_attributes`.
    pub const fn is_global(&self) -> bool {
        self.global_facts.is_some()
    }

    /// Facts that can be safely used for attribute-wide fallback.
    pub fn common_facts(&self) -> Option<&CatalogCompatFacts> {
        if let Some(facts) = self.global_facts.as_ref() {
            return Some(facts);
        }
        let mut facts = self.element_facts.values();
        let first = facts.next()?;
        facts.all(|facts| facts == first).then_some(first)
    }
}

/// Fetch and parse browser-compat-data and web-features.
pub fn fetch_compat_catalog() -> Fallible<CompatCatalog> {
    let bcd_source = package_source(BCD_PACKAGE, "data.json")?;
    let web_features_source = package_source(WEB_FEATURES_PACKAGE, "data.json")?;
    fetch_compat_catalog_from_sources(bcd_source, web_features_source)
}

/// Reparse exact recorded packages, without upgrading metadata during a schema migration.
pub fn fetch_compat_catalog_from_sources(
    bcd_source: CatalogPackageSource,
    web_features_source: CatalogPackageSource,
) -> Fallible<CompatCatalog> {
    let bcd_json: Value =
        serde_json::from_str(&fetch::url_text(&bcd_source.url, "application/json")?)?;
    let web_features_json: Value = serde_json::from_str(&fetch::url_text(
        &web_features_source.url,
        "application/json",
    )?)?;

    let svg_elements = bcd_json
        .pointer("/svg/elements")
        .and_then(Value::as_object)
        .ok_or_else(|| boxed("browser-compat-data missing /svg/elements object"))?;
    let web_features = web_features_json.get("features");

    let mut elements = BTreeMap::new();
    let mut attributes = BTreeMap::new();
    let mut unmodeled_features = Vec::new();
    collect_element_facts(
        svg_elements,
        web_features,
        &mut elements,
        &mut attributes,
        &mut unmodeled_features,
    );
    collect_global_attribute_facts(&bcd_json, web_features, &mut attributes);

    Ok(CompatCatalog {
        provenance: CatalogCompatProvenance {
            browser_compat_data: bcd_source,
            web_features: web_features_source,
            unmodeled_features,
        },
        elements,
        attributes,
    })
}

fn collect_element_facts(
    svg_elements: &serde_json::Map<String, Value>,
    web_features: Option<&Value>,
    elements: &mut BTreeMap<String, CatalogCompatFacts>,
    attributes: &mut BTreeMap<String, CompatAttribute>,
    unmodeled_features: &mut Vec<CatalogCompatSubfeature>,
) {
    for (element_name, element_data) in svg_elements {
        if let Some(compat) = element_data.pointer("/__compat") {
            elements.insert(
                element_name.clone(),
                facts_from_compat(
                    compat,
                    web_features,
                    &format!("svg.elements.{element_name}"),
                ),
            );
        }
        let Some(attribute_map) = element_data.as_object() else {
            continue;
        };
        for (attribute_name, attribute_data) in attribute_map {
            if attribute_name == "__compat" {
                continue;
            }
            let Some(compat) = attribute_data.pointer("/__compat") else {
                continue;
            };
            let compat_key = format!("svg.elements.{element_name}.{attribute_name}");
            let Some(canonical) = bcd_attribute_name(attribute_name) else {
                if let Some(kind) = unmodeled_feature_kind(attribute_name) {
                    unmodeled_features.push(CatalogCompatSubfeature {
                        compat_key: compat_key.clone(),
                        kind,
                        element: element_name.clone(),
                        name: attribute_name.clone(),
                        facts: facts_from_compat(compat, web_features, &compat_key),
                    });
                }
                continue;
            };
            let facts = facts_from_compat(compat, web_features, &compat_key);
            merge_element_compat_attribute(attributes.entry(canonical), element_name, facts);
        }
    }
}

fn collect_global_attribute_facts(
    bcd_json: &Value,
    web_features: Option<&Value>,
    attributes: &mut BTreeMap<String, CompatAttribute>,
) {
    let Some(global_attributes) = bcd_json
        .pointer("/svg/global_attributes")
        .and_then(Value::as_object)
    else {
        return;
    };
    for (attribute_name, attribute_data) in global_attributes {
        let Some(compat) = attribute_data.pointer("/__compat") else {
            continue;
        };
        let Some(canonical) = bcd_attribute_name(attribute_name) else {
            continue;
        };
        let facts = facts_from_compat(
            compat,
            web_features,
            &format!("svg.global_attributes.{attribute_name}"),
        );
        merge_global_compat_attribute(attributes.entry(canonical), facts);
    }
}

fn facts_from_compat(
    compat: &Value,
    web_features: Option<&Value>,
    compat_key: &str,
) -> CatalogCompatFacts {
    CatalogCompatFacts {
        mdn_url: compat
            .get("mdn_url")
            .and_then(Value::as_str)
            .map(str::to_owned),
        deprecated: compat
            .pointer("/status/deprecated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        experimental: compat
            .pointer("/status/experimental")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        standard_track: compat
            .pointer("/status/standard_track")
            .and_then(Value::as_bool),
        baseline: resolve_baseline(web_features, compat_key),
        discouraged: crate::compat_model::resolve_discouraged(web_features, compat_key),
        browser_support: browser_support_from_compat(compat),
    }
}

fn browser_support_from_compat(compat: &Value) -> Option<CatalogBrowserSupport> {
    crate::browser_compat::extract_browser_support(compat)
}

fn resolve_baseline(
    web_features: Option<&Value>,
    compat_key: &str,
) -> Option<CatalogBaselineStatus> {
    crate::compat_model::resolve_baseline(web_features, compat_key)
}

fn merge_element_compat_attribute(
    entry: Entry<'_, String, CompatAttribute>,
    element_name: &str,
    new: CatalogCompatFacts,
) {
    match entry {
        Entry::Vacant(entry) => {
            let mut element_facts = BTreeMap::new();
            element_facts.insert(element_name.to_owned(), new);
            entry.insert(CompatAttribute {
                global_facts: None,
                element_facts,
            });
        }
        Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            match existing.element_facts.entry(element_name.to_owned()) {
                Entry::Vacant(entry) => {
                    entry.insert(new);
                }
                Entry::Occupied(mut entry) => {
                    merge_compat_facts(entry.get_mut(), new);
                }
            }
        }
    }
}

fn merge_global_compat_attribute(
    entry: Entry<'_, String, CompatAttribute>,
    new: CatalogCompatFacts,
) {
    match entry {
        Entry::Vacant(entry) => {
            entry.insert(CompatAttribute {
                global_facts: Some(new),
                element_facts: BTreeMap::new(),
            });
        }
        Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            if let Some(global_facts) = existing.global_facts.as_mut() {
                merge_compat_facts(global_facts, new);
            } else {
                existing.global_facts = Some(new);
            }
        }
    }
}

fn merge_compat_facts(existing: &mut CatalogCompatFacts, new: CatalogCompatFacts) {
    if existing.mdn_url.is_none() {
        existing.mdn_url.clone_from(&new.mdn_url);
    }
    existing.deprecated |= new.deprecated;
    existing.experimental |= new.experimental;
    crate::compat_model::merge_baseline(&mut existing.baseline, new.baseline);
    for advice in new.discouraged {
        if !existing.discouraged.contains(&advice) {
            existing.discouraged.push(advice);
        }
    }
    if let Some(new_support) = new.browser_support {
        merge_browser_support(&mut existing.browser_support, new_support);
    }
}

fn merge_browser_support(
    existing: &mut Option<CatalogBrowserSupport>,
    incoming: CatalogBrowserSupport,
) {
    let existing = existing.get_or_insert_default();
    for (browser, statements) in incoming {
        let current = existing.entry(browser).or_default();
        for statement in statements {
            if !current.contains(&statement) {
                current.push(statement);
            }
        }
    }
}

fn bcd_attribute_name(name: &str) -> Option<String> {
    if unmodeled_feature_kind(name).is_some() {
        return None;
    }
    canonical_attribute_name(name)
}

fn unmodeled_feature_kind(name: &str) -> Option<CatalogCompatSubfeatureKind> {
    // Compatibility-normalization policy: some BCD keys describe behaviors or
    // aliases that should be retained as compat records, not promoted to first-
    // class catalog attributes.
    match name {
        "data_uri" | "external_uri" | "omit_external_fragment" | "tooltip_display" => {
            Some(CatalogCompatSubfeatureKind::Behavior)
        }
        "xlink_href" => Some(CatalogCompatSubfeatureKind::LegacyXlinkAlias),
        _ => None,
    }
}

fn canonical_attribute_name(name: &str) -> Option<String> {
    // Compatibility-normalization policy: BCD key spellings are converted here
    // before they ever merge into catalog-facing attribute facts.
    match name {
        "data_attributes" => Some("data-*".to_owned()),
        "xlink_actuate" => Some("xlink:actuate".to_owned()),
        "xlink_href" => None,
        "xlink_show" => Some("xlink:show".to_owned()),
        "xlink_title" => Some("xlink:title".to_owned()),
        "xml_lang" => Some("xml:lang".to_owned()),
        "xml_space" => Some("xml:space".to_owned()),
        "referrerPolicy" => Some("referrerpolicy".to_owned()),
        other => Some(other.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_baseline_by_compat_key_override() {
        let web_features = serde_json::json!({
            "feature": {
                "compat_features": ["svg.elements.rect", "svg.elements.rect.width"],
                "status": {
                    "baseline": "high",
                    "baseline_high_date": "2022-01-01",
                    "by_compat_key": {
                        "svg.elements.rect.width": {
                            "baseline": "low",
                            "baseline_low_date": "2025-01-01"
                        }
                    }
                }
            }
        });

        assert_eq!(
            resolve_baseline(Some(&web_features), "svg.elements.rect.width"),
            crate::compat_model::parse_baseline(
                &serde_json::json!({"baseline":"low","baseline_low_date":"2025-01-01"})
            )
        );
    }

    #[test]
    fn resolves_qualified_baseline_dates() {
        let web_features = serde_json::json!({
            "feature": {
                "compat_features": ["svg.elements.feGaussianBlur"],
                "status": {
                    "baseline": "high",
                    "baseline_high_date": "2018-01-29",
                    "by_compat_key": {
                        "svg.elements.feGaussianBlur": {
                            "baseline": "high",
                            "baseline_high_date": "\u{2264}2021-04-02"
                        }
                    }
                }
            }
        });

        assert_eq!(
            resolve_baseline(Some(&web_features), "svg.elements.feGaussianBlur"),
            crate::compat_model::parse_baseline(
                &serde_json::json!({"baseline":"high","baseline_high_date":"≤2021-04-02"})
            )
        );
    }

    #[test]
    fn bcd_attribute_names_are_normalized_and_subfeatures_skipped() {
        assert_eq!(bcd_attribute_name("xml_lang").as_deref(), Some("xml:lang"));
        assert_eq!(
            bcd_attribute_name("data_attributes").as_deref(),
            Some("data-*")
        );
        assert_eq!(bcd_attribute_name("path").as_deref(), Some("path"));
        assert_eq!(bcd_attribute_name("data_uri"), None);
        assert_eq!(bcd_attribute_name("xlink_href"), None);
    }

    #[test]
    fn compat_normalization_boundary_is_explicit() {
        assert_eq!(
            unmodeled_feature_kind("xlink_href"),
            Some(CatalogCompatSubfeatureKind::LegacyXlinkAlias)
        );
        assert_eq!(
            canonical_attribute_name("referrerPolicy").as_deref(),
            Some("referrerpolicy")
        );
        assert_eq!(bcd_attribute_name("xlink_href"), None);
    }
}
