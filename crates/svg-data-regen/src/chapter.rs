//! Extract permalink anchors, term definitions, and example references from a
//! chapter or appendix HTML page.
//!
//! Chapter source HTML carries the prose, the `id` anchors that element and
//! attribute hrefs point at, the `<dfn>` term definitions, and `<edit:example>`
//! references. (The rendered element-summary tables are injected at publish
//! time from `definitions.xml`, so the structural content model is extracted
//! from there, not here.) This module turns one page into a typed record.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::Serialize;
use tl::{HTMLTag, Parser, ParserOptions};

use crate::util::is_keyword_token;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// An `id` anchor: a permalink target within a chapter.
#[derive(Debug, Clone, Serialize)]
pub struct Anchor {
    /// The fragment id (the part after `#` in a permalink).
    pub id: String,
    /// The HTML tag carrying the id (`h2`, `dfn`, `dt`, ...).
    pub tag: String,
    /// The heading text, captured for section headings (`h1`..`h6`).
    pub text: Option<String>,
}

/// A `<dfn>` term definition.
#[derive(Debug, Clone, Serialize)]
pub struct Dfn {
    /// The definition's anchor id, when it has one.
    pub id: Option<String>,
    /// The defined term's text.
    pub term: String,
    /// The `data-dfn-type` (e.g. `dfn`, `element`, `attribute`), when set.
    pub kind: Option<String>,
}

/// An `<edit:example>` reference to an example asset.
#[derive(Debug, Clone, Serialize)]
pub struct Example {
    /// The referenced example file (`href`).
    pub href: Option<String>,
    /// The `image` flag (`yes`/`no`), when set.
    pub image: Option<String>,
    /// The `link` flag (`yes`/`no`), when set.
    pub link: Option<String>,
}

/// A property definition table (`<table class="propdef">`): the value space and
/// metadata for a single CSS-style property or presentation attribute.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyValueDef {
    /// Property name (the `Name:` row).
    pub name: String,
    /// The property's definition anchor id, when its `<dfn>` has one.
    pub id: Option<String>,
    /// The raw value grammar (the `Value:` row), e.g. `start | middle | end`.
    pub value: Option<String>,
    /// The bare keyword alternatives parsed out of the value grammar (the enum
    /// members); `<type>` references and bracketed groups are excluded.
    pub keywords: Vec<String>,
    /// The initial value (`Initial:` row).
    pub initial: Option<String>,
    /// Which elements the property applies to (`Applies to:` row).
    pub applies_to: Option<String>,
    /// Whether the property is inherited (`Inherited:` row).
    pub inherited: Option<String>,
    /// The computed value description (`Computed value:` row).
    pub computed_value: Option<String>,
    /// The animation type (`Animation type:` row).
    pub animation_type: Option<String>,
}

/// A term defined in a `<dl class="definitions">` list, paired with the prose
/// description from its `<dd>`. This is the spec's own glossary: the reliable,
/// structured source of per-entity descriptions.
#[derive(Debug, Clone, Serialize)]
pub struct TermDefinition {
    /// The defined term (the `<dt>`'s `<dfn>` text).
    pub term: String,
    /// The definition's anchor id, when its `<dfn>` has one.
    pub id: Option<String>,
    /// The `data-dfn-type` (`dfn`, `element`, `attribute`, ...), when set.
    pub kind: Option<String>,
    /// The description prose from the paired `<dd>`.
    pub description: String,
}

/// Prose attached to an anchor id, usually the first meaningful paragraph
/// after a section heading, property table, or attribute definition row.
#[derive(Debug, Clone, Serialize)]
pub struct AnchorDescription {
    /// The anchor id the prose describes.
    pub id: String,
    /// Human-readable description prose.
    pub description: String,
}

/// Everything extracted from one chapter/appendix page.
#[derive(Debug, Clone, Serialize)]
pub struct Chapter {
    /// The chapter's source name (e.g. `struct`), backing `<name>.html`.
    pub name: String,
    /// Every `id` anchor on the page.
    pub anchors: Vec<Anchor>,
    /// Term definitions (anchors only).
    pub dfns: Vec<Dfn>,
    /// Example references.
    pub examples: Vec<Example>,
    /// Property value-definition tables.
    pub properties: Vec<PropertyValueDef>,
    /// Glossary term definitions paired with their descriptions.
    pub term_definitions: Vec<TermDefinition>,
    /// Prose descriptions keyed by spec anchor id.
    pub anchor_descriptions: Vec<AnchorDescription>,
}

/// Category membership used to expand the spec's publish-time `<edit:*category>`
/// macros (which are empty in source HTML) back into prose. Keyed by category
/// name; values are the member element/attribute names, in document order.
#[derive(Debug, Default)]
pub struct MacroIndex {
    /// Element-category name to its member element names.
    pub element_categories: BTreeMap<String, Vec<String>>,
    /// Attribute-category name to its member attribute names.
    pub attribute_categories: BTreeMap<String, Vec<String>>,
}

