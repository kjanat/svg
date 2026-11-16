use tower_lsp_server::ls_types::{Position, Range, SelectionRange};
use tree_sitter::Tree;

use crate::positions::{byte_offset_for_position, position_for_byte_offset};

pub fn selection_ranges(source: &str, tree: &Tree, positions: &[Position]) -> Vec<SelectionRange> {
    let bytes = source.as_bytes();
    positions
        .iter()
        .map(|&position| {
            let offset = byte_offset_for_position(bytes, position);
            let chain = svg_references::selection_chain(bytes, tree, offset);
            let mut selection: Option<SelectionRange> = None;
            for range in chain.iter().rev() {
                selection = Some(SelectionRange {
                    range: Range::new(
                        position_for_byte_offset(bytes, range.start),
                        position_for_byte_offset(bytes, range.end),
                    ),
                    parent: selection.map(Box::new),
                });
            }
            selection.unwrap_or_else(|| SelectionRange {
                range: Range::new(position, position),
                parent: None,
            })
        })
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

    #[test]
    fn chains_one_selection_per_position_innermost_first() {
        let source = "<svg>\n  <!--\u{e9}--><rect id=\"ab\"/>\n</svg>";
        let tree = parse(source);
        let positions = [Position::new(1, 21), Position::new(0, 2)];
        let selections = selection_ranges(source, &tree, &positions);
        assert_eq!(selections.len(), 2);
        let mut ranges = Vec::new();
        let mut current = Some(&selections[0]);
        while let Some(selection) = current {
            ranges.push(selection.range);
            current = selection.parent.as_deref();
        }
        assert_eq!(
            ranges.first(),
            Some(&Range::new(Position::new(1, 20), Position::new(1, 22)))
        );
        assert!(ranges.contains(&Range::new(Position::new(1, 10), Position::new(1, 25))));
        assert_eq!(
            ranges.last(),
            Some(&Range::new(Position::new(0, 0), Position::new(2, 6)))
        );
    }
}
