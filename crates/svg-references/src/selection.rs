use std::ops::Range;

use svg_tree::{child_of_kind, deepest_node_at};
use tree_sitter::Tree;

use crate::{inline_style_fragments, outline::element_tag};

/// Byte ranges enclosing `byte_offset`, innermost first, each strictly containing the one
/// before it, ending with the whole document.
///
/// Inside `<style>` the chain starts with the CSS syntax nodes. Quoted values add their
/// unquoted content, and elements add their content between the tags.
#[must_use]
pub fn selection_chain(source: &[u8], tree: &Tree, byte_offset: usize) -> Vec<Range<usize>> {
    let mut chain: Vec<Range<usize>> = Vec::new();

    if let Some((stylesheet, fragment)) =
        inline_style_fragments(source, tree)
            .into_iter()
            .find(|(sheet, _)| {
                (sheet.start_byte..=sheet.start_byte + sheet.css.len()).contains(&byte_offset)
            })
    {
        let inner = byte_offset - stylesheet.start_byte;
        let mut node = Some(deepest_node_at(&fragment.tree, inner));
        while let Some(current) = node {
            if current.is_named()
                && let (Some(start), Some(end)) = (
                    fragment.source_byte(current.start_byte()),
                    fragment.source_byte(current.end_byte()),
                )
            {
                extend(&mut chain, start..end, byte_offset);
            }
            node = current.parent();
        }
    }

    let mut node = Some(deepest_node_at(tree, byte_offset));
    while let Some(current) = node {
        if current.is_named() {
            if let Ok(text) = current.utf8_text(source)
                && text.len() >= 2
                && matches!(text.as_bytes()[0], b'"' | b'\'')
                && text.as_bytes()[text.len() - 1] == text.as_bytes()[0]
            {
                extend(
                    &mut chain,
                    current.start_byte() + 1..current.end_byte() - 1,
                    byte_offset,
                );
            }
            if element_tag(current).is_some()
                && let (Some(start_tag), Some(end_tag)) = (
                    child_of_kind(current, "start_tag"),
                    child_of_kind(current, "end_tag"),
                )
                && start_tag.end_byte() < end_tag.start_byte()
            {
                extend(
                    &mut chain,
                    start_tag.end_byte()..end_tag.start_byte(),
                    byte_offset,
                );
            }
            extend(&mut chain, current.byte_range(), byte_offset);
        }
        node = current.parent();
    }
    extend(&mut chain, 0..source.len(), byte_offset);
    chain
}

fn extend(chain: &mut Vec<Range<usize>>, range: Range<usize>, byte_offset: usize) {
    let contains_last = chain
        .last()
        .is_none_or(|last| range.start <= last.start && last.end <= range.end && range != *last);
    if contains_last && range.start <= byte_offset && byte_offset <= range.end {
        chain.push(range);
    }
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

    fn texts<'a>(source: &'a str, chain: &[Range<usize>]) -> Vec<&'a str> {
        chain.iter().map(|range| &source[range.clone()]).collect()
    }

    #[test]
    fn expands_from_class_token_to_document() -> Result<(), Box<dyn Error>> {
        let source = "<svg><g><rect class=\"a bb\"/></g></svg>\n";
        let tree = parse_svg(source)?;
        let offset = source.find("bb").ok_or("bb")? + 1;
        let chain = selection_chain(source.as_bytes(), &tree, offset);
        let texts = texts(source, &chain);
        assert_eq!(texts[0], "bb");
        assert!(texts.contains(&"a bb"));
        assert!(texts.contains(&"\"a bb\""));
        assert!(texts.contains(&"class=\"a bb\""));
        assert!(texts.contains(&"<rect class=\"a bb\"/>"));
        assert!(texts.contains(&"<g><rect class=\"a bb\"/></g>"));
        assert_eq!(texts.last().copied(), Some(source));
        for pair in chain.windows(2) {
            assert!(pair[1].start <= pair[0].start && pair[0].end <= pair[1].end);
            assert_ne!(pair[0], pair[1]);
        }
        Ok(())
    }

    #[test]
    fn starts_inside_css_for_style_content() -> Result<(), Box<dyn Error>> {
        let source = "<svg><style>.a { fill: red; }</style></svg>";
        let tree = parse_svg(source)?;
        let offset = source.find("red").ok_or("red")?;
        let chain = selection_chain(source.as_bytes(), &tree, offset);
        let texts = texts(source, &chain);
        assert_eq!(texts[0], "red");
        assert!(texts.contains(&"fill: red;"));
        assert!(texts.contains(&"{ fill: red; }"));
        assert!(texts.contains(&"<style>.a { fill: red; }</style>"));
        Ok(())
    }

    #[test]
    fn every_range_contains_the_cursor() -> Result<(), Box<dyn Error>> {
        let source = "<svg><rect width=\"12\"/><g>\n</g></svg>";
        let tree = parse_svg(source)?;
        for (offset, first) in [
            (source.find("\"12").ok_or("quote")?, "\"12\""),
            (source.find("<g>").ok_or("g")? + 1, "g"),
        ] {
            let chain = selection_chain(source.as_bytes(), &tree, offset);
            assert_eq!(texts(source, &chain)[0], first, "{offset}");
            for range in &chain {
                assert!(
                    range.start <= offset && offset <= range.end,
                    "{range:?} misses {offset}"
                );
            }
        }
        Ok(())
    }
}