/// Extract anchors, definitions, examples, properties, and term definitions
/// from a chapter's HTML. `macros` supplies category membership so that the
/// publish-time `<edit:*category>` placeholders in descriptions are expanded
/// rather than dropped.
///
/// # Errors
/// Returns an error if the HTML cannot be parsed.
pub fn extract_chapter(name: &str, html: &str, macros: &MacroIndex) -> Fallible<Chapter> {
    let dom = tl::parse(html, ParserOptions::default())?;
    let parser = dom.parser();
    let mut chapter = Chapter {
        name: name.to_owned(),
        anchors: Vec::new(),
        dfns: Vec::new(),
        examples: Vec::new(),
        properties: Vec::new(),
        term_definitions: Vec::new(),
        anchor_descriptions: Vec::new(),
    };
    let mut pending_description_ids: Vec<String> = Vec::new();
    let mut section_intro: Option<String> = None;

    for node in dom.nodes() {
        let Some(tag) = node.as_tag() else {
            continue;
        };
        let tag_name = tag.name().as_utf8_str();

        if let Some(id) = attr(tag, "id") {
            let text = if is_heading(&tag_name) {
                Some(normalize_ws(&tag.inner_text(parser)))
            } else {
                None
            };
            chapter.anchors.push(Anchor {
                id,
                tag: tag_name.clone().into_owned(),
                text,
            });
        }

        if is_heading(&tag_name)
            && let Some(id) = attr(tag, "id")
        {
            pending_description_ids = vec![id];
            section_intro = None;
            continue;
        }

        if has_class(tag, "propdef") {
            let ids = dfn_ids(tag, parser);
            if !ids.is_empty() {
                pending_description_ids = ids;
            }
        }

        if tag_name == "edit:elementsummary" {
            pending_description_ids.clear();
        }

        if handle_paragraph_description(
            tag,
            parser,
            macros,
            &mut pending_description_ids,
            &mut section_intro,
            &mut chapter.anchor_descriptions,
        ) {
            continue;
        }

        match tag_name.as_ref() {
            "dfn" => chapter.dfns.push(Dfn {
                id: attr(tag, "id"),
                term: normalize_ws(&tag.inner_text(parser)),
                kind: attr(tag, "data-dfn-type"),
            }),
            "edit:example" => chapter.examples.push(Example {
                href: attr(tag, "href"),
                image: attr(tag, "image"),
                link: attr(tag, "link"),
            }),
            "table" if has_class(tag, "propdef") => {
                if let Some(property) = extract_propdef(tag, parser) {
                    chapter.properties.push(property);
                }
            }
            "dl" if has_class(tag, "definitions") => {
                extract_definition_list(tag, parser, macros, &mut chapter.term_definitions);
            }
            "dl" if is_attribute_definition_list(tag) => {
                extract_attribute_definition_list(
                    tag,
                    parser,
                    macros,
                    &mut chapter.anchor_descriptions,
                );
            }
            _ => {}
        }
    }

    Ok(chapter)
}

/// Extract only CSS/SVG property-definition tables from an HTML page.
///
/// This is the fast path for external CSS specs: unlike [`extract_chapter`],
/// it does not walk prose anchors, examples, dfn panels, or descriptions.
///
pub fn extract_property_definitions(html: &str) -> Vec<PropertyValueDef> {
    let mut properties = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<table") {
        let start = offset + relative_start;
        let Some(relative_head_end) = html[start..].find('>') else {
            break;
        };
        let head_end = start + relative_head_end + 1;
        let Some(relative_end) = html[head_end..].find("</table>") else {
            offset = head_end;
            continue;
        };
        let end = head_end + relative_end + "</table>".len();
        if table_head_mentions_propdef(&html[start..head_end]) {
            let table_html = &html[start..end];
            if let Some(property) = extract_raw_propdef(table_html) {
                properties.push(property);
            }
        }
        offset = end;
    }
    properties.extend(extract_css2_propinfo_definitions(html));
    properties
}

fn table_head_mentions_propdef(head: &str) -> bool {
    head.to_ascii_lowercase().contains("propdef")
}

