//! Resolve runtime facts once per context, independently for each upstream source.
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use svg_data::{
    compat_model,
    effective_compat::{self, Facts},
};

/// Outcome of one source lookup. Absence is authoritative only after a valid load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Loaded,
    Absent,
    Unknown,
    Failed,
    Disabled,
}

#[derive(Clone, Debug)]
pub struct Provenance {
    pub source: &'static str,
    pub version: Option<String>,
    pub url: String,
    pub key: String,
    pub outcome: Outcome,
}

/// Complete effective facts, plus the identity and outcome of each contributing source.
#[derive(Clone, Debug)]
pub struct CompatOverride {
    pub facts: Facts,
    pub sources: [Provenance; 2],
}

#[derive(Clone)]
struct Source {
    name: &'static str,
    disabled: bool,
    version: Option<String>,
    url: String,
    data: Option<Value>,
}

impl Source {
    fn provenance(&self, key: &str, outcome: Outcome) -> Provenance {
        let bundled = svg_data::compat_sources()
            .iter()
            .find(|(name, _, _)| *name == self.name)
            .filter(|_| matches!(outcome, Outcome::Failed | Outcome::Disabled));
        Provenance {
            source: self.name,
            version: bundled
                .map(|(_, version, _)| (*version).to_owned())
                .or_else(|| self.version.clone()),
            url: bundled.map_or_else(|| self.url.clone(), |(_, _, url)| (*url).to_owned()),
            key: key.to_owned(),
            outcome,
        }
    }
}

#[derive(Clone)]
pub struct RuntimeCompat {
    pub elements: HashMap<String, CompatOverride>,
    /// Genuine global or common facts, never a merge of element-local facts.
    pub attributes: HashMap<String, CompatOverride>,
    pub attribute_contexts: HashMap<(String, String), CompatOverride>,
}

impl RuntimeCompat {
    pub fn disabled() -> Self {
        let source = |name| Source {
            name,
            disabled: true,
            version: None,
            url: String::new(),
            data: None,
        };
        build_runtime(&source("@mdn/browser-compat-data"), &source("web-features"))
    }

    pub fn attribute(&self, name: &str, element: Option<&str>) -> Option<&CompatOverride> {
        element
            .and_then(|el| {
                self.attribute_contexts
                    .get(&(el.to_owned(), name.to_owned()))
            })
            .or_else(|| self.attributes.get(name))
    }

    pub fn to_lint_overrides(&self) -> svg_lint::LintOverrides {
        let flags = |r: &CompatOverride| svg_lint::CompatFlags {
            deprecated: r.facts.deprecated,
            experimental: r.facts.experimental,
        };
        svg_lint::LintOverrides {
            elements: self
                .elements
                .iter()
                .filter(|(_, v)| {
                    !matches!(v.sources[0].outcome, Outcome::Failed | Outcome::Disabled)
                })
                .map(|(k, v)| (k.clone(), flags(v)))
                .collect(),
            attributes: self
                .attributes
                .iter()
                .filter(|(_, v)| {
                    !matches!(v.sources[0].outcome, Outcome::Failed | Outcome::Disabled)
                })
                .map(|(k, v)| (k.clone(), flags(v)))
                .collect(),
            attribute_contexts: self
                .attribute_contexts
                .iter()
                .filter(|(_, v)| {
                    !matches!(v.sources[0].outcome, Outcome::Failed | Outcome::Disabled)
                })
                .map(|(k, v)| (k.clone(), flags(v)))
                .collect(),
        }
    }
    pub fn to_verdict_overrides(&self) -> svg_lint::VerdictOverrides {
        // An explicit neutral verdict clears stale reasons. A missing map entry
        // alone means to use the catalog.
        let verdict = |r: &CompatOverride| {
            effective_compat::verdict(&r.facts).unwrap_or(svg_data::CompatVerdict {
                recommendation: svg_data::VerdictRecommendation::Safe,
                headline_template: "safe to use",
                reasons: Vec::new(),
            })
        };
        svg_lint::VerdictOverrides {
            elements: self
                .elements
                .iter()
                .map(|(k, v)| (k.clone(), verdict(v)))
                .collect(),
            attributes: self
                .attributes
                .iter()
                .map(|(k, v)| (k.clone(), verdict(v)))
                .collect(),
            attribute_contexts: self
                .attribute_contexts
                .iter()
                .map(|(k, v)| (k.clone(), verdict(v)))
                .collect(),
        }
    }
}

