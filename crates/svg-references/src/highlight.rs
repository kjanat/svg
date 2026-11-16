use svg_tree::{is_attribute_name_kind, is_attribute_node_kind, walk_tree};
use tree_sitter::Tree;

use crate::{
    DefinitionTarget,
    occurrences::{OccurrenceRole, collect_symbol_occurrences},
};

/// Syntax class of a highlighted token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightKind {
    /// Tag name in a start, end, or self-closing tag.
    ElementName,
    /// Attribute name.
    AttributeName,
    /// Attribute value text not covered by a more specific token.
    AttributeValue,
    /// An element id, declared or referenced.
    Id,
    /// A CSS class, declared or referenced.
    Class,
    /// A CSS custom property, declared or referenced.
    CustomProperty,
    /// An XML comment.
    Comment,
    /// An entity or character reference such as `&amp;`.
    EntityReference,
}

/// A highlighted token confined to one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightToken {
    /// Start byte in the document.
    pub start_byte: usize,
    /// Line of the token.
    pub row: usize,
    /// Byte column of the token start.
    pub start_col: usize,
    /// Length in bytes.
    pub len: usize,
    /// Syntax class.
    pub kind: HighlightKind,
    /// The token declares its symbol (`id="x"`, `.x {}`, `--x:`).
    pub declaration: bool,
}

#[derive(Clone, Copy)]
struct Piece {
    start: usize,
    end: usize,
    kind: HighlightKind,
    declaration: bool,
}

/// Highlight tokens for the whole document in source order, never overlapping, split at line
/// breaks.
///
/// Ids, classes, and custom properties inside attribute values and `<style>` elements take
/// precedence over the surrounding value text. Other `<style>` and `<script>` content is left
/// to the embedded language.
#[must_use]
pub fn highlight_tokens(source: &[u8], tree: &Tree) -> Vec<HighlightToken> {
    let mut bases = Vec::new();
    let mut overlays = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        let kind = node.kind();
        let piece = |kind| Piece {
            start: node.start_byte(),
            end: node.end_byte(),
            kind,
            declaration: false,
        };
        if kind == "name"
            && node.parent().is_some_and(|parent| {
                matches!(parent.kind(), "start_tag" | "end_tag" | "self_closing_tag")
            })
        {
            overlays.push(piece(HighlightKind::ElementName));
        } else if is_attribute_name_kind(kind) {
            overlays.push(piece(HighlightKind::AttributeName));
        } else if kind == "entity_reference" {
            overlays.push(piece(HighlightKind::EntityReference));
        } else if kind == "comment" {
            bases.push(piece(HighlightKind::Comment));
        } else if is_attribute_node_kind(kind)
            && let Some(value) = node.child_by_field_name("value")
        {
            bases.push(Piece {
                start: value.start_byte(),
                end: value.end_byte(),
                kind: HighlightKind::AttributeValue,
                declaration: false,
            });
        }
    });
    let pieces = segment(bases, overlays_with_symbols(source, tree, overlays));
    split_lines(source, pieces)
}

fn overlays_with_symbols(source: &[u8], tree: &Tree, mut overlays: Vec<Piece>) -> Vec<Piece> {
    for occurrence in collect_symbol_occurrences(source, tree) {
        overlays.push(Piece {
            start: occurrence.start_byte,
            end: occurrence.end_byte,
            kind: match occurrence.target {
                DefinitionTarget::Id(_) => HighlightKind::Id,
                DefinitionTarget::Class(_) => HighlightKind::Class,
                DefinitionTarget::CustomProperty(_) => HighlightKind::CustomProperty,
            },
            declaration: occurrence.role == OccurrenceRole::Declaration,
        });
    }

    overlays.sort_by_key(|piece| (piece.start, piece.end));
    overlays
}

fn segment(bases: Vec<Piece>, overlays: Vec<Piece>) -> Vec<Piece> {
    let mut pieces = Vec::with_capacity(bases.len() + overlays.len());
    for base in bases {
        let mut cursor = base.start;
        let first = overlays.partition_point(|overlay| overlay.end <= base.start);
        for overlay in overlays[first..]
            .iter()
            .take_while(|overlay| overlay.start < base.end)
        {
            if overlay.start > cursor {
                pieces.push(Piece {
                    start: cursor,
                    end: overlay.start,
                    ..base
                });
            }
            cursor = cursor.max(overlay.end);
        }
        if cursor < base.end {
            pieces.push(Piece {
                start: cursor,
                end: base.end,
                ..base
            });
        }
    }
    pieces.extend(overlays);
    pieces.sort_by_key(|piece| (piece.start, piece.end));
    pieces
}

