//! Legacy SVG snapshot extraction.
//!
//! The main SVGWG fetch gives the current SVG 2 editor's draft. Snapshot-only
//! value differences still need authoritative dated sources; this module keeps
//! those fetches explicit and turns them into the same catalog override model.

use std::collections::BTreeMap;

use tl::{HTMLTag, Parser, ParserOptions};

use crate::catalog::{
    CatalogAttributeValueOverride, CatalogAttributeValues, CatalogLegacySource,
    CatalogSpecSnapshotId,
};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// One dated property-index source for a legacy SVG profile.
pub struct LegacyPropertyIndexSource {
    /// Human-readable source label.
    pub name: &'static str,
    /// Profile represented by this property index.
    pub profile: CatalogSpecSnapshotId,
    /// Exact W3C source URL.
    pub url: &'static str,
}

/// SVG 1.1 property indexes used for profile-specific value overrides.
pub const SVG11_PROPERTY_INDEXES: &[LegacyPropertyIndexSource] = &[
    LegacyPropertyIndexSource {
        name: "SVG 1.1 First Edition Property Index",
        profile: CatalogSpecSnapshotId::Svg11Rec20030114,
        url: "https://www.w3.org/TR/2003/REC-SVG11-20030114/propidx.html",
    },
    LegacyPropertyIndexSource {
        name: "SVG 1.1 Second Edition Property Index",
        profile: CatalogSpecSnapshotId::Svg11Rec20110816,
        url: "https://www.w3.org/TR/SVG11/propidx.html",
    },
];

/// Legacy extraction output to thread into catalog generation.
#[derive(Default)]
pub struct LegacyValueOverrides {
    /// Source records to persist in `catalog.json`.
    pub sources: Vec<CatalogLegacySource>,
    /// Attribute value overrides keyed by attribute/property name.
    pub attributes: BTreeMap<String, Vec<CatalogAttributeValueOverride>>,
}

/// Extract value overrides from one SVG 1.1 property index HTML page.
///
/// # Errors
/// Returns an error if the HTML cannot be parsed, or if no keyword-only rows
/// are found.
pub fn extract_svg11_property_index(
    source: &LegacyPropertyIndexSource,
    html: &str,
) -> Fallible<LegacyValueOverrides> {
    let dom = tl::parse(html, ParserOptions::default())?;
    let parser = dom.parser();
    let value_overrides = extract_keyword_only_value_overrides(&dom, parser);
    if value_overrides.is_empty() {
        return Err(boxed("SVG 1.1 property index had no keyword-only rows"));
    }

    let mut attributes = BTreeMap::new();
    for (name, values) in value_overrides {
        attributes
            .entry(name)
            .or_insert_with(Vec::new)
            .push(CatalogAttributeValueOverride {
                profile: source.profile,
                values: CatalogAttributeValues::Enum { values },
            });
    }

    Ok(LegacyValueOverrides {
        sources: vec![CatalogLegacySource {
            name: source.name.to_owned(),
            profile: source.profile,
            url: source.url.to_owned(),
        }],
        attributes,
    })
}

/// Merge one legacy extraction into an accumulator.
pub fn merge_value_overrides(target: &mut LegacyValueOverrides, mut source: LegacyValueOverrides) {
    target.sources.append(&mut source.sources);
    for (attribute, mut overrides) in source.attributes {
        target
            .attributes
            .entry(attribute)
            .or_default()
            .append(&mut overrides);
    }
}

fn extract_keyword_only_value_overrides(
    dom: &tl::VDom<'_>,
    parser: &Parser,
) -> BTreeMap<String, Vec<String>> {
    let mut overrides = BTreeMap::new();
    for node in dom.nodes() {
        let Some(row) = node.as_tag() else {
            continue;
        };
        if row.name().as_utf8_str() != "tr" {
            continue;
        }
        let Some((name, values)) = property_row_name_and_values(row, parser) else {
            continue;
        };
        let name = normalized_property_name(&name);
        if name.is_empty() {
            continue;
        }
        let Some(values) = keyword_only_values(&values) else {
            continue;
        };
        overrides.insert(name, values);
    }
    overrides
}

fn property_row_name_and_values(row: &HTMLTag, parser: &Parser) -> Option<(String, String)> {
    let mut cells = row.query_selector(parser, "td")?;
    let name = cell_text(cells.next()?, parser)?;
    let values = cell_text(cells.next()?, parser)?;
    Some((name, values))
}

fn cell_text(handle: tl::NodeHandle, parser: &Parser) -> Option<String> {
    let tag = handle.get(parser)?.as_tag()?;
    Some(normalize_ws(&tag.inner_text(parser)))
}

fn normalized_property_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect()
}

fn keyword_only_values(value: &str) -> Option<Vec<String>> {
    let tokens: Vec<&str> = value.split('|').map(str::trim).collect();
    if tokens.is_empty() || !tokens.iter().all(|token| is_keyword_token(token)) {
        return None;
    }
    let mut values: Vec<String> = tokens.into_iter().map(str::to_owned).collect();
    values.sort();
    values.dedup();
    Some(values)
}

fn is_keyword_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn boxed(message: &str) -> Box<dyn std::error::Error> {
    Box::<dyn std::error::Error>::from(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_keyword_only_svg11_property_values() -> Fallible<()> {
        let html = r#"
            <table class="property-table">
              <tr>
                <td><a href="painting.html#DisplayProperty"><span>'display'</span></a></td>
                <td>inline | block | list-item | run-in | compact |
                    marker | table | inline-table | table-row-group |
                    table-header-group | table-footer-group | table-row |
                    table-column-group | table-column | table-cell |
                    table-caption | none | <a><span>inherit</span></a></td>
                    <td>inline</td>
              </tr>
              <tr>
                <td><a href="painting.html#StrokeLinecapProperty"><span>'stroke-linecap'</span></a></td>
                <td>butt | round | square | inherit</td>
                <td>butt</td>
              </tr>
              <tr>
                <td><a href="painting.html#StrokeDasharrayProperty"><span>'stroke-dasharray'</span></a></td>
                <td>none | &lt;dasharray&gt;</td>
                <td>none</td>
              </tr>
            </table>
        "#;

        let overrides = extract_svg11_property_index(&SVG11_PROPERTY_INDEXES[0], html)?;
        let display = overrides
            .attributes
            .get("display")
            .and_then(|overrides| overrides.first())
            .ok_or("missing display override")?;

        assert_eq!(display.profile, CatalogSpecSnapshotId::Svg11Rec20030114);
        assert_eq!(
            display.values,
            CatalogAttributeValues::Enum {
                values: [
                    "block",
                    "compact",
                    "inherit",
                    "inline",
                    "inline-table",
                    "list-item",
                    "marker",
                    "none",
                    "run-in",
                    "table",
                    "table-caption",
                    "table-cell",
                    "table-column",
                    "table-column-group",
                    "table-footer-group",
                    "table-header-group",
                    "table-row",
                    "table-row-group",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            }
        );
        assert!(
            overrides.attributes.contains_key("stroke-linecap"),
            "keyword-only rows should become overrides"
        );
        assert!(
            !overrides.attributes.contains_key("stroke-dasharray"),
            "mixed grammars must not collapse to keyword-only enums"
        );
        Ok(())
    }
}
