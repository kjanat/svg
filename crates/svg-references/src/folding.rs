use svg_tree::{child_of_kind, walk_tree};
use tree_sitter::{Point, Tree};

use crate::{inline_style_fragments, outline::element_tag};

/// What a fold hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldKind {
    /// An XML comment.
    Comment,
    /// An element, CDATA section, or CSS block.
    Region,
}

/// A foldable line range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldRegion {
    /// First line, which stays visible.
    pub start_row: usize,
    /// Byte offset where `start_row` begins.
    pub start_row_byte: usize,
    /// Byte column on `start_row` where the folded text begins, when it differs from the line
    /// end.
    pub start_col: Option<usize>,
    /// Last hidden line.
    pub end_row: usize,
    /// What the fold hides.
    pub kind: FoldKind,
}

/// Collect foldable regions: multi-line elements (closing tag left visible), comments, CDATA
/// sections, and CSS blocks in `<style>` elements, ordered by start line with one region per
/// start line.
#[must_use]
pub fn folding_regions(source: &[u8], tree: &Tree) -> Vec<FoldRegion> {
    let mut regions = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if let Some(tag) = element_tag(node) {
            let start = node.start_position();
            if let Some(end_tag) = child_of_kind(node, "end_tag") {
                let close_row = end_tag.start_position().row;
                let start_col =
                    (tag.end_position().row == start.row).then_some(tag.end_position().column);
                push(
                    &mut regions,
                    node.start_byte() - start.column,
                    start,
                    close_row.checked_sub(1),
                    start_col,
                    FoldKind::Region,
                );
            } else {
                push(
                    &mut regions,
                    node.start_byte() - start.column,
                    start,
                    Some(node.end_position().row),
                    None,
                    FoldKind::Region,
                );
            }
            return;
        }
        let end_row = node.end_position().row;
        let (kind, end_row) = match node.kind() {
            "comment" => (FoldKind::Comment, Some(end_row)),
            "cdata_section" => (FoldKind::Region, end_row.checked_sub(1)),
            _ => return,
        };
        let start = node.start_position();
        push(
            &mut regions,
            node.start_byte() - start.column,
            start,
            end_row,
            None,
            kind,
        );
    });

    for (_, fragment) in inline_style_fragments(source, tree) {
        let mut cursor = fragment.tree.root_node().walk();
        walk_tree(&mut cursor, &mut |node| {
            if node.kind() != "block" {
                return;
            }
            let owner = node.parent().unwrap_or(node);
            let (Some(start), Some(close), Some(start_byte)) = (
                fragment.source_point(owner.start_byte(), owner.start_position()),
                fragment.source_point(node.end_byte(), node.end_position()),
                fragment.source_byte(owner.start_byte()),
            ) else {
                return;
            };
            push(
                &mut regions,
                start_byte - start.column,
                start,
                close.row.checked_sub(1),
                None,
                FoldKind::Region,
            );
        });
    }

    regions.sort_by(|a, b| {
        a.start_row
            .cmp(&b.start_row)
            .then(b.end_row.cmp(&a.end_row))
    });
    regions.dedup_by_key(|region| region.start_row);
    regions
}

fn push(
    regions: &mut Vec<FoldRegion>,
    start_row_byte: usize,
    start: Point,
    end_row: Option<usize>,
    start_col: Option<usize>,
    kind: FoldKind,
) {
    let Some(end_row) = end_row else {
        return;
    };
    if end_row <= start.row {
        return;
    }
    regions.push(FoldRegion {
        start_row: start.row,
        start_row_byte,
        start_col,
        end_row,
        kind,
    });
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
    fn folds_elements_comments_cdata_and_css_blocks() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg>\n",
            "  <!-- a\n",
            "       b -->\n",
            "  <g>\n",
            "    <rect/>\n",
            "  </g>\n",
            "  <rect\n",
            "    x=\"1\"/>\n",
            "  <style><![CDATA[\n",
            "    .a {\n",
            "      fill: red;\n",
            "    }\n",
            "  ]]></style>\n",
            "  <g><rect/></g>\n",
            "</svg>",
        );
        let tree = parse_svg(source)?;
        let regions = folding_regions(source.as_bytes(), &tree);
        let summary: Vec<_> = regions
            .iter()
            .map(|region| (region.start_row, region.end_row, region.kind))
            .collect();
        assert_eq!(
            summary,
            vec![
                (0, 13, FoldKind::Region),
                (1, 2, FoldKind::Comment),
                (3, 4, FoldKind::Region),
                (6, 7, FoldKind::Region),
                (8, 11, FoldKind::Region),
                (9, 10, FoldKind::Region),
            ]
        );
        assert_eq!(regions[0].start_col, Some(5));
        assert_eq!(regions[2].start_row_byte, source.find("  <g>").ok_or("g")?);
        Ok(())
    }
}
