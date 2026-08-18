use svg_tree::{is_attribute_name_kind, is_attribute_node_kind};
use tree_sitter::Node;

pub const SVG_NAMESPACE_URI: &str = "http://www.w3.org/2000/svg";
pub const XLINK_NAMESPACE_URI: &str = "http://www.w3.org/1999/xlink";

#[derive(Clone, Debug, Default)]
pub struct NamespaceScope<'a> {
    default_namespace: Option<&'a str>,
    prefixes: Vec<(&'a str, &'a str)>,
}

impl<'a> NamespaceScope<'a> {
    #[must_use]
    pub const fn default_namespace(&self) -> Option<&'a str> {
        self.default_namespace
    }

    pub const fn set_default_namespace(&mut self, namespace_uri: Option<&'a str>) {
        self.default_namespace = namespace_uri;
    }

    #[must_use]
    pub fn resolve_prefix(&self, prefix: &str) -> Option<&'a str> {
        self.prefixes
            .iter()
            .rev()
            .find_map(|(known_prefix, namespace_uri)| {
                (*known_prefix == prefix).then_some(*namespace_uri)
            })
            .filter(|namespace_uri| !namespace_uri.is_empty())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpandedName<'a> {
    pub namespace_uri: Option<&'a str>,
    pub local_name: &'a str,
}

#[must_use]
pub fn scope_for_tag<'a>(
    source: &'a [u8],
    tag: Node,
    parent: &NamespaceScope<'a>,
) -> NamespaceScope<'a> {
    let mut scope = parent.clone();
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if !is_attribute_node_kind(attr_node.kind()) {
            continue;
        }
        let Some(name_node) = find_attr_name(attr_node) else {
            continue;
        };
        let Ok(attr_name) = name_node.utf8_text(source) else {
            continue;
        };
        let Some(namespace_uri) = attr_value(attr_node, source) else {
            continue;
        };

        if attr_name == "xmlns" {
            scope.set_default_namespace(non_empty_namespace(namespace_uri));
            continue;
        }

        let Some(prefix) = attr_name.strip_prefix("xmlns:") else {
            continue;
        };
        scope.prefixes.push((prefix, namespace_uri));
    }
    scope
}

#[must_use]
pub fn declares_default_namespace(source: &[u8], tag: Node) -> bool {
    let mut cursor = tag.walk();
    for attr_node in tag.children(&mut cursor) {
        if !is_attribute_node_kind(attr_node.kind()) {
            continue;
        }
        let Some(name_node) = find_attr_name(attr_node) else {
            continue;
        };
        if name_node.utf8_text(source).ok() == Some("xmlns") {
            return true;
        }
    }
    false
}

#[must_use]
pub fn expand_element_name<'a>(raw_name: &'a str, scope: &NamespaceScope<'a>) -> ExpandedName<'a> {
    let (prefix, local_name) = split_qualified_name(raw_name);
    ExpandedName {
        namespace_uri: prefix.map_or_else(
            || scope.default_namespace(),
            |qualified_prefix| scope.resolve_prefix(qualified_prefix),
        ),
        local_name,
    }
}

#[must_use]
pub fn expand_attribute_name<'a>(
    raw_name: &'a str,
    scope: &NamespaceScope<'a>,
) -> ExpandedName<'a> {
    let (prefix, local_name) = split_qualified_name(raw_name);
    ExpandedName {
        namespace_uri: match prefix {
            // Real SVGs often omit xmlns:xlink while still using xlink:href.
            // Keep linting lenient instead of treating that as an unknown
            // foreign namespace.
            Some("xlink") => scope.resolve_prefix("xlink").or(Some(XLINK_NAMESPACE_URI)),
            Some(other_prefix) => scope.resolve_prefix(other_prefix),
            None => None,
        },
        local_name,
    }
}

#[must_use]
pub fn split_qualified_name(raw_name: &str) -> (Option<&str>, &str) {
    match raw_name.split_once(':') {
        Some((prefix, local_name)) => (Some(prefix), local_name),
        None => (None, raw_name),
    }
}

fn find_attr_name(attr_node: Node) -> Option<Node> {
    let mut cursor = attr_node.walk();
    for child in attr_node.children(&mut cursor) {
        if is_attribute_name_kind(child.kind()) {
            return Some(child);
        }
        let mut inner_cursor = child.walk();
        for grandchild in child.children(&mut inner_cursor) {
            if is_attribute_name_kind(grandchild.kind()) {
                return Some(grandchild);
            }
        }
    }
    None
}

