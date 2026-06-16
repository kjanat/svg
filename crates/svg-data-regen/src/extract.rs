//! Extract structured entities from a definitions module (`definitions.xml` and
//! the per-feature `definitions-*.xml` files).
//!
//! A definitions module is a flat-ish list of entity declarations: elements
//! (with their attribute categories, interfaces, an optional inline content
//! model, and any element-local attributes), global attributes, properties,
//! element categories (a named set of member elements), and attribute
//! categories (a named set of member attributes). This module streams the XML
//! once and routes each declaration into a typed record, preserving document
//! order (which, pinned to a commit, is deterministic).

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::reader::Reader;
use serde::Serialize;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// A reference to an attribute, as declared on an element or in an attribute
/// category. The href is the spec anchor (often within the SVG spec, sometimes
/// an absolute URL into another spec such as WAI-ARIA).
#[derive(Debug, Clone, Serialize)]
pub struct AttributeRef {
    /// Attribute name.
    pub name: String,
    /// Spec anchor or absolute URL, when declared.
    pub href: Option<String>,
    /// Whether the attribute is animatable (`animatable='yes'|'no'`).
    pub animatable: Option<bool>,
}

/// An element declaration.
#[derive(Debug, Clone, Serialize)]
pub struct ElementDef {
    /// Element name.
    pub name: String,
    /// Spec anchor for the element's definition.
    pub href: Option<String>,
    /// Names of the attribute categories the element pulls attributes from.
    pub attribute_categories: Vec<String>,
    /// IDL interface names the element implements.
    pub interfaces: Vec<String>,
    /// The inline content-model prose, when the module declares one (most
    /// elements declare it in chapter prose instead; that is extracted later).
    pub content_model: Option<String>,
    /// Attributes declared directly on the element (in addition to those it
    /// inherits from its attribute categories).
    pub attributes: Vec<AttributeRef>,
}

/// A property declaration.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyDef {
    /// Property name.
    pub name: String,
    /// Spec anchor for the property's definition.
    pub href: Option<String>,
}

/// An element category: a named set of member element names.
#[derive(Debug, Clone, Serialize)]
pub struct ElementCategory {
    /// Category name (e.g. `container`, `shape`, `gradient`).
    pub name: String,
    /// Spec anchor for the category's definition.
    pub href: Option<String>,
    /// Member element names.
    pub elements: Vec<String>,
}

/// An attribute category: a named set of member attributes.
#[derive(Debug, Clone, Serialize)]
pub struct AttributeCategory {
    /// Category name (e.g. `core`, `presentation`, `aria`).
    pub name: String,
    /// Spec anchor for the category's definition.
    pub href: Option<String>,
    /// Member attributes.
    pub attributes: Vec<AttributeRef>,
}

/// A glossary-style entry (a `<term>`, `<symbol>`, or top-level `<interface>`)
/// captured so no declared name is dropped.
#[derive(Debug, Clone, Serialize)]
pub struct GlossaryEntry {
    /// The term/symbol/interface name.
    pub name: String,
    /// Spec anchor, when declared.
    pub href: Option<String>,
}

/// Everything extracted from one definitions module.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Definitions {
    /// External anchor base for this module, when its entities are defined in
    /// another spec (a CSS draft). Relative hrefs in the module resolve against
    /// it; `None` means they resolve within the SVG spec itself.
    pub anchor_base: Option<String>,
    /// Element declarations.
    pub elements: Vec<ElementDef>,
    /// Top-level (global) attribute declarations.
    pub global_attributes: Vec<AttributeRef>,
    /// Property declarations.
    pub properties: Vec<PropertyDef>,
    /// Element categories.
    pub element_categories: Vec<ElementCategory>,
    /// Attribute categories.
    pub attribute_categories: Vec<AttributeCategory>,
    /// Glossary terms.
    pub terms: Vec<GlossaryEntry>,
    /// Defined symbols.
    pub symbols: Vec<GlossaryEntry>,
    /// IDL interfaces declared at the top level.
    pub interfaces: Vec<GlossaryEntry>,
}

/// Which container a nested `<attribute>` or `<x:contentmodel>` belongs to.
enum Context {
    /// Not inside an element or attribute category.
    Top,
    /// Inside an `<element>` being assembled.
    Element(ElementDef),
    /// Inside an `<attributecategory>` being assembled.
    AttributeCategory(AttributeCategory),
}

/// Extract all entities from a definitions module's XML.
///
/// `anchor_base` is the module's external anchor base from `publish.xml` (the
/// `base` attribute on its `<definitions>` entry), carried onto the result so
/// later phases can resolve the module's hrefs into permalinks.
///
/// # Errors
/// Returns an error if the XML is malformed or an attribute value cannot be
/// decoded.
pub fn extract_definitions(xml: &str, anchor_base: Option<String>) -> Fallible<Definitions> {
    let mut reader = Reader::from_str(xml);
    let mut defs = Definitions {
        anchor_base,
        ..Definitions::default()
    };
    let mut context = Context::Top;
    let mut in_content_model = false;

    loop {
        match reader.read_event()? {
            Event::Eof => break,
            Event::Start(element) => {
                start_element(&element, &mut defs, &mut context, &mut in_content_model)?;
            }
            Event::Empty(element) => {
                empty_element(&element, &mut defs, &mut context)?;
            }
            Event::Text(text) if in_content_model => {
                if let Context::Element(current) = &mut context {
                    let prose = text.xml10_content()?;
                    append_content_model(current, prose.trim());
                }
            }
            Event::End(element) => {
                end_element(&element, &mut defs, &mut context, &mut in_content_model);
            }
            _ => {}
        }
    }

    Ok(defs)
}

