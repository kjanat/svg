//! Shared tree-sitter traversal and query helpers.
//!
//! This crate provides the small set of tree-sitter utility functions used by
//! multiple workspace crates (`svg-references`, `svg-lint`, `svg-color`,
//! `svg-language-server`).  Centralising them here eliminates duplicated
//! implementations and keeps the helpers in sync.
//!
//! # Examples
//!
//! ```rust
//! assert!(svg_tree::is_attribute_name_kind("attribute_name"));
//! assert!(svg_tree::is_attribute_name_kind("viewBox_attribute_name"));
//! assert!(!svg_tree::is_attribute_name_kind("element"));
//! ```

/// Iterative depth-first traversal of every node reachable from `cursor`.
///
/// Calls `f` on each node visited.  Uses `TreeCursor` internally for
/// efficiency (no per-node allocation).  Iterative traversal to avoid stack
/// overflow from deeply-nested SVG trees.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br"<svg><rect/></svg>";
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
///
/// let mut count = 0;
/// let mut cursor = tree.root_node().walk();
/// svg_tree::walk_tree(&mut cursor, &mut |_| count += 1);
/// assert!(count > 1);
/// # Ok(())
/// # }
/// ```
pub fn walk_tree(
    cursor: &mut tree_sitter::TreeCursor<'_>,
    f: &mut impl FnMut(tree_sitter::Node<'_>),
) {
    let start_depth = cursor.depth();
    loop {
        f(cursor.node());

        // Try descending into first child.
        if cursor.goto_first_child() {
            continue;
        }

        // No children — try next sibling, or walk up until we find one.
        // Never move to siblings at start_depth: that would leak into
        // adjacent subtrees outside the node we were given.
        loop {
            if cursor.depth() > start_depth && cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() || cursor.depth() < start_depth {
                return;
            }
        }
    }
}

/// Return the deepest (most specific) node that spans `byte_offset`.
///
/// Falls back to the tree root when no descendant covers the offset.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br#"<svg><rect id="box"/></svg>"#;
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
///
/// let offset = source.iter().position(|byte| *byte == b'b').ok_or("missing id")?;
/// let node = svg_tree::deepest_node_at(&tree, offset);
/// assert!(node.byte_range().contains(&offset));
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn deepest_node_at(tree: &tree_sitter::Tree, byte_offset: usize) -> tree_sitter::Node<'_> {
    tree.root_node()
        .descendant_for_byte_range(byte_offset, byte_offset)
        .unwrap_or_else(|| tree.root_node())
}

/// Walk up from `node` and return the first ancestor whose kind is in `kinds`.
///
/// The search includes `node` itself.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br"<svg><g><rect/></g></svg>";
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
///
/// let offset = source.iter().position(|byte| *byte == b'r').ok_or("missing rect")?;
/// let node = svg_tree::deepest_node_at(&tree, offset);
/// let ancestor = svg_tree::find_ancestor_any(node, &["element", "svg_root_element"]);
/// assert!(ancestor.is_some());
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn find_ancestor_any<'a>(
    node: tree_sitter::Node<'a>,
    kinds: &[&str],
) -> Option<tree_sitter::Node<'a>> {
    let mut current = node;
    loop {
        if kinds.contains(&current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// Return `true` if any ancestor of `node` (exclusive) has the given `kind`.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br"<svg><g><rect/></g></svg>";
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
///
/// let offset = source.iter().position(|byte| *byte == b'r').ok_or("missing rect")?;
/// let node = svg_tree::deepest_node_at(&tree, offset);
/// assert!(svg_tree::has_ancestor(node, "element"));
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn has_ancestor(node: tree_sitter::Node<'_>, kind: &str) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == kind {
            return true;
        }
        current = parent;
    }
    false
}