fn split_lines(source: &[u8], pieces: Vec<Piece>) -> Vec<HighlightToken> {
    let mut tokens = Vec::with_capacity(pieces.len());
    let mut row = 0;
    let mut line_start = 0;
    let mut scanned = 0;
    let mut covered = 0;
    for piece in pieces {
        if piece.start < covered || piece.end <= piece.start || piece.end > source.len() {
            continue;
        }
        covered = piece.end;
        let mut start = piece.start;
        while start < piece.end {
            for (index, &byte) in source[scanned..start].iter().enumerate() {
                if byte == b'\n' {
                    row += 1;
                    line_start = scanned + index + 1;
                }
            }
            scanned = start;
            let line_end = source[start..piece.end]
                .iter()
                .position(|&byte| byte == b'\n')
                .map_or(piece.end, |index| start + index);
            let mut end = line_end;
            if end > start && source[end - 1] == b'\r' && end < piece.end {
                end -= 1;
            }
            if end > start {
                tokens.push(HighlightToken {
                    start_byte: start,
                    row,
                    start_col: start - line_start,
                    len: end - start,
                    kind: piece.kind,
                    declaration: piece.declaration,
                });
            }
            start = line_end + 1;
        }
    }
    tokens
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

    fn summary(source: &str, tokens: &[HighlightToken]) -> Vec<(String, HighlightKind, bool)> {
        tokens
            .iter()
            .map(|token| {
                (
                    source[token.start_byte..token.start_byte + token.len].to_owned(),
                    token.kind,
                    token.declaration,
                )
            })
            .collect()
    }

    #[test]
    fn splits_values_around_symbols() -> Result<(), Box<dyn Error>> {
        use HighlightKind::{
            AttributeName, AttributeValue, Class, ElementName, EntityReference, Id,
        };

        let source = r#"<svg><g id="a" class="b c" fill="url(#a)">x &amp; y</g></svg>"#;
        let tree = parse_svg(source)?;
        let tokens = highlight_tokens(source.as_bytes(), &tree);
        let owned = summary(source, &tokens);
        let actual: Vec<_> = owned
            .iter()
            .map(|(text, kind, declaration)| (text.as_str(), *kind, *declaration))
            .collect();
        assert_eq!(
            actual,
            vec![
                ("svg", ElementName, false),
                ("g", ElementName, false),
                ("id", AttributeName, false),
                ("\"", AttributeValue, false),
                ("a", Id, true),
                ("\"", AttributeValue, false),
                ("class", AttributeName, false),
                ("\"", AttributeValue, false),
                ("b", Class, false),
                (" ", AttributeValue, false),
                ("c", Class, false),
                ("\"", AttributeValue, false),
                ("fill", AttributeName, false),
                ("\"url(#", AttributeValue, false),
                ("a", Id, false),
                (")\"", AttributeValue, false),
                ("&amp;", EntityReference, false),
                ("g", ElementName, false),
                ("svg", ElementName, false),
            ]
        );
        Ok(())
    }

    #[test]
    fn splits_multiline_tokens_per_line() -> Result<(), Box<dyn Error>> {
        let source = "<svg>\r\n<!-- a\r\n b -->\r\n<style>.k{}</style></svg>";
        let tree = parse_svg(source)?;
        let tokens = highlight_tokens(source.as_bytes(), &tree);
        let comments: Vec<_> = tokens
            .iter()
            .filter(|token| token.kind == HighlightKind::Comment)
            .map(|token| (token.row, token.start_col, token.len))
            .collect();
        assert_eq!(comments, vec![(1, 0, 6), (2, 0, 6)]);
        let class = tokens
            .iter()
            .find(|token| token.kind == HighlightKind::Class)
            .ok_or("class token")?;
        assert_eq!((class.row, class.start_col, class.len), (3, 8, 1));
        assert!(class.declaration);
        Ok(())
    }
}
