use svg_tree::{child_of_kind, walk_tree};
use tree_sitter::Tree;

use crate::{
    Span, inline_style_fragments,
    occurrences::{is_structural_iri, parse_paint, url_argument_range},
    outline::element_tag,
    tag_attribute_value,
    text::{
        CssFragment, advance_point, css_unescape, node_span, rebase_point, span_between,
        xml_unescape,
    },
};

/// A reference to a resource outside the current document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalLink {
    /// The reference as written, XML character references decoded, possibly relative.
    pub target: String,
    /// Span of the reference text, quotes and `url(` excluded.
    pub span: Span,
    /// The `xml:base` values in scope, outermost first. A relative target resolves against
    /// the document URL joined with each in turn.
    pub base: Vec<String>,
}

/// Collect references to other resources: `href`/`xlink:href` values, `url(...)` values,
/// `@import` rules, and `<?xml-stylesheet href?>` instructions.
///
/// Fragment-only references to this document (`#id`) and `data:` URIs are left out. An
/// `href` fragment under an `xml:base` names a fragment of that base resource.
#[must_use]
pub fn external_links(source: &[u8], tree: &Tree) -> Vec<ExternalLink> {
    let mut links = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| match node.kind() {
        "iri_reference" if is_structural_iri(node) => {
            if let Ok(text) = node.utf8_text(source) {
                push_link(
                    &mut links,
                    &xml_unescape(text),
                    node_span(node),
                    &xml_bases(node, source),
                    fragment_is_local(node, source),
                );
            }
        }
        "paint_payload" => paint_links(node, source, &mut links),
        "style_text_double" | "style_text_single" => {
            if let Ok(content) = node.utf8_text(source)
                && let Some(fragment) =
                    CssFragment::declarations(content, node.start_byte(), node.start_position())
            {
                css_links(&fragment, &xml_bases(node, source), &mut links);
            }
        }
        "processing_instruction" => stylesheet_pi_link(node, source, &mut links),
        _ => {}
    });
    for (stylesheet, fragment) in inline_style_fragments(source, tree) {
        let bases = tree
            .root_node()
            .descendant_for_byte_range(stylesheet.start_byte, stylesheet.start_byte)
            .map_or_default(|node| xml_bases(node, source));
        css_links(&fragment, &bases, &mut links);
    }
    links.sort_by_key(|link| {
        (
            link.span.start_row,
            link.span.start_col,
            link.span.end_row,
            link.span.end_col,
        )
    });
    links.dedup();
    links
}

fn xml_bases(node: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    let mut bases = Vec::new();
    let mut current = Some(node);
    while let Some(element) = current {
        if let Some(base) =
            element_tag(element).and_then(|tag| tag_attribute_value(source, tag, "xml:base"))
        {
            bases.push(xml_unescape(&base));
        }
        current = element.parent();
    }
    bases.reverse();
    bases
}

pub fn fragment_is_local(iri: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    iri.parent()
        .is_none_or(|parent| parent.kind() != "href_reference")
        || xml_bases(iri, source).iter().all(|base| {
            let base = base.trim();
            base.is_empty() || base.starts_with('#')
        })
}

fn push_link(
    links: &mut Vec<ExternalLink>,
    target: &str,
    span: Span,
    base: &[String],
    local_fragment: bool,
) {
    let target = target.trim();
    let is_data = target
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"));
    if target.is_empty() || (local_fragment && target.starts_with('#')) || is_data {
        return;
    }
    links.push(ExternalLink {
        target: target.to_owned(),
        span,
        base: base.to_vec(),
    });
}

fn paint_links(payload: tree_sitter::Node<'_>, source: &[u8], links: &mut Vec<ExternalLink>) {
    let Some(payload_source) = source.get(payload.byte_range()) else {
        return;
    };
    let Some(tree) = parse_paint(payload_source) else {
        return;
    };
    let base = payload.start_position();
    let bases = xml_bases(payload, source);
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() == "iri_reference"
            && node
                .parent()
                .is_some_and(|parent| parent.kind() == "paint_server")
            && let Ok(text) = node.utf8_text(payload_source)
        {
            push_link(
                links,
                &xml_unescape(text),
                span_between(
                    rebase_point(base, node.start_position()),
                    rebase_point(base, node.end_position()),
                ),
                &bases,
                true,
            );
        }
    });
}

