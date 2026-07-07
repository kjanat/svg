//! Parse `publish.xml` - the SVG 2 publication manifest - into the input graph.
//!
//! The manifest is the authoritative list of what the spec is built from: the
//! permalink/render base URLs (`<versions>`), the definitions modules
//! (`<definitions>`), and every chapter, appendix, and index page. Walking it
//! (rather than hardcoding a file list) keeps the pipeline faithful to whatever
//! upstream currently ships.
//!
//! # Sources parsed
//!
//! - [`publish.xml`][publish] — fetched pinned to the run's commit; the link
//!   resolves to the current `master`.
//!
//! [publish]: https://raw.githubusercontent.com/w3c/svgwg/master/publish.xml

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::util::boxed;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// A named base URL from the `<versions>` block (e.g. `this`, `latest`, `cvs`).
#[derive(Debug, Clone)]
pub struct VersionLink {
    /// The element's local name (`cvs`, `this`, `latest`, `latestrec`, ...).
    pub name: String,
    /// The URL the element points at.
    pub href: String,
}

/// A `<definitions>` module reference.
#[derive(Debug, Clone)]
pub struct DefinitionsModule {
    /// The href, relative to the manifest's directory (`master/`).
    pub href: String,
    /// An external anchor base, when the module's definitions live in another
    /// spec (e.g. a CSS draft) rather than in `svgwg` itself.
    pub base: Option<String>,
}

/// The full input graph discovered from `publish.xml`.
#[derive(Debug, Clone, Default)]
pub struct PublishGraph {
    /// Maturity level (`<maturity>`, e.g. `ED`, `CR`).
    pub maturity: String,
    /// The `<versions>` base URLs (permalink bases for each edition alias).
    pub versions: Vec<VersionLink>,
    /// The `<definitions>` modules to extract entities from.
    pub definitions: Vec<DefinitionsModule>,
    /// Chapter names (`<chapter name>`), each backing a `master/<name>.html`.
    pub chapters: Vec<String>,
    /// Appendix names (`<appendix name>`), each backing a `master/<name>.html`.
    pub appendices: Vec<String>,
    /// Standalone index/toc page references (`<toc>`, `<elementindex>`, ...).
    pub references: Vec<VersionLink>,
}

/// Parse the publication manifest into its input graph.
///
/// # Errors
/// Returns an error if the XML is malformed or an attribute value cannot be
/// decoded.
pub fn parse_publish(xml: &str) -> Fallible<PublishGraph> {
    let mut reader = Reader::from_str(xml);
    let mut graph = PublishGraph::default();
    let mut in_maturity = false;

    loop {
        match reader.read_event()? {
            Event::Eof => break,
            Event::Start(element) => {
                if element.local_name().as_ref() == b"maturity" {
                    in_maturity = true;
                } else {
                    handle_element(&element, &mut graph)?;
                }
            }
            Event::Empty(element) => handle_element(&element, &mut graph)?,
            Event::Text(text) if in_maturity => {
                text.xml10_content()?.trim().clone_into(&mut graph.maturity);
            }
            Event::End(element) if element.local_name().as_ref() == b"maturity" => {
                in_maturity = false;
            }
            _ => {}
        }
    }

    validate_publish_graph(&graph)?;
    Ok(graph)
}

/// Route a single (possibly self-closing) manifest element into the graph.
fn handle_element(element: &BytesStart, graph: &mut PublishGraph) -> Fallible<()> {
    let local = element.local_name();
    match local.as_ref() {
        b"cvs" | b"cvs-single" | b"this" | b"this-single" | b"latest" | b"latestrec"
        | b"historyURL" => {
            graph.versions.push(VersionLink {
                name: local_name(element)?,
                href: required_attribute(element, b"href")?,
            });
        }
        b"definitions" => {
            graph.definitions.push(DefinitionsModule {
                href: required_attribute(element, b"href")?,
                base: attribute(element, b"base")?,
            });
        }
        b"chapter" => {
            graph.chapters.push(required_attribute(element, b"name")?);
        }
        b"appendix" => {
            graph.appendices.push(required_attribute(element, b"name")?);
        }
        b"toc" | b"elementindex" | b"attributeindex" | b"propertyindex" => {
            graph.references.push(VersionLink {
                name: local_name(element)?,
                href: required_attribute(element, b"href")?,
            });
        }
        _ => {}
    }
    Ok(())
}

