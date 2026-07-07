//! WAI-ARIA state/property value extraction.
//!
//! SVG 2's `aria` attribute category delegates every `aria-*` attribute to
//! WAI-ARIA: each one carries an explicit
//! `https://www.w3.org/TR/wai-aria-1.1/#aria-NAME` href and no local value
//! grammar. WAI-ARIA defines each attribute's value type in a characteristics
//! table (the `Value:` row) and, for `token` types, an enumerated value table.
//! Follow that source and project the ARIA value types into catalog value
//! spaces so `aria-*` attributes stop falling back to unresolved.
//!
//! # Sources parsed
//!
//! - [WAI-ARIA 1.1] — each `#aria-NAME` section's `table.property-features`
//!   (properties) or `table.state-features` (states) gives the `Value:` type;
//!   the trailing `table.value-descriptions` gives the token enumerations.
//!
//! [WAI-ARIA 1.1]: https://www.w3.org/TR/wai-aria-1.1/

use std::collections::BTreeMap;

use tl::ParserOptions;

use crate::{catalog::CatalogAttributeValues, fetch, util::normalize_ws};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

const WAI_ARIA_URL: &str = "https://www.w3.org/TR/wai-aria-1.1/";

/// Derived value spaces for `aria-*` attributes, keyed by attribute name.
pub struct AriaValueIndex {
    values: BTreeMap<String, CatalogAttributeValues>,
}

impl AriaValueIndex {
    /// The derived value space for an `aria-*` attribute, if WAI-ARIA defined a
    /// value type we model.
    pub fn get(&self, name: &str) -> Option<&CatalogAttributeValues> {
        self.values.get(name)
    }

    /// Number of `aria-*` attributes with a derived value space.
    pub fn len(&self) -> usize {
        self.values.len()
    }
}

/// Fetch WAI-ARIA and derive the value space of every `aria-*` attribute.
///
/// # Errors
/// Returns an error if the WAI-ARIA page cannot be fetched or parsed.
pub fn fetch_aria_value_index() -> Fallible<AriaValueIndex> {
    let html = fetch::url_text(WAI_ARIA_URL, "text/html")?;
    Ok(AriaValueIndex {
        values: parse_aria_value_index(&html)?,
    })
}

fn parse_aria_value_index(html: &str) -> Fallible<BTreeMap<String, CatalogAttributeValues>> {
    let mut values = BTreeMap::new();
    for (name, segment) in aria_sections(html) {
        if let Some(value_space) = section_value_space(segment)? {
            values.insert(name, value_space);
        }
    }
    Ok(values)
}

/// Split the page into `(aria-name, section-html)` slices. Each state/property
/// section opens with `id="aria-NAME" ... property="bibo:hasPart"`; the slice
/// runs to the next such section (or end of document).
fn aria_sections(html: &str) -> Vec<(String, &str)> {
    let marker = "property=\"bibo:hasPart\"";
    let mut starts = Vec::new();
    for (offset, _) in html.match_indices("id=\"aria-") {
        let after = &html[offset + 4..];
        let Some(close) = after.find('"') else {
            continue;
        };
        let name = &after[..close];
        if !name
            .strip_prefix("aria-")
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_lowercase()))
        {
            continue;
        }
        // Only the top-level property section carries `bibo:hasPart`; the tag's
        // remaining attributes hold it before the tag closes.
        let Some(tag_end) = after[close..].find('>') else {
            continue;
        };
        if !after[close..close + tag_end].contains(marker) {
            continue;
        }
        starts.push((offset, name.to_owned()));
    }
    let mut sections = Vec::with_capacity(starts.len());
    for index in 0..starts.len() {
        let (start, ref name) = starts[index];
        let end = starts.get(index + 1).map_or(html.len(), |next| next.0);
        sections.push((name.clone(), &html[start..end]));
    }
    sections
}

/// Read one section's `Value:` characteristic (and, for `token` types, its
/// enumerated values) and project it to a catalog value space.
fn section_value_space(segment: &str) -> Fallible<Option<CatalogAttributeValues>> {
    let dom = tl::parse(segment, ParserOptions::default())?;
    let parser = dom.parser();

    // WAI-ARIA formats *properties* and *states* identically but under distinct
    // class names (`property-features` / `state-features`), so read either.
    let features = dom
        .query_selector("table.property-features")
        .and_then(|mut tables| tables.next())
        .or_else(|| {
            dom.query_selector("table.state-features")
                .and_then(|mut tables| tables.next())
        })
        .and_then(|handle| handle.get(parser))
        .and_then(|node| node.as_tag());
    let mut value_type = None;
    if let Some(rows) = features.and_then(|table| table.query_selector(parser, "tr")) {
        for handle in rows {
            let Some(row) = handle.get(parser).and_then(|node| node.as_tag()) else {
                continue;
            };
            let label = first_text(row, parser, "th");
            if label
                .as_deref()
                .map(|text| text.trim_end_matches(':').trim())
                == Some("Value")
            {
                value_type = first_text(row, parser, "td");
                break;
            }
        }
    }
    let Some(value_type) = value_type else {
        return Ok(None);
    };

    // Enumerated token names live in `value-name` (properties) or `state-name`
    // (states) header cells of the trailing value-description table.
    let mut tokens: Vec<String> = query_token_cells(&dom, parser, "th.value-name");
    if tokens.is_empty() {
        tokens = query_token_cells(&dom, parser, "th.state-name");
    }

    Ok(map_value_type(&value_type, &tokens))
}