fn css_links(fragment: &CssFragment, bases: &[String], links: &mut Vec<ExternalLink>) {
    let decode = |text: &str| css_unescape(text);
    let mut cursor = fragment.tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| match node.kind() {
        "call_expression" => {
            let is_url = child_of_kind(node, "function_name")
                .and_then(|function| fragment.node_text(function))
                .is_some_and(|function| function.eq_ignore_ascii_case("url"));
            if !is_url {
                return;
            }
            if let Some(arguments) = child_of_kind(node, "arguments")
                && let Some((start, end)) = url_argument_range(fragment, arguments)
                && let Some(text) = fragment.text.get(start..end)
                && let Some((_, _, span)) = fragment.source_range(start, end)
            {
                push_link(links, &decode(text), span, bases, true);
            }
        }
        "string_value"
            if node
                .parent()
                .is_some_and(|p| p.kind() == "import_statement") =>
        {
            let Some(text) = fragment.node_text(node) else {
                return;
            };
            let inner_start = node.start_byte() + 1;
            let inner_end = node.end_byte().saturating_sub(1).max(inner_start);
            if text.len() >= 2
                && let Some(inner) = fragment.text.get(inner_start..inner_end)
                && let Some((_, _, span)) = fragment.source_range(inner_start, inner_end)
            {
                push_link(links, &decode(inner), span, bases, true);
            }
        }
        _ => {}
    });
}

fn stylesheet_pi_link(node: tree_sitter::Node<'_>, source: &[u8], links: &mut Vec<ExternalLink>) {
    let is_stylesheet = node
        .child_by_field_name("target")
        .and_then(|target| target.utf8_text(source).ok())
        == Some("xml-stylesheet");
    if !is_stylesheet {
        return;
    }
    let Some(content) = node.child_by_field_name("content") else {
        return;
    };
    let Ok(text) = content.utf8_text(source) else {
        return;
    };
    let Some((start, end)) = pseudo_attribute_range(text, "href") else {
        return;
    };
    let Some(value) = text.get(start..end) else {
        return;
    };
    let base = content.start_position();
    let start_point = advance_point(base, &text.as_bytes()[..start]);
    let end_point = advance_point(start_point, value.as_bytes());
    push_link(
        links,
        &xml_unescape(value),
        span_between(start_point, end_point),
        &[],
        true,
    );
}

