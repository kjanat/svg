use tower_lsp_server::ls_types::{Position, Range, TextEdit};
use tree_sitter::Tree;

use crate::{
    positions::{LineIndex, byte_offset_for_position, span_range_utf16},
    references::span_range,
};

pub fn prepare_rename(source: &str, tree: &Tree, position: Position) -> Option<Range> {
    let bytes = source.as_bytes();
    let occurrences = svg_references::collect_symbol_occurrences(bytes, tree);
    let offset = byte_offset_for_position(bytes, position);
    svg_references::symbol_occurrence_at(&occurrences, offset)
        .map(|occurrence| span_range_utf16(bytes, &occurrence.span))
}

pub fn rename(
    source: &str,
    tree: &Tree,
    position: Position,
    new_name: &str,
) -> Result<Option<Vec<TextEdit>>, String> {
    let bytes = source.as_bytes();
    let occurrences = svg_references::collect_symbol_occurrences(bytes, tree);
    let offset = byte_offset_for_position(bytes, position);
    let Some(target) = svg_references::symbol_occurrence_at(&occurrences, offset)
        .map(|occurrence| occurrence.target.clone())
    else {
        return Ok(None);
    };
    let edits = svg_references::rename_edits(&occurrences, &target, new_name)
        .map_err(|error| error.to_string())?;
    let index = LineIndex::new(bytes);
    Ok(Some(
        edits
            .into_iter()
            .map(|edit| TextEdit::new(span_range(&index, bytes, &edit.span), edit.new_text))
            .collect(),
    ))
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

    fn apply(source: &str, edits: &[TextEdit]) -> String {
        let mut sorted = edits.to_vec();
        sorted.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        let mut out = source.to_owned();
        for edit in sorted.iter().rev() {
            assert_eq!(edit.range.start.line, 0, "single-line fixture");
            let start = usize::try_from(edit.range.start.character).unwrap_or(usize::MAX);
            let end = usize::try_from(edit.range.end.character).unwrap_or(usize::MAX);
            out.replace_range(start..end, &edit.new_text);
        }
        out
    }

    #[test]
    fn prepare_rename_returns_bare_name_range() {
        let source = r##"<svg><use href="#icon"/></svg>"##;
        let tree = parse(source);
        assert_eq!(
            prepare_rename(source, &tree, Position::new(0, 18)),
            Some(Range::new(Position::new(0, 17), Position::new(0, 21)))
        );
        assert_eq!(prepare_rename(source, &tree, Position::new(0, 2)), None);
    }

    #[test]
    fn renames_id_everywhere() -> Result<(), String> {
        let source = concat!(
            r#"<svg><style>#g{fill:url(#g)}</style><linearGradient id="g"/>"#,
            r##"<rect fill="url(#g)" style="stroke:url(#g)"/><use href="#g"/></svg>"##,
        );
        let tree = parse(source);
        let position = Position::new(0, 56);
        let edits = rename(source, &tree, position, "next")?.ok_or("symbol at cursor")?;
        assert_eq!(
            apply(source, &edits),
            concat!(
                r#"<svg><style>#next{fill:url(#next)}</style><linearGradient id="next"/>"#,
                r##"<rect fill="url(#next)" style="stroke:url(#next)"/><use href="#next"/></svg>"##,
            )
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_names_and_ignores_non_symbols() {
        let source = r#"<svg><g id="a" class="c"/></svg>"#;
        let tree = parse(source);
        let error = rename(source, &tree, Position::new(0, 12), "1a");
        assert!(matches!(error, Err(message) if message.contains("XML name")));
        let error = rename(source, &tree, Position::new(0, 22), "a b");
        assert!(matches!(error, Err(message) if message.contains("whitespace")));
        assert_eq!(rename(source, &tree, Position::new(0, 2), "x"), Ok(None));
    }
}