fn extract_raw_propdef(table_html: &str) -> Option<PropertyValueDef> {
    let mut name = None;
    let mut id = None;
    let mut value = None;
    let mut initial = None;
    let mut applies_to = None;
    let mut inherited = None;
    let mut computed_value = None;
    let mut animation_type = None;

    for row in raw_rows(table_html) {
        let cells = raw_cells(row);
        let Some(label) = cells.first() else {
            continue;
        };
        let cell = cells.get(1).cloned().unwrap_or_default();
        match label
            .trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "name" => {
                name = Some(cell);
                id = first_raw_dfn_id(row);
            }
            "value" => value = Some(cell),
            "initial" => initial = Some(cell),
            "applies to" => applies_to = Some(cell),
            "inherited" => inherited = Some(cell),
            "computed value" => computed_value = Some(cell),
            "animation type" => animation_type = Some(cell),
            _ => {}
        }
    }

    let name = name?;
    let keywords = value.as_deref().map(value_keywords).unwrap_or_default();
    Some(PropertyValueDef {
        name,
        id,
        value,
        keywords,
        initial,
        applies_to,
        inherited,
        computed_value,
        animation_type,
    })
}

fn extract_css2_propinfo_definitions(html: &str) -> Vec<PropertyValueDef> {
    let mut properties = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<div") {
        let start = offset + relative_start;
        let Some(relative_head_end) = html[start..].find('>') else {
            break;
        };
        let head_end = start + relative_head_end + 1;
        let Some(relative_end) = html[head_end..].find("</div>") else {
            offset = head_end;
            continue;
        };
        let end = head_end + relative_end + "</div>".len();
        let block_html = &html[start..end];
        if table_head_mentions_propdef(&html[start..head_end])
            && block_html.contains("class=\"propinfo\"")
            && let Some(property) = extract_raw_propinfo_propdef(block_html)
        {
            properties.push(property);
        }
        offset = end;
    }
    properties
}

fn extract_raw_propinfo_propdef(block_html: &str) -> Option<PropertyValueDef> {
    let id = first_raw_propdef_anchor(block_html)?;
    let name = id.strip_prefix("propdef-")?.to_owned();
    let mut value = None;
    let mut initial = None;
    let mut applies_to = None;
    let mut inherited = None;
    let mut computed_value = None;
    let mut animation_type = None;

    for row in raw_rows(block_html) {
        let cells = raw_cells(row);
        let Some(label) = cells.first() else {
            continue;
        };
        let cell = cells.get(1).cloned().unwrap_or_default();
        match label
            .trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "value" => value = Some(cell),
            "initial" => initial = Some(cell),
            "applies to" => applies_to = Some(cell),
            "inherited" => inherited = Some(cell),
            "computed value" => computed_value = Some(cell),
            "animation type" => animation_type = Some(cell),
            _ => {}
        }
    }

    let keywords = value.as_deref().map(value_keywords).unwrap_or_default();
    Some(PropertyValueDef {
        name,
        id: Some(id),
        value,
        keywords,
        initial,
        applies_to,
        inherited,
        computed_value,
        animation_type,
    })
}

fn raw_rows(table_html: &str) -> Vec<&str> {
    let mut rows = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = table_html[offset..].find("<tr") {
        let start = offset + relative_start;
        let Some(relative_head_end) = table_html[start..].find('>') else {
            break;
        };
        let content_start = start + relative_head_end + 1;
        let relative_end = find_first(&table_html[content_start..], &["<tr", "</table>"])
            .unwrap_or(table_html.len() - content_start);
        let end = content_start + relative_end;
        rows.push(&table_html[content_start..end]);
        offset = end;
    }
    rows
}

fn raw_cells(row_html: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = find_first(&row_html[offset..], &["<th", "<td"]) {
        let start = offset + relative_start;
        let Some(relative_head_end) = row_html[start..].find('>') else {
            break;
        };
        let content_start = start + relative_head_end + 1;
        let relative_end = find_first(
            &row_html[content_start..],
            &["<th", "<td", "</tr", "<tr", "</table>"],
        )
        .unwrap_or(row_html.len() - content_start);
        let end = content_start + relative_end;
        cells.push(normalize_ws(&strip_tags(&row_html[content_start..end])));
        offset = end;
    }
    cells
}

fn find_first(haystack: &str, needles: &[&str]) -> Option<usize> {
    needles
        .iter()
        .filter_map(|needle| haystack.find(needle))
        .min()
}

fn strip_tags(html: &str) -> String {
    // This is not a general HTML tokenizer. It is only used on controlled W3C
    // snippets where `<` inside attribute values is not expected in the text
    // fragments we strip.
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text
}

fn first_raw_dfn_id(html: &str) -> Option<String> {
    let start = html.find("<dfn")?;
    let end = start + html[start..].find('>')?;
    raw_attr(&html[start..end], "id")
}

fn first_raw_propdef_anchor(html: &str) -> Option<String> {
    let mut offset = 0;
    while let Some(relative_start) = html[offset..].find("<a") {
        let start = offset + relative_start;
        let Some(relative_end) = html[start..].find('>') else {
            break;
        };
        let end = start + relative_end;
        let tag_head = &html[start..end];
        if let Some(name) = raw_attr(tag_head, "name")
            && name.starts_with("propdef-")
        {
            return Some(name);
        }
        if let Some(id) = raw_attr(tag_head, "id")
            && id.starts_with("propdef-")
        {
            return Some(id);
        }
        offset = end + 1;
    }
    None
}

