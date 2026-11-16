use svg_references::HighlightKind;
use tower_lsp_server::ls_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend,
};
use tree_sitter::Tree;

use crate::positions::{byte_col_to_utf16, u32_from_usize};

const TOKEN_TYPES: [SemanticTokenType; 7] = [
    SemanticTokenType::TYPE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::STRING,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::CLASS,
    SemanticTokenType::COMMENT,
    SemanticTokenType::ENUM_MEMBER,
];

const DECLARATION_BIT: u32 = 1;

pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: vec![SemanticTokenModifier::DECLARATION],
    }
}

const fn token_type_index(kind: HighlightKind) -> u32 {
    match kind {
        HighlightKind::ElementName => 0,
        HighlightKind::AttributeName | HighlightKind::CustomProperty => 1,
        HighlightKind::AttributeValue => 2,
        HighlightKind::Id => 3,
        HighlightKind::Class => 4,
        HighlightKind::Comment => 5,
        HighlightKind::EntityReference => 6,
    }
}

pub fn semantic_tokens(source: &str, tree: &Tree) -> Vec<SemanticToken> {
    let bytes = source.as_bytes();
    let mut encoded = Vec::new();
    let mut previous: Option<(usize, usize, u32)> = None;
    for token in svg_references::highlight_tokens(bytes, tree) {
        let Some(rest) = bytes.get(token.start_byte..) else {
            continue;
        };
        let (delta_line, delta_start, start_utf16) = match previous {
            Some((row, previous_byte, previous_utf16)) if row == token.row => {
                let gap = bytes.get(previous_byte..).map_or(0, |from| {
                    byte_col_to_utf16(from, 0, token.start_byte - previous_byte)
                });
                (0, gap, previous_utf16 + gap)
            }
            _ => {
                let line_start = token.start_byte - token.start_col;
                let start = bytes
                    .get(line_start..)
                    .map_or(0, |line| byte_col_to_utf16(line, 0, token.start_col));
                let delta_line = previous.map_or(token.row, |(row, _, _)| token.row - row);
                (u32_from_usize(delta_line), start, start)
            }
        };
        encoded.push(SemanticToken {
            delta_line,
            delta_start,
            length: byte_col_to_utf16(rest, 0, token.len),
            token_type: token_type_index(token.kind),
            token_modifiers_bitset: if token.declaration {
                DECLARATION_BIT
            } else {
                0
            },
        });
        previous = Some((token.row, token.start_byte, start_utf16));
    }
    encoded
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

    fn decode(tokens: &[SemanticToken]) -> Vec<(u32, u32, u32, u32, u32)> {
        let mut line = 0;
        let mut start = 0;
        tokens
            .iter()
            .map(|token| {
                if token.delta_line > 0 {
                    line += token.delta_line;
                    start = token.delta_start;
                } else {
                    start += token.delta_start;
                }
                (
                    line,
                    start,
                    token.length,
                    token.token_type,
                    token.token_modifiers_bitset,
                )
            })
            .collect()
    }

    #[test]
    fn legend_matches_type_indices() {
        let legend = semantic_tokens_legend();
        let type_of = |kind| {
            usize::try_from(token_type_index(kind))
                .ok()
                .and_then(|index| legend.token_types.get(index))
        };
        assert_eq!(
            type_of(HighlightKind::Id),
            Some(&SemanticTokenType::VARIABLE)
        );
        assert_eq!(
            type_of(HighlightKind::Class),
            Some(&SemanticTokenType::CLASS)
        );
        assert_eq!(
            type_of(HighlightKind::ElementName),
            Some(&SemanticTokenType::TYPE)
        );
        assert_eq!(
            type_of(HighlightKind::Comment),
            Some(&SemanticTokenType::COMMENT)
        );
        assert_eq!(
            legend.token_modifiers,
            vec![SemanticTokenModifier::DECLARATION]
        );
    }

    #[test]
    fn encodes_utf16_deltas_across_lines() {
        let source = "<svg>\n<!--\u{1F600}\nx--><g id=\"a\"/>\n</svg>";
        let tokens = decode(&semantic_tokens(source, &parse(source)));
        assert_eq!(
            tokens,
            vec![
                (0, 1, 3, 0, 0),
                (1, 0, 6, 5, 0),
                (2, 0, 4, 5, 0),
                (2, 5, 1, 0, 0),
                (2, 7, 2, 1, 0),
                (2, 10, 1, 2, 0),
                (2, 11, 1, 3, DECLARATION_BIT),
                (2, 12, 1, 2, 0),
                (3, 2, 3, 0, 0),
            ]
        );
    }
}