/// Return the first direct child of `node` whose kind matches `kind`.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = br"<svg><rect/></svg>";
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
/// let tree = parser
///     .parse(source, None)
///     .ok_or_else(|| std::io::Error::other("parse failed"))?;
///
/// let document = tree.root_node();
/// let root = svg_tree::child_of_kind(document, "svg_root_element");
/// assert!(root.is_some());
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn child_of_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.kind() == kind {
            return Some(child);
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

/// Check whether a tree-sitter node kind represents an attribute name.
///
/// tree-sitter-svg emits `attribute_name` for plain attributes and
/// `*_attribute_name` variants (e.g. `viewBox_attribute_name`) as value
/// grammars expand.
///
/// # Examples
///
/// ```rust
/// assert!(svg_tree::is_attribute_name_kind("attribute_name"));
/// assert!(svg_tree::is_attribute_name_kind("viewBox_attribute_name"));
/// assert!(!svg_tree::is_attribute_name_kind("element"));
/// ```
#[must_use]
pub fn is_attribute_name_kind(kind: &str) -> bool {
    kind == "attribute_name" || kind.ends_with("_attribute_name")
}

/// Check whether a node kind is an attribute wrapper.
///
/// `attribute` is a tree-sitter supertype in tree-sitter-svg, so concrete parse
/// trees usually expose `generic_attribute` or typed `*_attribute` nodes instead
/// of a visible `attribute` wrapper.
///
/// # Examples
///
/// ```rust
/// assert!(svg_tree::is_attribute_node_kind("generic_attribute"));
/// assert!(svg_tree::is_attribute_node_kind("href_attribute"));
/// assert!(!svg_tree::is_attribute_node_kind("href_attribute_value"));
/// ```
#[must_use]
pub fn is_attribute_node_kind(kind: &str) -> bool {
    kind == "attribute" || kind == "generic_attribute" || kind.ends_with("_attribute")
}

/// Decode the CSS escapes in `text`.
///
/// An escape follows CSS Syntax 3 §4.3.7, "consume an escaped code point": a
/// backslash and 1 to 6 hex digits, with one following whitespace (CRLF
/// counting as one), or a backslash and any other character. Zero and invalid
/// code points become U+FFFD.
///
/// # Examples
///
/// ```rust
/// assert_eq!(svg_tree::css_unescape(r"v\61 r"), "var");
/// assert_eq!(svg_tree::css_unescape(r"a\.b"), "a.b");
/// ```
#[must_use]
pub fn css_unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(backslash) = rest.find('\\') {
        out.push_str(&rest[..backslash]);
        rest = &rest[backslash + 1..];
        if let Some((decoded, len)) = escaped_code_point(rest) {
            out.push(decoded);
            rest = &rest[len..];
        }
    }
    out.push_str(rest);
    out
}

/// Rewrite the identifiers in CSS value `text` into one canonical spelling,
/// so text comparisons and function-call splitting see what a CSS tokenizer
/// sees.
///
/// Each ident sequence with escapes is decoded when the result still starts an
/// ident sequence (CSS Syntax 3 §4.3.9) or follows `#` (a hash token value).
/// Escaped characters outside the ident code points (CSS Syntax 3 §4.2) stay
/// escaped, spelled `\<lowercase hex> `. Other sequences are left as written,
/// because decoding `\31 0` to `10` would turn an ident into a number.
/// Applying the function twice gives the same text as applying it once.
///
/// # Examples
///
/// ```rust
/// assert_eq!(svg_tree::css_canonical_identifiers(r"v\61 r(--\63)"), "var(--c)");
/// assert_eq!(svg_tree::css_canonical_identifiers(r"r\28 x"), r"r\28 x");
/// assert_eq!(svg_tree::css_canonical_identifiers(r"r\(x"), r"r\28 x");
/// assert_eq!(svg_tree::css_canonical_identifiers(r"\31 0"), r"\31 0");
/// assert_eq!(svg_tree::css_canonical_identifiers(r"#\66 00"), "#f00");
/// ```
#[must_use]
pub fn css_canonical_identifiers(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('\\') {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        let run_len = ident_run_len(rest);
        if run_len == 0 {
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
            continue;
        }
        let (run, tail) = rest.split_at(run_len);
        let decoded = decode_ident_run(run);
        if out.ends_with('#') || starts_ident_sequence(&decoded) {
            out.push_str(&decoded);
        } else {
            out.push_str(run);
        }
        rest = tail;
    }
    std::borrow::Cow::Owned(out)
}

/// Decode the escape that follows a backslash, returning the code point and
/// the bytes it spans. Returns `None` at the end of input.
fn escaped_code_point(text: &str) -> Option<(char, usize)> {
    let hex_len = text
        .bytes()
        .take(6)
        .take_while(u8::is_ascii_hexdigit)
        .count();
    if hex_len == 0 {
        let ch = text.chars().next()?;
        return Some((ch, ch.len_utf8()));
    }
    let decoded = u32::from_str_radix(&text[..hex_len], 16)
        .ok()
        .filter(|&code| code != 0)
        .and_then(char::from_u32)
        .unwrap_or(char::REPLACEMENT_CHARACTER);
    let after = &text[hex_len..];
    let whitespace = if after.starts_with("\r\n") {
        2
    } else {
        usize::from(
            after
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_whitespace()),
        )
    };
    Some((decoded, hex_len + whitespace))
}