fn query_token_cells(dom: &tl::VDom, parser: &tl::Parser, selector: &str) -> Vec<String> {
    dom.query_selector(selector)
        .map(|handles| {
            handles
                .filter_map(|handle| handle.get(parser).and_then(|node| node.as_tag()))
                .map(|tag| aria_token(&normalize_ws(&tag.inner_text(parser))))
                .filter(|token| !token.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Strip the `(default)` marker WAI-ARIA appends to a default token.
fn aria_token(cell: &str) -> String {
    cell.split_once(" (default)")
        .map_or(cell, |(token, _)| token)
        .trim()
        .to_owned()
}

/// Project a WAI-ARIA value type to a catalog value space. Types are defined in
/// WAI-ARIA §6.3; `string` is genuinely free-form, so it maps to
/// [`CatalogAttributeValues::FreeText`] rather than being left unresolved.
fn map_value_type(value_type: &str, tokens: &[String]) -> Option<CatalogAttributeValues> {
    let enum_values = |values: &[&str]| CatalogAttributeValues::Enum {
        values: values.iter().map(|value| (*value).to_owned()).collect(),
    };
    match value_type.trim().to_ascii_lowercase().as_str() {
        "true/false" => Some(CatalogAttributeValues::Boolean),
        "true/false/undefined" => Some(enum_values(&["false", "true", "undefined"])),
        "tristate" => Some(enum_values(&["false", "mixed", "true", "undefined"])),
        "integer" => Some(CatalogAttributeValues::Integer),
        "number" => Some(CatalogAttributeValues::Number),
        "id reference" => Some(CatalogAttributeValues::Id),
        "id reference list" => Some(CatalogAttributeValues::IdList),
        "string" => Some(CatalogAttributeValues::FreeText),
        "token list" => Some(CatalogAttributeValues::TokenList),
        "token" => (!tokens.is_empty()).then(|| CatalogAttributeValues::Enum {
            values: tokens.to_vec(),
        }),
        _ => None,
    }
}

fn first_text(tag: &tl::HTMLTag, parser: &tl::Parser, selector: &str) -> Option<String> {
    let handle = tag.query_selector(parser, selector)?.next()?;
    let found = handle.get(parser)?.as_tag()?;
    Some(normalize_ws(&found.inner_text(parser)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECTION: &str = r#"
<section id="aria-autocomplete" property="bibo:hasPart">
  <table class="property-features"><caption>Characteristics:</caption>
    <tbody>
      <tr><th class="property-applicability-head" scope="row">Used in Roles:</th><td>combobox</td></tr>
      <tr><th class="property-value-head" scope="row">Value:</th><td class="property-value">token</td></tr>
    </tbody>
  </table>
  <table class="value-descriptions"><caption>Values:</caption>
    <tbody>
      <tr><th class="value-name" scope="row">inline</th><td>x</td></tr>
      <tr><th class="value-name" scope="row">list</th><td>x</td></tr>
      <tr><th class="value-name" scope="row">none (default)</th><td>x</td></tr>
    </tbody>
  </table>
</section>
<section id="aria-checked" property="bibo:hasPart">
  <table class="property-features"><tbody>
    <tr><th scope="row">Value:</th><td>tristate</td></tr>
  </tbody></table>
</section>"#;

    #[test]
    fn extracts_token_enum_and_tristate() {
        let index = parse_aria_value_index(SECTION).expect("parse");
        assert_eq!(
            index.get("aria-autocomplete"),
            Some(&CatalogAttributeValues::Enum {
                values: vec!["inline".to_owned(), "list".to_owned(), "none".to_owned()],
            })
        );
        assert_eq!(
            index.get("aria-checked"),
            Some(&CatalogAttributeValues::Enum {
                values: vec![
                    "false".to_owned(),
                    "mixed".to_owned(),
                    "true".to_owned(),
                    "undefined".to_owned(),
                ],
            })
        );
    }

    #[test]
    fn maps_value_types_to_spaces() {
        assert_eq!(
            map_value_type("true/false", &[]),
            Some(CatalogAttributeValues::Boolean)
        );
        assert_eq!(
            map_value_type("ID reference list", &[]),
            Some(CatalogAttributeValues::IdList)
        );
        assert_eq!(
            map_value_type("number", &[]),
            Some(CatalogAttributeValues::Number)
        );
        assert_eq!(
            map_value_type("string", &[]),
            Some(CatalogAttributeValues::FreeText)
        );
    }
}