pub fn fetch_runtime_compat() -> RuntimeCompat {
    build_runtime(
        &fetch_source("@mdn/browser-compat-data", "/svg/elements"),
        &fetch_source("web-features", "/features"),
    )
}

fn fetch_source(name: &'static str, required: &str) -> Source {
    let package_url = format!("https://unpkg.com/{name}@latest/package.json");
    let version = fetch_json(&package_url).and_then(|v| v["version"].as_str().map(str::to_owned));
    let url = version.as_ref().map_or(package_url, |v| {
        format!("https://unpkg.com/{name}@{v}/data.json")
    });
    let data = version
        .as_ref()
        .and_then(|_| fetch_json(&url))
        .filter(|v| v.pointer(required).is_some_and(Value::is_object));
    Source {
        name,
        disabled: false,
        version,
        url,
        data,
    }
}
fn fetch_json(url: &str) -> Option<Value> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build(),
    );
    let text = agent
        .get(url)
        .call()
        .map_err(|err| tracing::warn!(url,error=%err,"compat fetch failed"))
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    serde_json::from_str(&text)
        .map_err(|err| tracing::warn!(url,error=%err,"invalid compat JSON"))
        .ok()
}

fn raw_attribute_name(name: &str) -> String {
    match name {
        "referrerpolicy" => "referrerPolicy".to_owned(),
        "data-*" => "data_attributes".to_owned(),
        _ => name.replace(':', "_"),
    }
}
fn global_key(name: &str) -> String {
    format!("svg.global_attributes.{}", raw_attribute_name(name))
}
fn attribute_key(element: &str, name: &str) -> String {
    format!("svg.elements.{element}.{}", raw_attribute_name(name))
}
fn bcd_record<'a>(data: &'a Value, key: &str) -> Option<&'a Value> {
    data.pointer(&format!("/{}/__compat", key.replace('.', "/")))
}
fn wf_has_key(features: Option<&Value>, key: &str) -> bool {
    features
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|f| f.values())
        .any(|f| {
            f.get("compat_features")
                .and_then(Value::as_array)
                .is_some_and(|keys| keys.iter().any(|v| v.as_str() == Some(key)))
        })
}
fn bcd_outcome(record: &Value) -> Outcome {
    let status = record.get("status");
    let support = record.get("support");
    if !record.is_object() || (status.is_none() && support.is_none()) {
        return Outcome::Unknown;
    }
    let invalid_status = status.is_some_and(|s| {
        !s.is_object()
            || ["deprecated", "experimental", "standard_track"]
                .iter()
                .any(|key| s.get(key).is_some_and(|v| !v.is_boolean()))
    });
    let unknown_support = support.is_some_and(|s| {
        !s.is_object()
            || s.as_object().into_iter().flat_map(|s| s.values()).any(|v| {
                let statements = v
                    .as_array()
                    .map_or_else(|| std::slice::from_ref(v), Vec::as_slice);
                statements.iter().any(|s| {
                    s.get("version_added")
                        .is_none_or(|v| !v.is_boolean() && !v.is_string())
                })
            })
    });
    if invalid_status || unknown_support {
        Outcome::Unknown
    } else {
        Outcome::Loaded
    }
}