/// CSS Syntax 3 §4.2 "ident-start code point".
const fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || !ch.is_ascii()
}

/// CSS Syntax 3 §4.2 "ident code point".
const fn is_ident_code_point(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit() || ch == '-'
}

/// CSS Syntax 3 §4.3.8: a backslash followed by anything except a newline.
fn is_valid_escape(next: Option<char>) -> bool {
    next.is_some_and(|ch| !matches!(ch, '\n' | '\r' | '\u{c}'))
}

/// Byte length of the ident code points and valid escapes at the start of
/// `text`.
fn ident_run_len(text: &str) -> usize {
    let mut len = 0;
    loop {
        let rest = &text[len..];
        let mut chars = rest.chars();
        match chars.next() {
            Some('\\') if is_valid_escape(chars.next()) => {
                len += 1 + escaped_code_point(&rest[1..]).map_or(0, |(_, escape)| escape);
            }
            Some(ch) if is_ident_code_point(ch) => len += ch.len_utf8(),
            _ => return len,
        }
    }
}

/// Decode `run`, re-escaping every decoded character outside the ident code
/// points as `\<lowercase hex> `.
fn decode_ident_run(run: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(run.len());
    let mut rest = run;
    while let Some(ch) = rest.chars().next() {
        let (decoded, len) = if ch == '\\' {
            escaped_code_point(&rest[1..])
                .map_or((None, 1), |(decoded, escape)| (Some(decoded), 1 + escape))
        } else {
            (Some(ch), ch.len_utf8())
        };
        match decoded {
            Some(decoded) if is_ident_code_point(decoded) => out.push(decoded),
            Some(decoded) => {
                let _ = write!(out, "\\{:x} ", u32::from(decoded));
            }
            None => {}
        }
        rest = &rest[len..];
    }
    out
}