fn attr_value<'a>(attr_node: Node, source: &'a [u8]) -> Option<&'a str> {
    if let Some(value_node) = find_attr_value(attr_node) {
        let raw_value = value_node.utf8_text(source).ok()?;
        return Some(trim_attr_value(raw_value));
    }

    let raw_attr = attr_node.utf8_text(source).ok()?;
    let (_, raw_value) = raw_attr.split_once('=')?;
    Some(trim_attr_value(raw_value))
}

fn find_attr_value(attr_node: Node) -> Option<Node> {
    attr_node.child_by_field_name("value").or_else(|| {
        let mut cursor = attr_node.walk();
        attr_node
            .children(&mut cursor)
            .find_map(|child| child.child_by_field_name("value"))
    })
}

fn trim_attr_value(raw_value: &str) -> &str {
    raw_value.trim_matches(|ch: char| ch == '"' || ch == '\'' || ch.is_ascii_whitespace())
}

fn non_empty_namespace(namespace_uri: &str) -> Option<&str> {
    (!namespace_uri.is_empty()).then_some(namespace_uri)
}

/// Whether the element owning `name_node` (a tag `name` node) resolves to the
/// SVG namespace, walking the ancestor chain to honour prefix bindings and
/// default-namespace overrides declared anywhere above it.
#[must_use]
pub fn resolves_to_svg_namespace(source: &[u8], name_node: Node) -> bool {
    let Some(tag) = name_node
        .parent()
        .filter(|parent| matches!(parent.kind(), "start_tag" | "self_closing_tag" | "end_tag"))
    else {
        return false;
    };

    let mut chain = vec![tag];
    let mut current = tag;
    while let Some(parent) = current.parent() {
        let mut cursor = parent.walk();
        if let Some(open) = parent
            .children(&mut cursor)
            .find(|child| matches!(child.kind(), "start_tag" | "self_closing_tag"))
            && open.id() != current.id()
        {
            chain.push(open);
        }
        current = parent;
    }
    chain.reverse();

    let mut scope = NamespaceScope::default();
    let last = chain.len() - 1;
    for (index, link) in chain.iter().enumerate() {
        scope = scope_for_tag(source, *link, &scope);

        if index == 0
            && scope.default_namespace().is_none()
            && !declares_default_namespace(source, *link)
        {
            let roots_svg = link
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
                .is_some_and(|raw| {
                    let (prefix, local) = split_qualified_name(raw);
                    prefix.is_none() && local == "svg"
                });
            if roots_svg {
                scope.set_default_namespace(Some(SVG_NAMESPACE_URI));
            }
        }

        if index < last
            && let Some(raw) = link
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
        {
            let host = expand_element_name(raw, &scope);
            if host.namespace_uri == Some(SVG_NAMESPACE_URI)
                && svg_data::allows_foreign_children(host.local_name)
            {
                scope.set_default_namespace(None);
            }
        }
    }

    let Ok(raw_name) = name_node.utf8_text(source) else {
        return false;
    };

    expand_element_name(raw_name, &scope).namespace_uri == Some(SVG_NAMESPACE_URI)
}

#[cfg(test)]
mod tests {

    #[test]
    fn unbound_svg_root_resolves_itself_and_its_children_as_svg() -> Result<(), Box<dyn Error>> {
        let source = "<svg><rect/></svg>";
        let tree = parse_svg(source)?;
        let names = tag_names(tree.root_node(), source.as_bytes());

        assert_eq!(
            names,
            vec![("svg".to_owned(), true), ("rect".to_owned(), true)]
        );
        Ok(())
    }

    #[test]
    fn a_foreign_default_namespace_wins_over_the_svg_name() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg xmlns="http://www.w3.org/1999/xhtml"><svg/></svg>"#;
        let tree = parse_svg(source)?;
        let names = tag_names(tree.root_node(), source.as_bytes());