fn pseudo_attribute_range(content: &str, wanted: &str) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let name_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        let name = content.get(name_start..index)?;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index) != Some(&b'=') {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let quote = *bytes.get(index)?;
        if quote != b'"' && quote != b'\'' {
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            continue;
        }
        let value_start = index + 1;
        let value_end = value_start + bytes.get(value_start..)?.iter().position(|&b| b == quote)?;
        if name == wanted {
            return Some((value_start, value_end));
        }
        index = value_end + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use tree_sitter::Parser;

    use super::*;

    fn parse_svg(source: &str) -> Result<Tree, Box<dyn Error>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
        parser
            .parse(source, None)
            .ok_or_else(|| "parse returned None".into())
    }

    fn span_text<'a>(source: &'a str, span: &Span) -> &'a str {
        let lines: Vec<&str> = source.split('\n').collect();
        &lines[span.start_row][span.start_col..span.end_col]
    }

    #[test]
    fn skips_empty_and_unbalanced_css_urls() -> Result<(), Box<dyn Error>> {
        let source = "<svg><style>.x { background: url(); mask: url(\"#bad'); }</style></svg>";
        let tree = parse_svg(source)?;
        assert_eq!(external_links(source.as_bytes(), &tree), Vec::new());
        let targets: Vec<_> = crate::collect_symbol_occurrences(source.as_bytes(), &tree)
            .into_iter()
            .map(|occurrence| occurrence.target)
            .collect();
        assert_eq!(targets, vec![crate::DefinitionTarget::Class("x".into())]);
        Ok(())
    }

    #[test]
    fn pseudo_attribute_parser_skips_bare_names_and_unquoted_values() {
        let content = "alternate type=css href=\"a.css\"";
        assert_eq!(pseudo_attribute_range(content, "href"), Some((25, 30)));
        assert_eq!(pseudo_attribute_range("href=a.css", "href"), None);
        assert_eq!(pseudo_attribute_range("href", "href"), None);
    }

    #[test]
    fn collects_external_references_and_skips_fragments() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<?xml-stylesheet type=\"text/css\" href=\"theme.css\"?>\n",
            "<svg xmlns=\"http://www.w3.org/2000/svg\">\n",
            "  <style>@import url(\"a.css\"); @import 'b.css'; .x { fill: url(#g); background: \
             url(img.png); }</style>\n",
            "  <use href=\"#g\"/><use href=\"sprites.svg#icon\"/>\n",
            "  <image href=\"https://example.com/a.png?x=1&amp;y=2\"/>\n",
            "  <image href=\"data:image/png;base64,AAAA\"/>\n",
            "  <rect fill=\"url(paint.svg#p) red\" clip-path=\"url(clip.svg#c)\" style=\"fill: \
             url(s.svg#s)\"/>\n",
            "</svg>",
        );
        let tree = parse_svg(source)?;
        let links = external_links(source.as_bytes(), &tree);
        let targets: Vec<_> = links.iter().map(|link| link.target.as_str()).collect();
        assert_eq!(
            targets,
            vec![
                "theme.css",
                "a.css",
                "b.css",
                "img.png",
                "sprites.svg#icon",
                "https://example.com/a.png?x=1&y=2",
                "paint.svg#p",
                "clip.svg#c",
                "s.svg#s",
            ]
        );
        let spans: Vec<_> = links
            .iter()
            .map(|link| span_text(source, &link.span))
            .collect();
        assert_eq!(spans[0], "theme.css");
        assert_eq!(spans[1], "a.css");
        assert_eq!(spans[3], "img.png");
        assert_eq!(spans[5], "https://example.com/a.png?x=1&amp;y=2");
        assert_eq!(spans[8], "s.svg#s");
        assert!(links.iter().all(|link| link.base.is_empty()));
        Ok(())
    }

    #[test]
    fn fragment_hrefs_under_a_foreign_xml_base_are_external() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><g id=\"g\"/><use xml:base=\"other.svg\" href=\"#g\"/>",
            "<use xml:base=\"\" href=\"#g\"/><rect xml:base=\"other.svg\" fill=\"url(#g)\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let links: Vec<_> = external_links(source.as_bytes(), &tree)
            .into_iter()
            .map(|link| (link.target, link.base))
            .collect();
        assert_eq!(links, vec![("#g".to_owned(), vec!["other.svg".to_owned()])]);
        let references = crate::collect_symbol_occurrences(source.as_bytes(), &tree)
            .into_iter()
            .filter(|occurrence| occurrence.role == crate::OccurrenceRole::Reference)
            .count();
        assert_eq!(references, 2);
        let offset = source.find("\"#g").ok_or("href")? + 2;
        assert_eq!(
            crate::definition_target_at(source.as_bytes(), &tree, offset),
            None
        );
        Ok(())
    }

    #[test]
    fn records_xml_base_in_scope_outermost_first() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<?xml-stylesheet href=\"theme.css\"?>",
            "<svg xml:base=\"assets/\"><g xml:base=\"icons/\">",
            "<image href=\"a.png\" xml:base=\"x&amp;y/\"/><rect style=\"fill: url(p.svg#p)\"/>",
            "<style>.x { background: url(b.png) }</style></g><use href=\"u.svg#u\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let links: Vec<_> = external_links(source.as_bytes(), &tree)
            .into_iter()
            .map(|link| (link.target, link.base))
            .collect();
        let bases = |list: &[&str]| {
            list.iter()
                .map(|base| (*base).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            links,
            vec![
                ("theme.css".to_owned(), Vec::new()),
                ("a.png".to_owned(), bases(&["assets/", "icons/", "x&y/"])),
                ("p.svg#p".to_owned(), bases(&["assets/", "icons/"])),
                ("b.png".to_owned(), bases(&["assets/", "icons/"])),
                ("u.svg#u".to_owned(), bases(&["assets/"])),
            ]
        );
        Ok(())
    }
}