fn resolve(
    baked: Facts,
    key: &str,
    fallback: Option<&str>,
    bcd: &Source,
    wf: &Source,
) -> CompatOverride {
    let mut facts = baked;
    let mut bcd_key = key;
    let bcd_outcome = if let Some(data) = &bcd.data {
        let record = bcd_record(data, key).or_else(|| {
            fallback.and_then(|k| {
                let value = bcd_record(data, k);
                if value.is_some() {
                    bcd_key = k;
                }
                value
            })
        });
        facts.deprecated = record
            .and_then(|r| r.pointer("/status/deprecated"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        facts.experimental = record
            .and_then(|r| r.pointer("/status/experimental"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        facts.standard_track = record
            .and_then(|r| r.pointer("/status/standard_track"))
            .and_then(Value::as_bool);
        facts.browser_support = record.and_then(svg_data::compat_parse::extract_browser_support);
        record.map_or(Outcome::Absent, bcd_outcome)
    } else if bcd.disabled {
        Outcome::Disabled
    } else {
        Outcome::Failed
    };
    let mut wf_key = key;
    let wf_outcome = if let Some(data) = &wf.data {
        let features = data.get("features");
        if !wf_has_key(features, key)
            && let Some(k) = fallback.filter(|k| wf_has_key(features, k))
        {
            wf_key = k;
        }
        facts.baseline = compat_model::resolve_baseline(features, wf_key);
        facts.discouraged = compat_model::resolve_discouraged(features, wf_key);
        if !wf_has_key(features, wf_key) {
            Outcome::Absent
        } else if facts.baseline.as_ref().is_none_or(|b| b.status.is_none()) {
            Outcome::Unknown
        } else {
            Outcome::Loaded
        }
    } else if wf.disabled {
        Outcome::Disabled
    } else {
        Outcome::Failed
    };
    CompatOverride {
        facts,
        sources: [
            bcd.provenance(bcd_key, bcd_outcome),
            wf.provenance(wf_key, wf_outcome),
        ],
    }
}
fn build_runtime(bcd: &Source, wf: &Source) -> RuntimeCompat {
    let elements = svg_data::elements()
        .iter()
        .map(|el| {
            let baked = svg_data::CompatFacts {
                deprecated: el.deprecated,
                experimental: el.experimental,
                standard_track: el.standard_track,
                baseline: el.baseline,
                discouraged: el.discouraged,
                browser_support: el.browser_support,
            };
            (
                el.name.to_owned(),
                resolve(
                    baked.into(),
                    &format!("svg.elements.{}", el.name),
                    None,
                    bcd,
                    wf,
                ),
            )
        })
        .collect();
    let contexts = attribute_contexts(bcd, wf);
    let attributes = svg_data::attributes()
        .iter()
        .map(|a| {
            (
                a.name.to_owned(),
                resolve(a.compat_facts().into(), &global_key(a.name), None, bcd, wf),
            )
        })
        .collect();
    let attribute_contexts = contexts
        .into_iter()
        .filter_map(|(el, name)| {
            let a = svg_data::attribute(&name)?;
            let record = resolve(
                a.compat_facts_for_element(Some(&el)).into(),
                &attribute_key(&el, &name),
                Some(&global_key(&name)),
                bcd,
                wf,
            );
            Some(((el, name), record))
        })
        .collect();
    RuntimeCompat {
        elements,
        attributes,
        attribute_contexts,
    }
}

fn attribute_contexts(bcd: &Source, wf: &Source) -> HashSet<(String, String)> {
    let mut contexts: HashSet<(String, String)> = svg_data::attributes()
        .iter()
        .flat_map(|a| {
            a.element_compat
                .iter()
                .map(move |c| (c.element.to_owned(), a.name.to_owned()))
        })
        .collect();
    if let Some(elements) = bcd
        .data
        .as_ref()
        .and_then(|d| d.pointer("/svg/elements"))
        .and_then(Value::as_object)
    {
        for (element, attrs) in elements {
            for (raw, attr) in attrs.as_object().into_iter().flatten() {
                if let Some(name) = attr
                    .get("__compat")
                    .and_then(|c| bcd_attribute_name(raw, c))
                {
                    contexts.insert((element.clone(), name));
                }
            }
        }
    }
    if let Some(features) = wf
        .data
        .as_ref()
        .and_then(|d| d.get("features"))
        .and_then(Value::as_object)
    {
        for feature in features.values() {
            for key in feature
                .get("compat_features")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let parts: Vec<_> = key.split('.').collect();
                if let ["svg", "elements", element, raw] = parts.as_slice() {
                    let canonical = match *raw {
                        "referrerPolicy" => "referrerpolicy".to_owned(),
                        "data_attributes" => "data-*".to_owned(),
                        _ => raw.replace('_', ":"),
                    };
                    if svg_data::attribute(&canonical).is_some() {
                        contexts.insert(((*element).to_owned(), canonical));
                    }
                }
            }
        }
    }
    contexts
}

fn bcd_attribute_name(attribute_name: &str, _compat: &Value) -> Option<String> {
    let canonical = match attribute_name {
        "data_attributes" => "data-*".to_owned(),
        "referrerPolicy" => "referrerpolicy".to_owned(),
        name if name.starts_with("xlink_") || name.starts_with("xml_") => name.replace('_', ":"),
        other => svg_data::xlink::canonical_svg_attribute_name(other).into_owned(),
    };
    svg_data::attribute(&canonical).map(|_| canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn source(name: &'static str, data: Option<Value>) -> Source {
        Source {
            name,
            disabled: false,
            version: Some("fixture-2".to_owned()),
            url: "https://example.com/fixture-2".to_owned(),
            data,
        }
    }
    fn runtime(bcd: Option<Value>, wf: Option<Value>) -> RuntimeCompat {
        build_runtime(
            &source("@mdn/browser-compat-data", bcd),
            &source("web-features", wf),
        )
    }
    fn context_bcd() -> Value {
        json!({"svg":{"elements":{
            "rect":{"width":{"__compat":{"mdn_url":"https://developer.mozilla.org/docs/Web/SVG/Attribute/width","status":{"deprecated":true},"support":{"chrome":{"version_added":false}}}}},
            "svg":{"width":{"__compat":{"mdn_url":"https://developer.mozilla.org/docs/Web/SVG/Attribute/width","status":{"deprecated":false},"support":{"chrome":{"version_added":"120","notes":"fresh note","flags":[{"name":"new-flag"}],"version_removed":"130"}}}}}
        },"global_attributes":{"width":{"__compat":{"status":{"experimental":true},"support":{"chrome":{"version_added":true}}}}}}})
    }
    fn context_wf() -> Value {
        json!({"features":{
            "scoped":{"name":"Scoped width","compat_features":["svg.elements.rect.width"],"status":{"baseline":"high","by_compat_key":{"svg.elements.rect.width":{"baseline":false}}},"discouraged":{"reason":"Rect advice only","according_to":[],"alternatives":[]}},
            "ordinary":{"compat_features":["svg.elements.svg.width"],"status":{"baseline":"high"}},
            "global":{"compat_features":["svg.global_attributes.width"],"status":{"baseline":"low"}}
        }})
    }
    fn hover(record: &CompatOverride, element: &str) -> Result<String, Box<dyn std::error::Error>> {
        Ok(crate::hover::format_attribute_hover_with_profile_name(
            svg_data::attribute("width").ok_or("width")?,
            "width",
            Some(element),
            svg_data::SpecSnapshotId::LATEST,
            None,
            Some(record),
            None,
        ))
    }
    #[test]
    fn exact_context_controls_hover_lint_and_completion() -> TestResult {
        let runtime = runtime(Some(context_bcd()), Some(context_wf()));
        let rect = runtime
            .attribute("width", Some("rect"))
            .ok_or("rect width")?;
        let svg = runtime.attribute("width", Some("svg")).ok_or("svg width")?;
        assert_eq!(
            rect.facts.baseline.as_ref().and_then(|b| b.status),
            Some(svg_data::BaselineTier::Limited)
        );
        assert_eq!(
            svg.facts.baseline.as_ref().and_then(|b| b.status),
            Some(svg_data::BaselineTier::Widely)
        );
        assert_eq!(rect.sources[0].key, "svg.elements.rect.width");
        assert_eq!(rect.facts.discouraged.len(), 1);
        assert_eq!(
            svg.facts.discouraged,
            Vec::<compat_model::Discouraged>::new()
        );
        let rect_hover = hover(rect, "rect")?;
        let svg_hover = hover(svg, "svg")?;
        assert!(rect_hover.contains("Rect advice only"));
        assert!(!svg_hover.contains("Rect advice only"));
        assert!(svg_hover.contains("fresh note"));
        assert!(svg_hover.contains("new-flag"));
        assert!(svg_hover.contains("removed in chrome 130"));
        assert!(!svg_hover.contains("limited baseline"));
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
        let source = b"<svg width=\"1\"><rect width=\"1\"/></svg>";
        let tree = parser.parse(source, None).ok_or("parse")?;
        let flags = runtime.to_lint_overrides();
        let verdicts = runtime.to_verdict_overrides();
        let diagnostics = svg_lint::lint_tree_with_compat(
            source,
            &tree,
            svg_lint::LintOptions::default(),
            Some(&flags),
            Some(&verdicts),
        );
        let deprecated: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == svg_lint::DiagnosticCode::DeprecatedAttribute)
            .collect();
        assert_eq!(deprecated.len(), 1, "{diagnostics:?}");
        assert!(
            deprecated[0].start_col > 20,
            "only rect.width should be deprecated"
        );
        for (owner, expected) in [("rect", Some(true)), ("svg", None)] {
            let mut items = vec![tower_lsp_server::ls_types::CompletionItem {
                label: "width".to_owned(),
                ..Default::default()
            }];
            crate::completion::reconcile_compat_items(
                &mut items,
                Some(owner),
                svg_data::SpecSnapshotId::LATEST,
                Some(&runtime),
            );
            assert_eq!(items[0].deprecated, expected);
            let Some(tower_lsp_server::ls_types::Documentation::MarkupContent(doc)) =
                &items[0].documentation
            else {
                return Err("completion documentation".into());
            };
            assert_eq!(doc.value.contains("Rect advice only"), owner == "rect");
        }
        Ok(())
    }
    #[test]
    fn exact_unknown_overrides_global_but_absent_context_uses_global() -> TestResult {
        let mut wf = context_wf();
        wf["features"]["scoped"]["status"]["by_compat_key"]["svg.elements.rect.width"] = json!({});
        let r = runtime(Some(context_bcd()), Some(wf));
        let exact = r.attribute("width", Some("rect")).ok_or("exact")?;
        assert!(
            exact
                .facts
                .baseline
                .as_ref()
                .is_none_or(|b| b.status.is_none())
        );
        assert_eq!(exact.sources[1].outcome, Outcome::Unknown);
        let fallback = r.attribute("width", Some("unlisted")).ok_or("global")?;
        assert_eq!(
            fallback.facts.baseline.as_ref().and_then(|b| b.status),
            Some(svg_data::BaselineTier::Newly)
        );
        assert_eq!(fallback.sources[1].key, "svg.global_attributes.width");
        assert!(fallback.facts.experimental);
        Ok(())
    }
    #[test]
    fn authoritative_absence_clears_baked_facts_and_failed_refresh_retains_them() -> TestResult {
        let attribute = svg_data::attribute("width").ok_or("width")?;
        let baked = Facts::from(attribute.compat_facts_for_element(Some("rect")));
        assert!(baked.baseline.is_some());
        let absent = runtime(
            Some(json!({"svg":{"elements":{},"global_attributes":{}}})),
            Some(json!({"features":{}})),
        );
        let record = absent.attribute("width", Some("rect")).ok_or("absent")?;
        assert!(record.facts.baseline.is_none());
        assert!(record.facts.browser_support.is_none());
        assert_eq!(
            record.facts.discouraged,
            Vec::<compat_model::Discouraged>::new()
        );
        assert_eq!(record.sources[0].outcome, Outcome::Absent);
        assert_eq!(record.sources[1].outcome, Outcome::Absent);
        let failed = runtime(None, None);
        let record = failed.attribute("width", Some("rect")).ok_or("failed")?;
        assert_eq!(record.facts, baked);
        assert_eq!(record.sources[1].outcome, Outcome::Failed);
        assert_eq!(record.sources[1].version.as_deref(), Some("3.36.0"));
        assert!(hover(record, "rect")?.contains("bundled facts retained (stale)"));
        Ok(())
    }
    #[test]
    fn partial_refresh_keeps_source_identity_and_complete_browser_changes() -> TestResult {
        let r = runtime(Some(context_bcd()), None);
        let record = r.attribute("width", Some("svg")).ok_or("width")?;
        let baked = Facts::from(
            svg_data::attribute("width")
                .ok_or("width")?
                .compat_facts_for_element(Some("svg")),
        );
        assert_eq!(record.facts.baseline, baked.baseline);
        assert_eq!(record.sources[0].version.as_deref(), Some("fixture-2"));
        assert_eq!(record.sources[1].version.as_deref(), Some("3.36.0"));
        assert_eq!(record.sources[0].outcome, Outcome::Loaded);
        assert_eq!(record.sources[1].outcome, Outcome::Failed);
        let browser = record
            .facts
            .browser_support
            .as_ref()
            .and_then(|s| s.chrome.as_ref())
            .ok_or("chrome")?;
        assert_eq!(browser.notes, ["fresh note"]);
        assert_eq!(browser.version_removed.as_deref(), Some("130"));
        let verdict = effective_compat::verdict(&record.facts).ok_or("verdict")?;
        assert!(
            verdict
                .reasons
                .contains(&svg_data::VerdictReason::BehindFlagIn("chrome"))
        );
        assert!(verdict.reasons.iter().any(
            |r| matches!(r,svg_data::VerdictReason::RemovedIn {version,..} if version=="130")
        ));
        Ok(())
    }
    #[test]
    fn baseline_changes_replace_baked_reasons_in_both_directions() -> TestResult {
        let bcd = source(
            "@mdn/browser-compat-data",
            Some(
                json!({"svg":{"elements":{"rect":{"__compat":{"status":{"standard_track":false}}}}}}),
            ),
        );
        for (before, after, limited) in [(false, json!("high"), false), (true, json!(false), true)]
        {
            let baked = Facts {
                baseline: compat_model::parse_baseline(
                    &json!({"baseline":if before {json!("high")}else{json!(false)}}),
                ),
                ..Default::default()
            };
            let wf = source(
                "web-features",
                Some(
                    json!({"features":{"svg":{"compat_features":["svg.elements.rect"],"status":{"baseline":after}}}}),
                ),
            );
            let record = resolve(baked, "svg.elements.rect", None, &bcd, &wf);
            assert_eq!(
                effective_compat::verdict(&record.facts).is_some_and(|v| v
                    .reasons
                    .contains(&svg_data::VerdictReason::BaselineLimited)),
                limited
            );
            assert!(
                effective_compat::verdict(&record.facts).is_some_and(|v| v.recommendation
                    == svg_data::VerdictRecommendation::Avoid
                    && v.reasons.contains(&svg_data::VerdictReason::BcdNonStandard))
            );
            let element = svg_data::element("rect").ok_or("rect")?;
            let hover = crate::hover::format_element_hover_with_profile(
                element,
                svg_data::SpecSnapshotId::LATEST,
                None,
                Some(&record),
                None,
            );
            assert_eq!(hover.contains("limited baseline"), limited);
            assert_eq!(hover.contains("Widely Available"), !limited);
        }
        Ok(())
    }
    #[test]
    fn disabled_and_failed_refreshes_use_baked_common_facts_and_lint_policy() -> TestResult {
        for runtime in [RuntimeCompat::disabled(), runtime(None, None)] {
            let a = svg_data::attribute("id").ok_or("id")?;
            assert_eq!(
                runtime.attribute("id", Some("rect")).ok_or("id")?.facts,
                Facts::from(a.compat_facts_for_element(Some("rect")))
            );
            let flags = runtime.to_lint_overrides();
            assert_eq!(flags.elements.len(), 0);
            assert_eq!(flags.attributes.len(), 0);
            assert_eq!(flags.attribute_contexts.len(), 0);
            let source = br#"<svg version="1.1" baseProfile="full"/>"#;
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
            let tree = parser.parse(source, None).ok_or("parse")?;
            let options = svg_lint::LintOptions {
                profile: svg_data::SpecSnapshotId::Svg11Rec20110816,
                ..Default::default()
            };
            let baked = svg_lint::lint_tree_with_compat(source, &tree, options, None, None);
            let refreshed = svg_lint::lint_tree_with_compat(
                source,
                &tree,
                options,
                Some(&flags),
                Some(&runtime.to_verdict_overrides()),
            );
            assert_eq!(format!("{baked:?}"), format!("{refreshed:?}"));
        }
        Ok(())
    }
    #[test]
    fn malformed_bcd_fields_are_unknown_and_do_not_keep_old_warnings() -> TestResult {
        let bcd = source(
            "@mdn/browser-compat-data",
            Some(
                json!({"svg":{"elements":{"rect":{"__compat":{"status":{"deprecated":"maybe"},"support":{"chrome":{"version_added":null}}}}}}}),
            ),
        );
        let wf = source("web-features", Some(json!({"features":{}})));
        let record = resolve(
            Facts {
                deprecated: true,
                ..Default::default()
            },
            "svg.elements.rect",
            None,
            &bcd,
            &wf,
        );
        assert_eq!(record.sources[0].outcome, Outcome::Unknown);
        assert!(!record.facts.deprecated);
        assert_eq!(
            record
                .facts
                .browser_support
                .as_ref()
                .and_then(|s| s.chrome.as_ref())
                .ok_or("chrome")?
                .supported,
            None
        );
        Ok(())
    }

    #[test]
    fn exact_context_does_not_require_a_documentation_url() -> TestResult {
        let r = runtime(
            Some(
                json!({"svg":{"elements":{"circle":{"width":{"__compat":{"status":{"deprecated":true}}}}},"global_attributes":{"width":{"__compat":{"status":{"deprecated":false}}}}}}),
            ),
            Some(json!({"features":{}})),
        );
        assert!(
            r.attribute("width", Some("circle"))
                .ok_or("circle width")?
                .facts
                .deprecated
        );
        assert!(
            !r.attribute("width", Some("unlisted"))
                .ok_or("global width")?
                .facts
                .deprecated
        );
        Ok(())
    }
    #[test]
    fn browser_refresh_removes_old_notes_flags_prefixes_and_removal_reasons() -> TestResult {
        use svg_data::effective_compat::{BrowserFlag, BrowserSupport, BrowserVersion};
        let old = Facts {
            browser_support: Some(BrowserSupport {
                chrome: Some(BrowserVersion {
                    supported: Some(true),
                    version_added: Some("40".to_owned()),
                    version_removed: Some("60".to_owned()),
                    prefix: Some("-old-".to_owned()),
                    notes: vec!["obsolete note".to_owned()],
                    flags: vec![BrowserFlag {
                        name: "old-flag".to_owned(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let bcd = source(
            "@mdn/browser-compat-data",
            Some(
                json!({"svg":{"elements":{"rect":{"width":{"__compat":{"support":{"chrome":{"version_added":"130","notes":"replacement note"}}}}}}}}),
            ),
        );
        let record = resolve(
            old,
            "svg.elements.rect.width",
            None,
            &bcd,
            &source("web-features", Some(json!({"features":{}}))),
        );
        let output = hover(&record, "rect")?;
        assert!(output.contains("Chrome 130"));
        assert!(output.contains("replacement note"));
        for stale in [
            "old-flag",
            "obsolete note",
            "-old-",
            "removed in chrome",
            "flagged in chrome",
        ] {
            assert!(!output.contains(stale), "{output}");
        }
        assert!(effective_compat::verdict(&record.facts).is_none());
        Ok(())
    }
}