/// Handle a `<...>` start tag (one that has children or is a container).
fn start_element(
    element: &BytesStart,
    defs: &mut Definitions,
    context: &mut Context,
    in_content_model: &mut bool,
) -> Fallible<()> {
    match element.local_name().as_ref() {
        b"element" => *context = Context::Element(parse_element_head(element)?),
        b"attributecategory" => {
            *context = Context::AttributeCategory(AttributeCategory {
                name: required(element, b"name")?,
                href: attribute(element, b"href")?,
                attributes: Vec::new(),
            });
        }
        b"contentmodel" => *in_content_model = true,
        // A nested attribute inside an element/category opened as a container.
        b"attribute" => route_attribute(parse_attribute_ref(element)?, defs, context),
        _ => {}
    }
    Ok(())
}

/// Handle a self-closing `<.../>` tag.
fn empty_element(
    element: &BytesStart,
    defs: &mut Definitions,
    context: &mut Context,
) -> Fallible<()> {
    match element.local_name().as_ref() {
        b"element" => {
            // An element with no children (no inline content model or attrs).
            defs.elements.push(parse_element_head(element)?);
        }
        b"attribute" => route_attribute(parse_attribute_ref(element)?, defs, context),
        b"property" => defs.properties.push(PropertyDef {
            name: required(element, b"name")?,
            href: attribute(element, b"href")?,
        }),
        b"elementcategory" => defs.element_categories.push(ElementCategory {
            name: required(element, b"name")?,
            href: attribute(element, b"href")?,
            elements: comma_list(element, b"elements")?,
        }),
        b"term" => defs.terms.push(glossary_entry(element)?),
        b"symbol" => defs.symbols.push(glossary_entry(element)?),
        b"interface" => defs.interfaces.push(glossary_entry(element)?),
        _ => {}
    }
    Ok(())
}

/// Handle a `</...>` end tag, closing the current container.
fn end_element(
    element: &BytesEnd,
    defs: &mut Definitions,
    context: &mut Context,
    in_content_model: &mut bool,
) {
    match element.local_name().as_ref() {
        b"contentmodel" => *in_content_model = false,
        b"element" => {
            if let Context::Element(current) = std::mem::replace(context, Context::Top) {
                defs.elements.push(current);
            }
        }
        b"attributecategory" => {
            if let Context::AttributeCategory(current) = std::mem::replace(context, Context::Top) {
                defs.attribute_categories.push(current);
            }
        }
        _ => {}
    }
}

/// Parse an `<element>`'s own attributes (not its children).
fn parse_element_head(element: &BytesStart) -> Fallible<ElementDef> {
    Ok(ElementDef {
        name: required(element, b"name")?,
        href: attribute(element, b"href")?,
        attribute_categories: comma_list(element, b"attributecategories")?,
        interfaces: comma_list(element, b"interfaces")?,
        content_model: None,
        attributes: Vec::new(),
    })
}

/// Build an [`AttributeRef`] from an `<attribute>` tag's attributes.
fn parse_attribute_ref(element: &BytesStart) -> Fallible<AttributeRef> {
    Ok(AttributeRef {
        name: required(element, b"name")?,
        href: attribute(element, b"href")?,
        animatable: match attribute(element, b"animatable")?.as_deref() {
            Some("yes") => Some(true),
            Some("no") => Some(false),
            _ => None,
        },
    })
}

/// Route an attribute into the open element, open attribute category, or the
/// top-level global attribute list.
fn route_attribute(attr: AttributeRef, defs: &mut Definitions, context: &mut Context) {
    match context {
        Context::Element(current) => current.attributes.push(attr),
        Context::AttributeCategory(current) => current.attributes.push(attr),
        Context::Top => defs.global_attributes.push(attr),
    }
}

/// Append a chunk of content-model prose to the open element.
fn append_content_model(element: &mut ElementDef, prose: &str) {
    if prose.is_empty() {
        return;
    }
    match &mut element.content_model {
        Some(existing) => {
            existing.push(' ');
            existing.push_str(prose);
        }
        None => element.content_model = Some(prose.to_owned()),
    }
}

/// Build a [`GlossaryEntry`] from a `<term>`/`<symbol>`/`<interface>` tag.
fn glossary_entry(element: &BytesStart) -> Fallible<GlossaryEntry> {
    Ok(GlossaryEntry {
        name: required(element, b"name")?,
        href: attribute(element, b"href")?,
    })
}

/// The value of a required attribute, erroring if it is absent.
fn required(element: &BytesStart, key: &[u8]) -> Fallible<String> {
    attribute(element, key)?.ok_or_else(|| {
        let key = String::from_utf8_lossy(key).into_owned();
        Box::<dyn std::error::Error>::from(format!("definitions entity missing `{key}`"))
    })
}

/// The unescaped value of attribute `key`, if present.
fn attribute(element: &BytesStart, key: &[u8]) -> Fallible<Option<String>> {
    for attr in element.attributes() {
        let attr = attr?;
        if attr.key.local_name().as_ref() == key {
            return Ok(Some(
                attr.normalized_value(quick_xml::XmlVersion::default())?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

/// Parse a comma-separated attribute value into trimmed, non-empty names.
fn comma_list(element: &BytesStart, key: &[u8]) -> Fallible<Vec<String>> {
    let Some(raw) = attribute(element, key)? else {
        return Ok(Vec::new());
    };
    Ok(raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect())
}
