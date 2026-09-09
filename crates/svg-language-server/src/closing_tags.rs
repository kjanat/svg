//! Closing-name recovery from the document's cached syntax tree.

use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Range, TextEdit,
};
use tree_sitter::{Node, Tree};

use crate::positions::{position_for_byte_offset, u32_from_usize};

/// `Some` owns a closing-tag context, including one with no valid suggestion.
/// This prevents the normal child/attribute completion path from leaking into it.
pub fn completion(source: &[u8], tree: &Tree, offset: usize) -> Option<Vec<CompletionItem>> {
    let start = source
        .get(..offset)?
        .iter()
        .rposition(|byte| *byte == b'<')?;
    let name_start = start + 2;
    if source.get(start..name_start)? != b"</" || offset < name_start {
        return None;
    }

    let name_end = name_start
        + source[name_start..]
            .iter()
            .take_while(|byte| is_name_byte(**byte))
            .count();
    if offset > name_end {
        return if source[name_end..offset].contains(&b'>') {
            None
        } else {
            Some(Vec::new())
        };
    }

    // Require a real closing delimiter, including in ERROR nodes. Raw text,
    // comments and attribute values can contain the same bytes as ordinary text.
    let delimiter = tree
        .root_node()
        .descendant_for_byte_range(start, name_start)?;
    if delimiter.kind() != "</" || delimiter.start_byte() != start {
        return Some(Vec::new());
    }

    let Some(open) = open_tags_before(tree, source, start) else {
        return Some(Vec::new());
    };
    if !svg_lint::tag_chain_resolves_to_svg_namespace(source, &open) {
        return Some(Vec::new());
    }
    let name = open
        .last()?
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?;
    let has_bracket = source[name_end..]
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(&b'>');
    Some(vec![CompletionItem {
        label: name.to_owned(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some("Close the open SVG element".to_owned()),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
            Range::new(
                position_for_byte_offset(source, name_start),
                position_for_byte_offset(source, name_end),
            ),
            if has_bracket {
                name.to_owned()
            } else {
                format!("{name}>")
            },
        ))),
        ..Default::default()
    }])
}

const fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte >= 0x80 || matches!(byte, b'_' | b':' | b'-' | b'.')
}

fn open_tags_before<'tree>(
    tree: &'tree Tree,
    source: &[u8],
    offset: usize,
) -> Option<Vec<Node<'tree>>> {
    let mut open = Vec::new();
    let mut cursor = tree.root_node().walk();
    loop {
        let node = cursor.node();
        if node.start_byte() >= offset {
            break;
        }
        let tag = matches!(
            node.kind(),
            "start_tag" | "end_tag" | "erroneous_end_tag" | "self_closing_tag"
        );
        // An opening delimiter outside a parsed tag can be an unfinished
        // attribute. Do not interpret a later `</` inside it as child markup.
        if node.kind() == "<" || (tag && node.end_byte() > offset) {
            return None;
        }
        if tag && node.end_byte() <= offset {
            // Missing delimiters inserted by error recovery do not open or close
            // an element. Only explicit tags before the requested edit count.
            if node
                .child(u32_from_usize(node.child_count().saturating_sub(1)))
                .is_some_and(|last| last.kind() == ">" && !last.is_missing())
            {
                if node.kind() == "start_tag" {
                    open.push(node);
                } else if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|name| name.utf8_text(source).ok())
                    && let Some(index) = open.iter().rposition(|tag| {
                        tag.child_by_field_name("name")
                            .and_then(|name| name.utf8_text(source).ok())
                            == Some(name)
                    })
                {
                    // A closing ancestor also ends any malformed unclosed children.
                    open.truncate(index);
                }
            }
        }
        if !tag && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Some(open);
            }
        }
    }
    Some(open)
}
