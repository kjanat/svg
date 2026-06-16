//! Extract permalink anchors, term definitions, and example references from a
//! chapter or appendix HTML page.
//!
//! Chapter source HTML carries the prose, the `id` anchors that element and
//! attribute hrefs point at, the `<dfn>` term definitions, and `<edit:example>`
//! references. (The rendered element-summary tables are injected at publish
//! time from `definitions.xml`, so the structural content model is extracted
//! from there, not here.) This module turns one page into a typed record.

use serde::Serialize;
use tl::{HTMLTag, ParserOptions};

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

/// Everything extracted from one chapter/appendix page.
#[derive(Debug, Clone, Serialize)]
pub struct Chapter {
    /// The chapter's source name (e.g. `struct`), backing `<name>.html`.
    pub name: String,
    /// Every `id` anchor on the page.
    pub anchors: Vec<Anchor>,
    /// Term definitions.
    pub dfns: Vec<Dfn>,
    /// Example references.
    pub examples: Vec<Example>,
}

/// Extract anchors, definitions, and examples from a chapter's HTML.
///
/// # Errors
/// Returns an error if the HTML cannot be parsed.
pub fn extract_chapter(name: &str, html: &str) -> Fallible<Chapter> {
    let dom = tl::parse(html, ParserOptions::default())?;
    let parser = dom.parser();
    let mut chapter = Chapter {
        name: name.to_owned(),
        anchors: Vec::new(),
        dfns: Vec::new(),
        examples: Vec::new(),
    };

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
            _ => {}
        }
    }

    Ok(chapter)
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

/// Collapse runs of whitespace into single spaces and trim.
fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