fn raw_attr(tag_head: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = tag_head.find(&needle)? + needle.len();
    let value = tag_head[start..].trim_start();
    if let Some(rest) = value.strip_prefix('"') {
        return rest.split_once('"').map(|(value, _)| value.to_owned());
    }
    if let Some(rest) = value.strip_prefix('\'') {
        return rest.split_once('\'').map(|(value, _)| value.to_owned());
    }
    let end = value
        .find(|ch: char| ch.is_whitespace() || ch == '>')
        .unwrap_or(value.len());
    Some(value[..end].to_owned())
}

/// Pair the `<dt>`/`<dd>` children of a definition list into term definitions,
/// walking direct children in document order so each term keeps its own
/// description.
fn extract_definition_list(
    dl: &HTMLTag,
    parser: &Parser,
    macros: &MacroIndex,
    out: &mut Vec<TermDefinition>,
) {
    let mut pending: Option<Dfn> = None;
    for handle in dl.children().top().iter() {
        let Some(child) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        match child.name().as_utf8_str().as_ref() {
            "dt" => pending = Some(term_of(child, parser)),
            "dd" => {
                if let Some(dfn) = pending.take() {
                    out.push(TermDefinition {
                        term: dfn.term,
                        id: dfn.id,
                        kind: dfn.kind,
                        description: description_text(child, parser, macros),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Pair `<dt>` rows containing attribute `<dfn>` ids with their following
/// `<dd>` prose and assign that prose to each id in the row.
fn extract_attribute_definition_list(
    dl: &HTMLTag,
    parser: &Parser,
    macros: &MacroIndex,
    out: &mut Vec<AnchorDescription>,
) {
    let mut pending_ids = Vec::new();
    for handle in dl.children().top().iter() {
        let Some(child) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        match child.name().as_utf8_str().as_ref() {
            "dt" => pending_ids = description_anchor_ids(child, parser),
            "dd" if !pending_ids.is_empty() => {
                let description = attribute_description_text(child, parser, macros);
                if !description.is_empty() {
                    let ids = std::mem::take(&mut pending_ids);
                    out.extend(ids.into_iter().map(|id| AnchorDescription {
                        id,
                        description: description.clone(),
                    }));
                }
            }
            _ => {}
        }
    }
}

fn handle_paragraph_description(
    tag: &HTMLTag,
    parser: &Parser,
    macros: &MacroIndex,
    pending_ids: &mut Vec<String>,
    section_intro: &mut Option<String>,
    out: &mut Vec<AnchorDescription>,
) -> bool {
    if tag.name().as_utf8_str().as_ref() != "p" {
        return false;
    }

    let description = description_text(tag, parser, macros);
    let usable = is_description_paragraph(tag, &description);
    if pending_ids.is_empty() {
        if section_intro.is_none() && usable {
            *section_intro = Some(description);
        }
        return true;
    }

    if usable {
        if section_intro.is_none() {
            *section_intro = Some(description.clone());
        }
        append_anchor_descriptions(pending_ids, &description, out);
    } else if let Some(description) = section_intro.clone() {
        append_anchor_descriptions(pending_ids, &description, out);
    }
    true
}

fn append_anchor_descriptions(
    pending_ids: &mut Vec<String>,
    description: &str,
    out: &mut Vec<AnchorDescription>,
) {
    let ids = std::mem::take(pending_ids);
    out.extend(ids.into_iter().map(|id| AnchorDescription {
        id,
        description: description.to_owned(),
    }));
}

fn is_attribute_definition_list(tag: &HTMLTag) -> bool {
    has_class(tag, "attrdef-list") || has_class(tag, "attrdef-list-svg2")
}

fn is_description_paragraph(tag: &HTMLTag, description: &str) -> bool {
    if description.is_empty()
        || has_class(tag, "annotation")
        || has_class(tag, "caption")
        || has_class(tag, "definition")
        || has_class(tag, "prod")
    {
        return false;
    }
    !matches!(
        description.trim(),
        "where:" | "Values have the following meanings:"
    ) && !description.ends_with(" is defined as follows:")
}

fn description_anchor_ids(tag: &HTMLTag, parser: &Parser) -> Vec<String> {
    let ids = dfn_ids(tag, parser);
    if !ids.is_empty() {
        return ids;
    }
    attr(tag, "id").into_iter().collect()
}

fn attribute_description_text(dd: &HTMLTag, parser: &Parser, macros: &MacroIndex) -> String {
    for handle in dd.children().top().iter() {
        let Some(child) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        if child.name().as_utf8_str().as_ref() == "p" && !has_class(child, "annotation") {
            let description = description_text(child, parser, macros);
            if !description.is_empty() {
                return description;
            }
        }
    }
    description_text(dd, parser, macros)
}

/// Build a tag's text content, expanding `<edit:elementcategory>` and
/// `<edit:attributecategory>` placeholders into their member lists (which
/// `inner_text` would otherwise drop, leaving dangling "Specifically:" prose).
fn description_text(tag: &HTMLTag, parser: &Parser, macros: &MacroIndex) -> String {
    let mut buffer = String::new();
    collect_text(tag, parser, macros, &mut buffer);
    normalize_ws(&buffer)
}

/// Append `tag`'s descendant text to `buffer`, substituting category macros.
fn collect_text(tag: &HTMLTag, parser: &Parser, macros: &MacroIndex, buffer: &mut String) {
    for handle in tag.children().top().iter() {
        let Some(node) = handle.get(parser) else {
            continue;
        };
        if let Some(child) = node.as_tag() {
            match child.name().as_utf8_str().as_ref() {
                "edit:elementcategory" => {
                    push_members(&macros.element_categories, child, buffer);
                }
                "edit:attributecategory" => {
                    push_members(&macros.attribute_categories, child, buffer);
                }
                _ => collect_text(child, parser, macros, buffer),
            }
        } else if let Some(raw) = node.as_raw() {
            buffer.push_str(&raw.as_utf8_str());
        }
    }
}

/// Append the comma-joined members of the category named by `tag`'s `name`.
fn push_members(members: &BTreeMap<String, Vec<String>>, tag: &HTMLTag, buffer: &mut String) {
    if let Some(name) = attr(tag, "name")
        && let Some(names) = members.get(&name)
    {
        buffer.push_str(&names.join(", "));
    }
}

/// The term a `<dt>` defines: its inner `<dfn>` when present, else the `<dt>`'s
/// own text (so no term is dropped).
fn term_of(dt: &HTMLTag, parser: &Parser) -> Dfn {
    if let Some(handle) = dt
        .query_selector(parser, "dfn")
        .and_then(|mut hits| hits.next())
        && let Some(dfn) = handle.get(parser).and_then(|node| node.as_tag())
    {
        return Dfn {
            id: attr(dfn, "id"),
            term: normalize_ws(&dfn.inner_text(parser)),
            kind: attr(dfn, "data-dfn-type"),
        };
    }
    Dfn {
        id: None,
        term: normalize_ws(&dt.inner_text(parser)),
        kind: None,
    }
}

fn dfn_ids(tag: &HTMLTag, parser: &Parser) -> Vec<String> {
    let Some(hits) = tag.query_selector(parser, "dfn") else {
        return Vec::new();
    };
    hits.filter_map(|handle| handle.get(parser).and_then(|node| node.as_tag()))
        .filter_map(|dfn| attr(dfn, "id"))
        .collect()
}

/// Extract a single property value-definition table into a [`PropertyValueDef`].
///
/// Returns `None` when the table has no `Name:` row (so it is not a real
/// property definition).
fn extract_propdef(table: &HTMLTag, parser: &Parser) -> Option<PropertyValueDef> {
    let mut name = None;
    let mut id = None;
    let mut value = None;
    let mut initial = None;
    let mut applies_to = None;
    let mut inherited = None;
    let mut computed_value = None;
    let mut animation_type = None;

    let rows = table.query_selector(parser, "tr")?;
    for handle in rows {
        let Some(row) = handle.get(parser).and_then(|node| node.as_tag()) else {
            continue;
        };
        let Some((label, cell)) = propdef_row_label_and_value(row, parser) else {
            continue;
        };
        match label
            .trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "name" => {
                name = Some(cell);
                id = first_attr(row, parser, "dfn", "id");
            }
            "value" => value = Some(cell),
            "initial" => initial = Some(cell),
            "applies to" => applies_to = Some(cell),
            "inherited" => inherited = Some(cell),
            "computed value" => computed_value = Some(cell),
            "animation type" => animation_type = Some(cell),
            _ => {}
        }
    }

    let name = name?;
    let keywords = value.as_deref().map(value_keywords).unwrap_or_default();
    Some(PropertyValueDef {
        name,
        id,
        value,
        keywords,
        initial,
        applies_to,
        inherited,
        computed_value,
        animation_type,
    })
}

/// The bare keyword alternatives in a value grammar (the enum members).
///
/// Splits on the CSS `|` alternation and keeps only bare identifier tokens,
/// dropping `<type>` references, functional notation, and bracketed groups. The
/// full grammar is retained separately, so this is a convenience view, not the
/// source of truth.
fn value_keywords(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|token| is_keyword_token(token))
        .map(str::to_owned)
        .collect()
}

/// Whether `tag` carries `class` among its space-separated class list.
fn has_class(tag: &HTMLTag, class: &str) -> bool {
    attr(tag, "class").is_some_and(|classes| classes.split_whitespace().any(|each| each == class))
}

fn propdef_row_label_and_value(row: &HTMLTag, parser: &Parser) -> Option<(String, String)> {
    if let Some(label) = first_text(row, parser, "th") {
        return Some((label, first_text(row, parser, "td").unwrap_or_default()));
    }

    let mut cells = row.query_selector(parser, "td")?;
    let label = handle_text(cells.next()?, parser)?;
    let value = cells
        .next()
        .and_then(|handle| handle_text(handle, parser))
        .unwrap_or_default();
    Some((label, value))
}

/// The normalized inner text of the first descendant matching `selector`.
fn first_text(tag: &HTMLTag, parser: &Parser, selector: &str) -> Option<String> {
    let handle = tag.query_selector(parser, selector)?.next()?;
    handle_text(handle, parser)
}

fn handle_text(handle: tl::NodeHandle, parser: &Parser) -> Option<String> {
    let found = handle.get(parser)?.as_tag()?;
    Some(normalize_ws(&found.inner_text(parser)))
}

/// The value of `attr_key` on the first descendant matching `selector`.
fn first_attr(tag: &HTMLTag, parser: &Parser, selector: &str, attr_key: &str) -> Option<String> {
    let handle = tag.query_selector(parser, selector)?.next()?;
    let found = handle.get(parser)?.as_tag()?;
    attr(found, attr_key)
}

/// Whether a tag name is an HTML heading (`h1`..`h6`).
fn is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// The value of attribute `key` on `tag`, if present with a value.
fn attr(tag: &HTMLTag, key: &str) -> Option<String> {
    match tag.attributes().get(key) {
        Some(Some(value)) => Some(value.as_utf8_str().into_owned()),
        _ => None,
    }
}

/// Decode HTML entities, then collapse whitespace runs into single spaces.
///
/// `tl`'s `inner_text` returns text verbatim (entities undecoded), so value
/// grammars come back as `auto | &lt;length-percentage&gt;`; this restores them
/// to `auto | <length-percentage>`.
fn normalize_ws(text: &str) -> String {
    decode_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Decode the HTML entities the spec text uses (named basics plus numeric
/// references) in a single pass, leaving unrecognized `&...;` runs verbatim.
fn decode_entities(input: &str) -> Cow<'_, str> {
    if !input.contains('&') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(semi) = after.find(';')
            && let Some(decoded) = decode_entity(&after[1..semi])
        {
            out.push(decoded);
            rest = &after[semi + 1..];
            continue;
        }
        out.push('&');
        rest = &after[1..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Decode one entity body (the text between `&` and `;`).
fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{00A0}'),
        _ => {
            let code = entity.strip_prefix('#')?;
            let value = match code.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<h2 id="Shapes">Basic <span>Shapes</span></h2>
<p>Shapes are graphical elements.</p>
<dl class="definitions">
  <dt><dfn id="term-shape" data-dfn-type="dfn">shape</dfn></dt>
  <dd>A graphics element with a defined outline.</dd>
</dl>
<table class="propdef def">
  <tr><th>Name:</th><td><dfn id="TextAnchor" data-dfn-type="property">text-anchor</dfn></td></tr>
  <tr><th>Value:</th><td>start | middle | end</td></tr>
  <tr><th>Initial:</th><td>start</td></tr>
  <tr><th>Inherited:</th><td>yes</td></tr>
</table>
<p>The <a>'text-anchor'</a> property aligns text.</p>
<dl class="attrdef-list">
  <dt><table><tr><td><dfn id="DemoAttribute">demo</dfn></td></tr></table></dt>
  <dd>The demo attribute controls demo behavior.</dd>
</dl>
<dl class="attrdef-list-svg2">
  <dt id="DirectAttribute"><span class="adef">direct</span></dt>
  <dd><p>The direct attribute uses its dt id.</p><dl><dt>Value</dt><dd>number</dd></dl></dd>
</dl>
<edit:example href='images/x.svg' image='no'/>"#;

    #[test]
    fn value_keywords_keeps_only_bare_keywords() {
        assert_eq!(
            value_keywords("start | middle | end"),
            ["start", "middle", "end"]
        );
        assert_eq!(value_keywords("auto | <length-percentage>"), ["auto"]);
        assert_eq!(value_keywords("<paint>"), Vec::<String>::new());
        assert_eq!(value_keywords("nonzero | evenodd"), ["nonzero", "evenodd"]);
    }

    #[test]
    fn decode_entities_handles_named_and_numeric() {
        assert_eq!(decode_entities("a&lt;b&gt;c").as_ref(), "a<b>c");
        assert_eq!(decode_entities("&amp;&quot;").as_ref(), "&\"");
        assert_eq!(decode_entities("x&#65;y").as_ref(), "xAy");
        assert_eq!(decode_entities("x&#x41;y").as_ref(), "xAy");
        assert_eq!(decode_entities("plain text").as_ref(), "plain text");
        // Unrecognized entity body is left verbatim.
        assert_eq!(decode_entities("a&bogus;b").as_ref(), "a&bogus;b");
    }

    #[test]
    fn extracts_chapter_entities() -> Result<(), Box<dyn std::error::Error>> {
        let ch = extract_chapter("shapes", HTML, &MacroIndex::default())?;

        // Heading anchor keeps its (entity/whitespace-normalized) text.
        let shapes = ch
            .anchors
            .iter()
            .find(|a| a.id == "Shapes")
            .ok_or("no Shapes anchor")?;
        assert_eq!(shapes.tag, "h2");
        assert_eq!(shapes.text.as_deref(), Some("Basic Shapes"));

        assert_eq!(ch.examples.len(), 1);
        assert_eq!(ch.examples[0].href.as_deref(), Some("images/x.svg"));
        assert_eq!(ch.examples[0].image.as_deref(), Some("no"));

        assert_eq!(ch.properties.len(), 1);
        let prop = &ch.properties[0];
        assert_eq!(prop.name, "text-anchor");
        assert_eq!(prop.id.as_deref(), Some("TextAnchor"));
        assert_eq!(prop.value.as_deref(), Some("start | middle | end"));
        assert_eq!(prop.keywords, ["start", "middle", "end"]);
        assert_eq!(prop.initial.as_deref(), Some("start"));
        assert_eq!(prop.inherited.as_deref(), Some("yes"));

        assert_eq!(ch.term_definitions.len(), 1);
        let term = &ch.term_definitions[0];
        assert_eq!(term.term, "shape");
        assert_eq!(term.id.as_deref(), Some("term-shape"));
        assert_eq!(term.kind.as_deref(), Some("dfn"));
        assert_eq!(
            term.description,
            "A graphics element with a defined outline."
        );

        let descriptions: std::collections::BTreeMap<_, _> = ch
            .anchor_descriptions
            .iter()
            .map(|description| (description.id.as_str(), description.description.as_str()))
            .collect();
        assert_eq!(
            descriptions.get("Shapes").copied(),
            Some("Shapes are graphical elements.")
        );
        assert_eq!(
            descriptions.get("TextAnchor").copied(),
            Some("The 'text-anchor' property aligns text.")
        );
        assert_eq!(
            descriptions.get("DemoAttribute").copied(),
            Some("The demo attribute controls demo behavior.")
        );
        assert_eq!(
            descriptions.get("DirectAttribute").copied(),
            Some("The direct attribute uses its dt id.")
        );
        Ok(())
    }

    #[test]
    fn property_descriptions_skip_value_explanation_boilerplate()
    -> Result<(), Box<dyn std::error::Error>> {
        let html = r#"<h3 id="DashSection">Dash section</h3>
<table class="propdef">
  <tr><th>Name:</th><td><dfn id="DashProperty">stroke-dasharray</dfn></td></tr>
  <tr><th>Value:</th><td>none | &lt;dasharray&gt;</td></tr>
</table>
<p>where:</p>
<p class="definition prod">&lt;dasharray&gt; = [ &lt;number&gt;+ ]#</p>
<p>The 'stroke-dasharray' property controls the pattern of dashes and gaps.</p>
<h3 id="AnchorSection">Anchor section</h3>
<p>The 'text-anchor' property aligns text relative to a point.</p>
<table class="propdef">
  <tr><th>Name:</th><td><dfn id="AnchorProperty">text-anchor</dfn></td></tr>
  <tr><th>Value:</th><td>start | middle | end</td></tr>
</table>
<p>Values have the following meanings:</p>"#;
        let ch = extract_chapter("text", html, &MacroIndex::default())?;
        let descriptions: std::collections::BTreeMap<_, _> = ch
            .anchor_descriptions
            .iter()
            .map(|description| (description.id.as_str(), description.description.as_str()))
            .collect();

        assert_eq!(
            descriptions.get("DashProperty").copied(),
            Some("The 'stroke-dasharray' property controls the pattern of dashes and gaps.")
        );
        assert_eq!(
            descriptions.get("AnchorProperty").copied(),
            Some("The 'text-anchor' property aligns text relative to a point.")
        );
        Ok(())
    }

    #[test]
    fn decodes_entities_in_value_grammar() -> Result<(), Box<dyn std::error::Error>> {
        let html = r#"<table class="propdef">
  <tr><th>Name:</th><td><dfn id="P">inline-size</dfn></td></tr>
  <tr><th>Value:</th><td>auto | <a>&lt;length-percentage&gt;</a></td></tr>
</table>"#;
        let ch = extract_chapter("text", html, &MacroIndex::default())?;
        assert_eq!(ch.properties.len(), 1);
        assert_eq!(
            ch.properties[0].value.as_deref(),
            Some("auto | <length-percentage>")
        );
        assert_eq!(ch.properties[0].keywords, ["auto"]);
        Ok(())
    }

    #[test]
    fn extracts_legacy_css_propdef_tables_with_td_labels() -> Result<(), Box<dyn std::error::Error>>
    {
        let html = r#"<table class="propdef">
  <tr><td>Name:</td><td><dfn id="propdef-font-style">font-style</dfn></td></tr>
  <tr><td>Value:</td><td>normal | italic | oblique</td></tr>
  <tr><td>Animation type:</td><td>no</td></tr>
</table>"#;
        let ch = extract_chapter("css-fonts-3", html, &MacroIndex::default())?;
        assert_eq!(ch.properties.len(), 1);
        let prop = &ch.properties[0];
        assert_eq!(prop.name, "font-style");
        assert_eq!(prop.id.as_deref(), Some("propdef-font-style"));
        assert_eq!(prop.value.as_deref(), Some("normal | italic | oblique"));
        assert_eq!(prop.keywords, ["normal", "italic", "oblique"]);
        assert_eq!(prop.animation_type.as_deref(), Some("no"));
        Ok(())
    }

    #[test]
    fn extracts_bikeshed_propdef_tables_with_optional_cell_closures() {
        let html = r#"
<table class="data"><tr><th>Other<td>ignored</table>
<table class="def propdef" data-link-for-hint="clip-rule">
  <tbody>
    <tr>
      <th>Name:
      <td><dfn class="css" id="propdef-clip-rule">clip-rule</dfn>
    <tr class="value">
      <th>Value:
      <td class="prod">nonzero | evenodd
    <tr>
      <th>Animation type:
      <td>discrete
  </table>"#;
        let properties = extract_property_definitions(html);

        assert_eq!(properties.len(), 1);
        let prop = &properties[0];
        assert_eq!(prop.name, "clip-rule");
        assert_eq!(prop.id.as_deref(), Some("propdef-clip-rule"));
        assert_eq!(prop.value.as_deref(), Some("nonzero | evenodd"));
        assert_eq!(prop.keywords, ["nonzero", "evenodd"]);
        assert_eq!(prop.animation_type.as_deref(), Some("discrete"));
    }

    #[test]
    fn extracts_css2_propinfo_blocks() {
        let html = r#"
<div class="propdef">
<dl><dt>
<span class="index-def" title="'display'"><a name="propdef-display" class="propdef-title"><strong>'display'</strong></a></span>
<dd>
<table class="propinfo" cellspacing=0 cellpadding=0>
<tr valign=baseline><td><em>Value:</em>&nbsp;&nbsp;<td>inline | block | list-item | inline-block |
table | inline-table | table-row-group | table-header-group |
table-footer-group | table-row | table-column-group | table-column |
table-cell | table-caption | none | <a href="cascade.html#value-def-inherit" class="noxref"><span class="value-inst-inherit">inherit</span></a>
<tr valign=baseline><td><em>Initial:</em>&nbsp;&nbsp;<td>inline
<tr valign=baseline><td><em>Inherited:</em>&nbsp;&nbsp;<td>no
</table>
</dl>
</div>"#;
        let properties = extract_property_definitions(html);

        assert_eq!(properties.len(), 1);
        let prop = &properties[0];
        assert_eq!(prop.name, "display");
        assert_eq!(prop.id.as_deref(), Some("propdef-display"));
        assert!(prop.keywords.contains(&"inline-block".to_owned()));
        assert!(prop.keywords.contains(&"inherit".to_owned()));
        assert!(!prop.keywords.contains(&"run-in".to_owned()));
        assert_eq!(prop.initial.as_deref(), Some("inline"));
        assert_eq!(prop.inherited.as_deref(), Some("no"));
    }

    #[test]
    fn expands_category_macro_in_description() -> Result<(), Box<dyn std::error::Error>> {
        let html = r"<dl class='definitions'>
  <dt><dfn id='c' data-dfn-type='dfn'>container element</dfn></dt>
  <dd>An element. Specifically: <edit:elementcategory name='container'/>.</dd>
</dl>";
        let mut macros = MacroIndex::default();
        macros.element_categories.insert(
            "container".to_owned(),
            vec!["svg".to_owned(), "g".to_owned(), "defs".to_owned()],
        );
        let ch = extract_chapter("struct", html, &macros)?;
        assert_eq!(ch.term_definitions.len(), 1);
        assert_eq!(
            ch.term_definitions[0].description,
            "An element. Specifically: svg, g, defs."
        );
        Ok(())
    }
}
