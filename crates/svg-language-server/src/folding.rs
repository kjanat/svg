use tower_lsp_server::ls_types::{FoldingRange, FoldingRangeKind};
use tree_sitter::Tree;

use crate::positions::{byte_col_to_utf16, u32_from_usize};

pub fn folding_ranges(source: &str, tree: &Tree, line_folding_only: bool) -> Vec<FoldingRange> {
    let bytes = source.as_bytes();
    svg_references::folding_regions(bytes, tree)
        .into_iter()
        .map(|region| FoldingRange {
            start_line: u32_from_usize(region.start_row),
            start_character: if line_folding_only {
                None
            } else {
                region.start_col.and_then(|col| {
                    bytes
                        .get(region.start_row_byte..)
                        .map(|line| byte_col_to_utf16(line, 0, col))
                })
            },
            end_line: u32_from_usize(region.end_row),
            end_character: None,
            kind: Some(match region.kind {
                svg_references::FoldKind::Comment => FoldingRangeKind::Comment,
                svg_references::FoldKind::Region => FoldingRangeKind::Region,
            }),
            collapsed_text: None,
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
    fn line_folding_only_omits_characters() {
        let source = "<svg>\n  <!--\u{e9}--><g>\n    <rect/>\n  </g>\n  <!--\n  -->\n</svg>";
        let tree = parse(source);
        let lines: Vec<_> = folding_ranges(source, &tree, true)
            .into_iter()
            .map(|range| {
                (
                    range.start_line,
                    range.end_line,
                    range.start_character,
                    range.kind,
                )
            })
            .collect();
        assert_eq!(
            lines,
            vec![
                (0, 5, None, Some(FoldingRangeKind::Region)),
                (1, 2, None, Some(FoldingRangeKind::Region)),
                (4, 5, None, Some(FoldingRangeKind::Comment)),
            ]
        );
        let characters: Vec<_> = folding_ranges(source, &tree, false)
            .into_iter()
            .map(|range| range.start_character)
            .collect();
        assert_eq!(characters, vec![Some(5), Some(13), None]);
    }
}