        assert_eq!(
            names,
            vec![("svg".to_owned(), false), ("svg".to_owned(), false)]
        );
        Ok(())
    }

    #[test]
    fn a_prefixed_foreign_element_is_not_svg() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg xmlns:html="http://www.w3.org/1999/xhtml"><html:title/></svg>"#;
        let tree = parse_svg(source)?;
        let names = tag_names(tree.root_node(), source.as_bytes());

        assert_eq!(
            names,
            vec![("svg".to_owned(), true), ("html:title".to_owned(), false)]
        );
        Ok(())
    }

    #[test]
    fn foreign_content_children_do_not_resolve_to_svg() -> Result<(), Box<dyn Error>> {
        let source = "<svg><foreignObject><div/></foreignObject></svg>";
        let tree = parse_svg(source)?;
        let names = tag_names(tree.root_node(), source.as_bytes());

        assert_eq!(
            names,
            vec![
                ("svg".to_owned(), true),
                ("foreignObject".to_owned(), true),
                ("div".to_owned(), false)
            ]
        );
        Ok(())
    }

    #[test]
    fn svg_redeclared_inside_foreign_content_resolves_again() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><foreignObject><svg xmlns="http://www.w3.org/2000/svg"><rect/></svg></foreignObject></svg>"#;
        let tree = parse_svg(source)?;
        let names = tag_names(tree.root_node(), source.as_bytes());

        assert_eq!(
            names,
            vec![
                ("svg".to_owned(), true),
                ("foreignObject".to_owned(), true),
                ("svg".to_owned(), true),
                ("rect".to_owned(), true)
            ]
        );
        Ok(())
    }

    #[test]
    fn an_empty_prefix_declaration_undeclares_the_inherited_binding() -> Result<(), Box<dyn Error>>
    {
        let source =
            r#"<svg xmlns:p="http://www.w3.org/2000/svg"><g xmlns:p=""><p:rect/></g></svg>"#;
        let tree = parse_svg(source)?;
        let names = tag_names(tree.root_node(), source.as_bytes());

        assert_eq!(
            names,
            vec![
                ("svg".to_owned(), true),
                ("g".to_owned(), true),
                ("p:rect".to_owned(), false)
            ]
        );
        Ok(())
    }

    fn tag_names(node: Node<'_>, source: &[u8]) -> Vec<(String, bool)> {
        let mut found = Vec::new();
        collect_tag_names(node, source, &mut found);
        found
    }

    fn collect_tag_names(node: Node<'_>, source: &[u8], out: &mut Vec<(String, bool)>) {
        if node.kind() == "name"
            && node
                .parent()
                .is_some_and(|parent| matches!(parent.kind(), "start_tag" | "self_closing_tag"))
            && let Ok(text) = node.utf8_text(source)
        {
            out.push((text.to_owned(), resolves_to_svg_namespace(source, node)));
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_tag_names(child, source, out);
        }
    }

    use std::error::Error;

    use tree_sitter::{Node, Parser, Tree};

    use super::*;

    fn parse_svg(source: &str) -> Result<Tree, Box<dyn Error>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_svg::LANGUAGE.into())
            .map_err(|err| format!("SVG grammar: {err}"))?;
        parser
            .parse(source, None)
            .ok_or_else(|| "parse returned None".into())
    }

    fn first_tag(node: Node<'_>) -> Option<Node<'_>> {
        if node.kind() == "start_tag" || node.kind() == "self_closing_tag" {
            return Some(node);
        }

        let mut cursor = node.walk();
        node.children(&mut cursor).find_map(first_tag)
    }

    #[test]
    fn scope_for_tag_reads_namespace_values() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"/>"#;
        let tree = parse_svg(source)?;
        let tag = first_tag(tree.root_node()).ok_or("missing tag")?;
        let scope = scope_for_tag(source.as_bytes(), tag, &NamespaceScope::default());

        assert_eq!(scope.default_namespace(), Some(SVG_NAMESPACE_URI));
        assert_eq!(scope.resolve_prefix("xlink"), Some(XLINK_NAMESPACE_URI));
        Ok(())
    }

    #[test]
    fn prefixed_element_without_binding_does_not_inherit_default_namespace() {
        let scope = NamespaceScope {
            default_namespace: Some(SVG_NAMESPACE_URI),
            prefixes: Vec::new(),
        };

        assert_eq!(
            expand_element_name("sodipodi:namedview", &scope),
            ExpandedName {
                namespace_uri: None,
                local_name: "namedview",
            }
        );
    }
}