/// CSS Syntax 3 §4.3.9 "check if three code points would start an ident
/// sequence".
fn starts_ident_sequence(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some('-') => {
            let mut rest = chars.clone();
            match rest.next() {
                Some('-') => true,
                Some('\\') => is_valid_escape(rest.next()),
                Some(ch) => is_ident_start(ch),
                None => false,
            }
        }
        Some('\\') => is_valid_escape(chars.next()),
        Some(ch) => is_ident_start(ch),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn css_unescape_decodes_hex_and_literal_escapes() {
        assert_eq!(css_unescape("a\\.b"), "a.b");
        assert_eq!(css_unescape("\\31 x"), "1x");
        assert_eq!(css_unescape("\\0"), "\u{FFFD}");
        assert_eq!(css_unescape("\\31\r\nx"), "1x");
        assert_eq!(css_unescape("\\31\rx"), "1x");
        assert_eq!(css_unescape("a\\"), "a");
    }

    #[test]
    fn css_canonical_identifiers_decodes_only_what_stays_an_ident() {
        let cases = [
            ("red", "red"),
            ("r\\65 d", "red"),
            ("R\\45 D", "RED"),
            ("v\\61 r(--\\63, \\#f00)", "var(--c, \\23 f00)"),
            (
                "color-mix(in srgb, r\\65 d, blue)",
                "color-mix(in srgb, red, blue)",
            ),
            ("r\\28 x", "r\\28 x"),
            ("r\\(x", "r\\28 x"),
            ("\\31 0", "\\31 0"),
            ("\\2d 5", "\\2d 5"),
            ("-\\31 x", "-\\31 x"),
            ("--\\31 x", "--1x"),
            ("5\\61", "5\\61"),
            ("#\\66 00", "#f00"),
            ("#\\31 23", "#123"),
            ("a\\\nb", "a\\\nb"),
            ("a\\", "a\\"),
        ];
        for (input, expected) in cases {
            let canonical = css_canonical_identifiers(input);
            assert_eq!(canonical, expected, "input {input:?}");
            assert_eq!(
                css_canonical_identifiers(&canonical),
                canonical,
                "canonical form of {input:?} is stable"
            );
        }
    }

    fn parse_svg(src: &[u8]) -> Result<tree_sitter::Tree, Box<dyn std::error::Error>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
        let tree = parser
            .parse(src, None)
            .ok_or_else(|| std::io::Error::other("parse failed"))?;
        Ok(tree)
    }

    #[test]
    fn walk_tree_visits_all_nodes() -> TestResult {
        let tree = parse_svg(br"<svg><rect/></svg>")?;
        let mut count = 0;
        let mut cursor = tree.root_node().walk();
        walk_tree(&mut cursor, &mut |_| {
            count += 1;
        });
        assert!(count > 3, "should visit multiple nodes, got {count}");
        Ok(())
    }

    #[test]
    fn deepest_node_at_finds_element() -> TestResult {
        let src = br#"<svg><rect x="1"/></svg>"#;
        let tree = parse_svg(src)?;
        let rect_pos = src.iter().position(|&b| b == b'r').ok_or("no r")?;
        let node = deepest_node_at(&tree, rect_pos);
        assert!(
            node.kind().contains("name") || node.kind().contains("rect"),
            "should find node near rect: {} at {}",
            node.kind(),
            rect_pos
        );
        Ok(())
    }

    #[test]
    fn deepest_node_at_falls_back_to_root() -> TestResult {
        let tree = parse_svg(br"<svg/>")?;
        let node = deepest_node_at(&tree, 9999);
        assert_eq!(node.kind(), tree.root_node().kind());
        Ok(())
    }

    #[test]
    fn find_ancestor_any_finds_matching_kind() -> TestResult {
        let src = br"<svg><g><rect/></g></svg>";
        let tree = parse_svg(src)?;
        let rect_pos = src.iter().position(|&b| b == b'r').ok_or("no r")?;
        let node = deepest_node_at(&tree, rect_pos);
        let ancestor = find_ancestor_any(node, &["element", "svg_root_element"]);
        assert!(ancestor.is_some(), "should find element ancestor");
        Ok(())
    }

    #[test]
    fn has_ancestor_detects_parent() -> TestResult {
        let src = br"<svg><g><rect/></g></svg>";
        let tree = parse_svg(src)?;
        let rect_pos = src.iter().position(|&b| b == b'r').ok_or("no r")?;
        let node = deepest_node_at(&tree, rect_pos);
        assert!(has_ancestor(node, "element") || has_ancestor(node, "svg_root_element"));
        Ok(())
    }

    #[test]
    fn is_attribute_name_kind_matches() {
        assert!(is_attribute_name_kind("attribute_name"));
        assert!(is_attribute_name_kind("viewBox_attribute_name"));
        assert!(!is_attribute_name_kind("attribute_value"));
        assert!(!is_attribute_name_kind("element"));
    }

    #[test]
    fn is_attribute_node_kind_matches() {
        assert!(is_attribute_node_kind("attribute"));
        assert!(is_attribute_node_kind("generic_attribute"));
        assert!(is_attribute_node_kind("href_attribute"));
        assert!(!is_attribute_node_kind("href_attribute_value"));
        assert!(!is_attribute_node_kind("href_attribute_name"));
        assert!(!is_attribute_node_kind("element"));
    }

    #[test]
    fn iri_reference_node_exists_in_url_attributes() -> TestResult {
        let src = br#"<svg><rect clip-path="url(#clip)"/></svg>"#;
        let tree = parse_svg(src)?;
        let mut found_iri = false;
        let mut cursor = tree.root_node().walk();
        walk_tree(&mut cursor, &mut |node| {
            if node.kind() == "iri_reference" {
                found_iri = true;
            }
        });
        assert!(
            found_iri,
            "grammar should produce iri_reference for host functional IRI attributes"
        );
        Ok(())
    }

    #[test]
    fn typed_attribute_value_kinds_are_produced() -> TestResult {
        let src = br#"<svg><animate dur="2s"/><line stroke-dasharray="10 5"/></svg>"#;
        let tree = parse_svg(src)?;
        let mut kinds = Vec::new();
        let mut cursor = tree.root_node().walk();
        walk_tree(&mut cursor, &mut |node| {
            let kind = node.kind();
            if kind.ends_with("_attribute_value") && kind != "quoted_attribute_value" {
                kinds.push(kind.to_owned());
            }
        });
        assert!(
            kinds.iter().any(|k| k == "duration_attribute_value"),
            "dur should produce duration_attribute_value: {kinds:?}"
        );
        assert!(
            kinds
                .iter()
                .any(|k| k == "stroke_dasharray_attribute_value"),
            "stroke-dasharray should produce stroke_dasharray_attribute_value: {kinds:?}"
        );
        Ok(())
    }
}
