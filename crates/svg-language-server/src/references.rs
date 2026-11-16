use tower_lsp_server::ls_types::{Position, Range};
use tree_sitter::Tree;

use crate::positions::{LineIndex, byte_offset_for_position, u32_from_usize};

pub fn span_range(index: &LineIndex, source: &[u8], span: &svg_references::Span) -> Range {
    Range::new(
        Position::new(
            u32_from_usize(span.start_row),
            index.utf16_col(source, span.start_row, span.start_col),
        ),
        Position::new(
            u32_from_usize(span.end_row),
            index.utf16_col(source, span.end_row, span.end_col),
        ),
    )
}

pub fn references(
    source: &str,
    tree: &Tree,
    position: Position,
    include_declaration: bool,
) -> Vec<Range> {
    let bytes = source.as_bytes();
    let occurrences = svg_references::collect_symbol_occurrences(bytes, tree);
    let offset = byte_offset_for_position(bytes, position);
    let Some(target) = svg_references::symbol_occurrence_at(&occurrences, offset)
        .map(|occurrence| occurrence.target.clone())
    else {
        return Vec::new();
    };
    let index = LineIndex::new(bytes);
    occurrences
        .iter()
        .filter(|occurrence| occurrence.target == target)
        .filter(|occurrence| {
            include_declaration || occurrence.role != svg_references::OccurrenceRole::Declaration
        })
        .map(|occurrence| span_range(&index, bytes, &occurrence.span))
        .collect()
}

#[cfg(test)]
mod tests {
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

    fn range(line: u32, start: u32, end: u32) -> Range {
        Range::new(Position::new(line, start), Position::new(line, end))
    }

    #[test]
    fn finds_id_references_with_and_without_declaration() {
        let source =
            "<svg>\n<linearGradient id=\"g\"/>\n<rect fill=\"url(#g)\"/><use href=\"#g\"/>\n</svg>";
        let tree = parse(source);
        let at_reference = Position::new(2, 17);
        assert_eq!(
            references(source, &tree, at_reference, true),
            vec![range(1, 20, 21), range(2, 17, 18), range(2, 34, 35)]
        );
        assert_eq!(
            references(source, &tree, at_reference, false),
            vec![range(2, 17, 18), range(2, 34, 35)]
        );
    }

    #[test]
    fn finds_class_occurrences_after_non_ascii_text() {
        let source = "<svg><text>\u{e9}</text><style>.k{}</style><g class=\"k\"/></svg>";
        let tree = parse(source);
        let found = references(source, &tree, Position::new(0, 48), true);
        assert_eq!(found, vec![range(0, 27, 28), range(0, 48, 49)]);
    }

    #[test]
    fn returns_nothing_outside_symbols() {
        let source = "<svg><rect width=\"1\"/></svg>";
        assert_eq!(
            references(source, &parse(source), Position::new(0, 18), true),
            Vec::<Range>::new()
        );
    }
}
