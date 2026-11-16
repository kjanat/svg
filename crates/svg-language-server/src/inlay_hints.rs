use tower_lsp_server::ls_types::{InlayHint, InlayHintKind, InlayHintLabel, Range};
use tree_sitter::Tree;

use crate::{
    positions::{LineIndex, byte_offset_for_position},
    references::span_range,
};

pub fn inlay_hints(source: &str, tree: &Tree, range: Range) -> Vec<InlayHint> {
    let bytes = source.as_bytes();
    let start = byte_offset_for_position(bytes, range.start);
    let end = byte_offset_for_position(bytes, range.end);
    let index = LineIndex::new(bytes);
    svg_references::resolve_id_references(bytes, tree)
        .into_iter()
        .filter(|target| (start..=end).contains(&target.reference.end_byte))
        .map(|target| InlayHint {
            position: span_range(&index, bytes, &target.reference.span).end,
            label: InlayHintLabel::String(format!(": {}", target.element_name)),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use tower_lsp_server::ls_types::Position;

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

    fn label(hint: &InlayHint) -> &str {
        match &hint.label {
            InlayHintLabel::String(text) => text,
            InlayHintLabel::LabelParts(_) => "",
        }
    }

    #[test]
    fn hints_resolved_element_kind_within_range() {
        let source = "<svg>\n<linearGradient id=\"g\"/>\n<rect fill=\"url(#g)\"/>\n<use \
                      href=\"#g\"/><use href=\"#nope\"/>\n</svg>";
        let tree = parse(source);
        let everything = Range::new(Position::new(0, 0), Position::new(5, 0));
        let hints = inlay_hints(source, &tree, everything);
        let summary: Vec<_> = hints
            .iter()
            .map(|hint| (hint.position, label(hint)))
            .collect();
        assert_eq!(
            summary,
            vec![
                (Position::new(2, 18), ": linearGradient"),
                (Position::new(3, 13), ": linearGradient"),
            ]
        );
        assert!(
            hints
                .iter()
                .all(|hint| hint.kind == Some(InlayHintKind::TYPE))
        );
        let only_line_three = Range::new(Position::new(3, 0), Position::new(4, 0));
        assert_eq!(inlay_hints(source, &tree, only_line_three).len(), 1);
    }
}
