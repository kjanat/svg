use std::collections::HashMap;

use svg_tree::{child_of_kind, walk_tree};
use tree_sitter::Tree;

use crate::{Span, text::node_span};

/// One element of an SVG document, listed in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineElement {
    /// Index of the enclosing element in the same list.
    pub parent: Option<usize>,
    /// Tag name as written, prefix included.
    pub name: String,
    /// Value of the element's `id` attribute.
    pub id: Option<String>,
    /// Tokens of the element's `class` attribute.
    pub classes: Vec<String>,
    /// The whole element, end tag included.
    pub span: Span,
    /// The tag name in the start tag.
    pub name_span: Span,
}

pub fn element_tag(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let kind = node.kind();
    if kind != "element" && !kind.ends_with("_element") {
        return None;
    }
    child_of_kind(node, "start_tag").or_else(|| child_of_kind(node, "self_closing_tag"))
}

/// List every element in document order, each pointing at its parent.
#[must_use]
pub fn element_outline(source: &[u8], tree: &Tree) -> Vec<OutlineElement> {
    let mut elements = Vec::new();
    let mut indices: HashMap<usize, usize> = HashMap::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        let Some(tag) = element_tag(node) else {
            return;
        };
        let Some(name_node) = tag.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source) else {
            return;
        };
        let mut parent = None;
        let mut ancestor = node.parent();
        while let Some(current) = ancestor {
            if let Some(&index) = indices.get(&current.id()) {
                parent = Some(index);
                break;
            }
            ancestor = current.parent();
        }
        let (id, classes) = identity(tag, source);
        indices.insert(node.id(), elements.len());
        elements.push(OutlineElement {
            parent,
            name: name.to_owned(),
            id,
            classes,
            span: node_span(node),
            name_span: node_span(name_node),
        });
    });
    elements
}

fn identity(tag: tree_sitter::Node<'_>, source: &[u8]) -> (Option<String>, Vec<String>) {
    let mut id = None;
    let mut classes = Vec::new();
    let mut cursor = tag.walk();
    for attribute in tag.named_children(&mut cursor) {
        match attribute.kind() {
            "id_attribute" if id.is_none() => {
                let mut inner = attribute.walk();
                walk_tree(&mut inner, &mut |node| {
                    if node.kind() == "id_token"
                        && let Ok(text) = node.utf8_text(source)
                    {
                        id = Some(crate::text::xml_unescape(text));
                    }
                });
            }
            "class_attribute" => {
                let mut inner = attribute.walk();
                walk_tree(&mut inner, &mut |node| {
                    if node.kind() == "class_name"
                        && let Ok(text) = node.utf8_text(source)
                    {
                        classes.extend(
                            crate::text::xml_tokens(text)
                                .into_iter()
                                .map(|(class, _, _)| class),
                        );
                    }
                });
            }
            _ => {}
        }
    }
    (id, classes)
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

    #[test]
    fn lists_elements_with_parents_and_identity() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg>\n",
            "  <defs><linearGradient id=\"g\"><stop/></linearGradient></defs>\n",
            "  <g class=\"a b\"><rect/></g>\n",
            "  <style>.a{}</style><script>1</script>\n",
            "</svg>",
        );
        let tree = parse_svg(source)?;
        let outline = element_outline(source.as_bytes(), &tree);
        let summary: Vec<_> = outline
            .iter()
            .map(|element| (element.name.as_str(), element.parent))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("svg", None),
                ("defs", Some(0)),
                ("linearGradient", Some(1)),
                ("stop", Some(2)),
                ("g", Some(0)),
                ("rect", Some(4)),
                ("style", Some(0)),
                ("script", Some(0)),
            ]
        );
        assert_eq!(outline[2].id.as_deref(), Some("g"));
        assert_eq!(outline[4].classes, vec!["a", "b"]);
        assert_eq!(
            outline[0].span,
            Span {
                start_row: 0,
                start_col: 0,
                end_row: 4,
                end_col: 6
            }
        );
        assert_eq!(
            outline[1].name_span,
            Span {
                start_row: 1,
                start_col: 3,
                end_row: 1,
                end_col: 7
            }
        );
        Ok(())
    }

    #[test]
    fn classes_split_after_decoding() -> Result<(), Box<dyn Error>> {
        let source = "<svg><g class=\"a&#32;b&amp;c\"/></svg>";
        let outline = element_outline(source.as_bytes(), &parse_svg(source)?);
        assert_eq!(outline[1].classes, vec!["a", "b&c"]);
        Ok(())
    }
}
