use std::{borrow::Cow, ops::Range};

use svg_tree::css_canonical_identifiers;

pub(super) fn is_style_raw_text(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "raw_text" {
        return false;
    }

    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "element" {
        return false;
    }

    let mut cursor = parent.walk();
    if !cursor.goto_first_child() {
        return false;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "start_tag" {
            return tag_name(child, source).is_some_and(|name| name.eq_ignore_ascii_case("style"));
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    false
}

fn tag_name<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "name" {
            return std::str::from_utf8(&source[child.byte_range()]).ok();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    None
}

pub(super) fn css_function_name<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "function_name" {
            return std::str::from_utf8(&source[child.byte_range()]).ok();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    None
}

pub(super) fn has_color_like_property(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(name) = nearest_declaration_property_name(node, source) else {
        return false;
    };
    is_color_like_property(&name)
}

fn nearest_declaration_property_name<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<Cow<'a, str>> {
    let mut current = Some(node);

    while let Some(node) = current {
        if node.kind() == "declaration" {
            return declaration_property_name(node, source);
        }
        current = node.parent();
    }

    None
}

/// The declaration's value: the first named node after its `:` other than a
/// comment, and the byte
/// range from that node to the `;`, `!` or `}` that ends the declaration,
/// trailing whitespace excluded.
///
/// The end comes from the source text. tree-sitter-css splits an escaped
/// function name (`c\6flor-mix(`) and its error recovery can end the
/// declaration before the closing `)`.
pub(super) fn declaration_value<'t>(
    node: tree_sitter::Node<'t>,
    source: &[u8],
) -> Option<(tree_sitter::Node<'t>, Range<usize>)> {
    let mut cursor = node.walk();
    let mut children = node.children(&mut cursor);
    children.by_ref().find(|child| child.kind() == ":")?;
    let first = children.find(|child| child.is_named() && child.kind() != "comment")?;
    let start = first.start_byte();
    let end = start + css_value_len(source.get(start..)?);
    Some((first, start..end))
}

/// Length of the CSS value at the start of `css`, up to the first `;`, `!`,
/// `}` or unmatched closing bracket outside strings, comments and escapes,
/// trailing whitespace excluded.
fn css_value_len(css: &[u8]) -> usize {
    let mut depth = 0usize;
    let mut end = 0;
    let mut index = 0;
    while let Some(&byte) = css.get(index) {
        match byte {
            b'\\' => index += 1,
            b'"' | b'\'' => {
                index += 1;
                while let Some(&inner) = css.get(index) {
                    match inner {
                        b'\\' => index += 1,
                        _ if inner == byte => break,
                        _ => {}
                    }
                    index += 1;
                }
            }
            b'/' if css.get(index + 1) == Some(&b'*') => {
                index += css[index + 2..]
                    .windows(2)
                    .position(|pair| pair == b"*/")
                    .map_or(css.len() - index, |close| close + 3);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth > 0 => depth -= 1,
            b';' | b'!' | b')' | b']' | b'}' if depth == 0 => break,
            _ => {}
        }
        index += 1;
        if !byte.is_ascii_whitespace() {
            end = index.min(css.len());
        }
    }
    end
}

pub(super) fn declaration_value_text<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<&'a str> {
    let (_, range) = declaration_value(node, source)?;
    std::str::from_utf8(source.get(range)?).ok()
}

/// The declaration's property name: the text before its `:`, with escaped
/// identifiers decoded, so `f\69ll` reads `fill` and `--\63` reads `--c`.
pub(super) fn declaration_property_name<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<Cow<'a, str>> {
    let mut cursor = node.walk();
    let colon = node
        .children(&mut cursor)
        .find(|child| child.kind() == ":")?;
    let name = std::str::from_utf8(source.get(node.start_byte()..colon.start_byte())?)
        .ok()?
        .trim();
    match css_canonical_identifiers(name) {
        Cow::Borrowed(name) => Some(Cow::Borrowed(name)),
        Cow::Owned(name) => Some(Cow::Owned(name)),
    }
}

pub(super) fn is_color_like_property(name: &str) -> bool {
    // `wrapping_sub` makes short names produce an out-of-bounds range, so
    // `get(..)` returns `None` instead of panicking on the suffix check.
    let has_color_suffix = name
        .get(name.len().wrapping_sub("color".len())..)
        .is_some_and(|s| s.eq_ignore_ascii_case("color"));

    name.eq_ignore_ascii_case("fill")
        || name.eq_ignore_ascii_case("stroke")
        || has_color_suffix
        || name.starts_with("--")
}
