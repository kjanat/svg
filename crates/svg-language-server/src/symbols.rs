use tower_lsp_server::ls_types::{DocumentSymbol, SymbolInformation, SymbolKind, Uri};
use tree_sitter::Tree;

use crate::{positions::LineIndex, references::span_range};

pub fn document_symbols(source: &str, tree: &Tree) -> Vec<DocumentSymbol> {
    let Some(template) = symbol_template() else {
        return Vec::new();
    };
    let bytes = source.as_bytes();
    let index = LineIndex::new(bytes);
    let outline = svg_references::element_outline(bytes, tree);
    let mut children: Vec<Vec<DocumentSymbol>> = vec![Vec::new(); outline.len()];
    let mut roots = Vec::new();
    for (position, element) in outline.iter().enumerate().rev() {
        let mut own_children = std::mem::take(&mut children[position]);
        own_children.reverse();
        let symbol = element_symbol(&index, bytes, element, own_children, &template);
        match element.parent {
            Some(parent) => children[parent].push(symbol),
            None => roots.push(symbol),
        }
    }
    roots.reverse();
    roots
}

pub fn flat_symbols(symbols: Vec<DocumentSymbol>, uri: &Uri) -> Vec<SymbolInformation> {
    let mut flat = Vec::new();
    flatten(symbols, None, uri, &mut flat);
    flat
}

fn flatten(
    symbols: Vec<DocumentSymbol>,
    container: Option<&str>,
    uri: &Uri,
    flat: &mut Vec<SymbolInformation>,
) {
    for symbol in symbols {
        let information = serde_json::from_value(serde_json::json!({
            "name": symbol.name,
            "kind": symbol.kind,
            "location": { "uri": uri, "range": symbol.range },
            "containerName": container,
        }));
        if let Ok(information) = information {
            flat.push(information);
        }
        if let Some(children) = symbol.children {
            flatten(children, Some(&symbol.name), uri, flat);
        }
    }
}

fn symbol_template() -> Option<DocumentSymbol> {
    let origin = serde_json::json!({ "line": 0, "character": 0 });
    let range = serde_json::json!({ "start": origin, "end": origin });
    serde_json::from_value(serde_json::json!({
        "name": "",
        "kind": SymbolKind::FIELD,
        "range": range,
        "selectionRange": range,
    }))
    .ok()
}

fn element_symbol(
    index: &LineIndex,
    source: &[u8],
    element: &svg_references::OutlineElement,
    children: Vec<DocumentSymbol>,
    template: &DocumentSymbol,
) -> DocumentSymbol {
    let mut detail = String::new();
    if let Some(id) = &element.id {
        detail.push('#');
        detail.push_str(id);
    }
    for class in &element.classes {
        detail.push('.');
        detail.push_str(class);
    }
    let local_name = element
        .name
        .split_once(':')
        .map_or(element.name.as_str(), |(_, local)| local);
    DocumentSymbol {
        name: element.name.clone(),
        detail: (!detail.is_empty()).then_some(detail),
        kind: symbol_kind(local_name),
        tags: None,
        range: span_range(index, source, &element.span),
        selection_range: span_range(index, source, &element.name_span),
        children: (!children.is_empty()).then_some(children),
        ..template.clone()
    }
}

fn symbol_kind(local_name: &str) -> SymbolKind {
    match local_name {
        "svg" => SymbolKind::NAMESPACE,
        "g" | "defs" | "a" | "switch" => SymbolKind::STRUCT,
        "symbol" | "marker" | "mask" | "clipPath" | "pattern" | "linearGradient"
        | "radialGradient" | "filter" => SymbolKind::CLASS,
        "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path" | "image"
        | "use" | "foreignObject" | "metadata" => SymbolKind::OBJECT,
        "text" | "tspan" | "textPath" | "title" | "desc" => SymbolKind::STRING,
        "stop" => SymbolKind::CONSTANT,
        "animate" | "animateMotion" | "animateTransform" | "set" | "mpath" | "discard" => {
            SymbolKind::EVENT
        }
        "style" => SymbolKind::PROPERTY,
        "script" => SymbolKind::FUNCTION,
        name if name.starts_with("fe") => SymbolKind::OPERATOR,
        _ => SymbolKind::FIELD,
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp_server::ls_types::{Position, Range};

    use super::*;

    fn parse(source: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        assert!(
            parser
                .set_language(&tree_sitter_svg::LANGUAGE.into())
                .is_ok()
        );
        parser
            .parse(source, None)
            .unwrap_or_else(|| unreachable!("parser has a language"))
    }

    #[test]
    fn nests_elements_with_details_and_kinds() {
        let source = "<svg>\n  <defs><linearGradient id=\"g\"/></defs>\n  <g class=\"a \
                      b\"><rect/></g>\n  <style>.a{}</style>\n</svg>";
        let symbols = document_symbols(source, &parse(source));
        assert_eq!(symbols.len(), 1);
        let root = &symbols[0];
        assert_eq!(root.name, "svg");
        assert_eq!(root.kind, SymbolKind::NAMESPACE);
        assert_eq!(
            root.range,
            Range::new(Position::new(0, 0), Position::new(4, 6))
        );
        assert_eq!(
            root.selection_range,
            Range::new(Position::new(0, 1), Position::new(0, 4))
        );
        let children = root.children.as_deref().unwrap_or_default();
        let names: Vec<_> = children.iter().map(|child| child.name.as_str()).collect();
        assert_eq!(names, vec!["defs", "g", "style"]);
        assert_eq!(children[0].kind, SymbolKind::STRUCT);
        let gradient = &children[0].children.as_deref().unwrap_or_default()[0];
        assert_eq!(gradient.detail.as_deref(), Some("#g"));
        assert_eq!(gradient.kind, SymbolKind::CLASS);
        assert_eq!(children[1].detail.as_deref(), Some(".a.b"));
        assert_eq!(children[2].kind, SymbolKind::PROPERTY);
        assert!(children[2].children.is_none());
    }

    #[test]
    fn flattens_with_locations_and_containers() -> Result<(), Box<dyn std::error::Error>> {
        let source = "<svg><g><rect/></g><circle/></svg>";
        let uri: Uri = "file:///flat.svg".parse()?;
        let flat = flat_symbols(document_symbols(source, &parse(source)), &uri);
        let rows: Vec<_> = flat
            .iter()
            .map(|symbol| {
                (
                    symbol.name.as_str(),
                    symbol.container_name.as_deref(),
                    symbol.location.range.start.character,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("svg", None, 0),
                ("g", Some("svg"), 5),
                ("rect", Some("g"), 8),
                ("circle", Some("svg"), 19),
            ]
        );
        assert!(flat.iter().all(|symbol| symbol.location.uri == uri));
        Ok(())
    }

    #[test]
    fn ranges_use_utf16_columns() {
        let source = "<svg><text>\u{1F600}</text><rect/></svg>";
        let symbols = document_symbols(source, &parse(source));
        let children = symbols[0].children.as_deref().unwrap_or_default();
        assert_eq!(children[1].selection_range.start, Position::new(0, 21));
    }
}