/// Validate that the manifest contained the graph sections this pipeline needs.
fn validate_publish_graph(graph: &PublishGraph) -> Fallible<()> {
    let mut missing = Vec::new();
    if graph.maturity.trim().is_empty() {
        missing.push("maturity");
    }
    if graph.versions.is_empty() {
        missing.push("versions");
    }
    if graph.definitions.is_empty() {
        missing.push("definitions");
    }
    if graph.chapters.is_empty() {
        missing.push("chapters");
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(boxed(format!(
        "publish.xml missing required graph section(s): {}",
        missing.join(", ")
    )))
}

/// The element's local name as an owned `String`.
fn local_name(element: &BytesStart) -> Fallible<String> {
    let local = element.local_name();
    Ok(std::str::from_utf8(local.as_ref())?.to_owned())
}

/// The unescaped value of attribute `key` on `element`, if present.
fn attribute(element: &BytesStart, key: &[u8]) -> Fallible<Option<String>> {
    for attribute in element.attributes() {
        let attribute = attribute?;
        if attribute.key.local_name().as_ref() == key {
            return Ok(Some(
                attribute
                    .normalized_value(quick_xml::XmlVersion::default())?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

/// Required attribute value for manifest elements this pipeline consumes.
fn required_attribute(element: &BytesStart, key: &[u8]) -> Fallible<String> {
    let name = local_name(element)?;
    let key_name = std::str::from_utf8(key).unwrap_or("<invalid>");
    match attribute(element, key)? {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(boxed(format!(
            "publish.xml <{name}> missing required `{key_name}` attribute"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUBLISH: &str = r"<publish-conf xmlns='http://mcc.id.au/ns/local'>
  <maturity>ED</maturity>
  <versions>
    <cvs href='https://draft/'/>
    <this href='https://tr/dated/'/>
    <latest href='https://tr/latest/'/>
  </versions>
  <definitions href='definitions.xml'/>
  <definitions href='../specs/x/definitions.xml' base='https://ext/'/>
  <chapter name='intro'/>
  <appendix name='changes'/>
  <toc href='Overview.html'/>
</publish-conf>";

    #[test]
    fn parses_the_input_graph() -> Result<(), Box<dyn std::error::Error>> {
        let graph = parse_publish(PUBLISH)?;
        assert_eq!(graph.maturity, "ED");
        assert_eq!(graph.chapters.len(), 1);
        assert_eq!(graph.chapters[0], "intro");
        assert_eq!(graph.appendices.len(), 1);
        assert_eq!(graph.appendices[0], "changes");

        assert_eq!(graph.definitions.len(), 2);
        assert_eq!(graph.definitions[0].href, "definitions.xml");
        assert_eq!(graph.definitions[0].base, None);
        assert_eq!(graph.definitions[1].href, "../specs/x/definitions.xml");
        assert_eq!(graph.definitions[1].base.as_deref(), Some("https://ext/"));

        let cvs = graph
            .versions
            .iter()
            .find(|v| v.name == "cvs")
            .ok_or("no cvs")?;
        assert_eq!(cvs.href, "https://draft/");
        let this = graph
            .versions
            .iter()
            .find(|v| v.name == "this")
            .ok_or("no this")?;
        assert_eq!(this.href, "https://tr/dated/");

        assert_eq!(graph.references.len(), 1);
        assert_eq!(graph.references[0].name, "toc");
        Ok(())
    }

    #[test]
    fn rejects_missing_required_graph_sections() {
        let Err(error) = parse_publish("<publish-conf><maturity/></publish-conf>") else {
            panic!("empty manifest should fail validation");
        };
        assert!(
            error.to_string().contains("definitions"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_missing_required_manifest_attributes() {
        let Err(error) = parse_publish(
            r"<publish-conf>
              <maturity>ED</maturity>
              <versions><cvs href='https://draft/'/></versions>
              <definitions/>
              <chapter name='intro'/>
            </publish-conf>",
        ) else {
            panic!("definitions without href should fail validation");
        };
        assert!(
            error.to_string().contains("missing required `href`"),
            "unexpected error: {error}"
        );
    }
}
