use std::fmt::Write as _;

use tree_sitter::{Point, Tree};

use crate::{Span, css_tree};

pub struct CssFragment {
    pub tree: Tree,
    pub text: String,
    prefix: usize,
    base_byte: usize,
    base_point: Point,
    pub xml_escaped: bool,
}

impl CssFragment {
    pub fn stylesheet(
        css: &str,
        base_byte: usize,
        base_point: Point,
        xml_escaped: bool,
    ) -> Option<Self> {
        Self::wrapped("", css, "", base_byte, base_point, xml_escaped)
    }

    pub fn declarations(content: &str, base_byte: usize, base_point: Point) -> Option<Self> {
        Self::wrapped("x{", content, "}", base_byte, base_point, true)
    }

    pub fn value(content: &str, base_byte: usize, base_point: Point) -> Option<Self> {
        Self::wrapped("x{p:", content, "}", base_byte, base_point, true)
    }

    fn wrapped(
        prefix: &str,
        content: &str,
        suffix: &str,
        base_byte: usize,
        base_point: Point,
        xml_escaped: bool,
    ) -> Option<Self> {
        let text = [prefix, content, suffix].concat();
        Some(Self {
            tree: css_tree(&text)?,
            text,
            prefix: prefix.len(),
            base_byte,
            base_point,
            xml_escaped,
        })
    }

    pub fn decode(&self, text: &str) -> String {
        if self.xml_escaped {
            xml_unescape(text)
        } else {
            text.to_owned()
        }
    }

    pub fn source_byte(&self, inner: usize) -> Option<usize> {
        inner
            .checked_sub(self.prefix)
            .map(|offset| self.base_byte + offset)
    }

    pub fn source_point(&self, inner: Point) -> Option<Point> {
        if inner.row == 0 {
            inner
                .column
                .checked_sub(self.prefix)
                .map(|column| Point::new(self.base_point.row, self.base_point.column + column))
        } else {
            Some(Point::new(self.base_point.row + inner.row, inner.column))
        }
    }

    pub fn source_range(&self, start: usize, end: usize) -> Option<(usize, usize, Span)> {
        let text = self.text.as_bytes();
        let start_point = advance_point(Point::new(0, 0), text.get(..start)?);
        let end_point = advance_point(start_point, text.get(start..end)?);
        let source_start = self.source_point(start_point)?;
        let source_end = self.source_point(end_point)?;
        Some((
            self.source_byte(start)?,
            self.source_byte(end)?,
            span_between(source_start, source_end),
        ))
    }

    pub fn node_text(&self, node: tree_sitter::Node<'_>) -> Option<&str> {
        node.utf8_text(self.text.as_bytes()).ok()
    }
}

pub fn advance_point(start: Point, text: &[u8]) -> Point {
    let mut point = start;
    for &byte in text {
        if byte == b'\n' {
            point.row += 1;
            point.column = 0;
        } else {
            point.column += 1;
        }
    }
    point
}

pub const fn rebase_point(base: Point, inner: Point) -> Point {
    if inner.row == 0 {
        Point::new(base.row, base.column + inner.column)
    } else {
        Point::new(base.row + inner.row, inner.column)
    }
}

pub const fn span_between(start: Point, end: Point) -> Span {
    Span {
        start_row: start.row,
        start_col: start.column,
        end_row: end.row,
        end_col: end.column,
    }
}

pub fn node_span(node: tree_sitter::Node<'_>) -> Span {
    span_between(node.start_position(), node.end_position())
}

pub fn strip_quotes(text: &str) -> (usize, &str) {
    let bytes = text.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(&first), Some(&last))
            if bytes.len() >= 2 && first == last && matches!(first, b'"' | b'\'') =>
        {
            text.get(1..text.len() - 1)
                .map_or((0, text), |inner| (1, inner))
        }
        _ => (0, text),
    }
}

pub fn css_unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let mut hex = String::new();
        while hex.len() < 6 {
            match chars.peek() {
                Some(next) if next.is_ascii_hexdigit() => {
                    hex.push(*next);
                    chars.next();
                }
                _ => break,
            }
        }
        if hex.is_empty() {
            if let Some(next) = chars.next() {
                out.push(next);
            }
            continue;
        }
        if chars.peek().is_some_and(char::is_ascii_whitespace) {
            chars.next();
        }
        let decoded = u32::from_str_radix(&hex, 16)
            .ok()
            .filter(|&code| code != 0)
            .and_then(char::from_u32)
            .unwrap_or(char::REPLACEMENT_CHARACTER);
        out.push(decoded);
    }
    out
}

pub fn css_escape_identifier(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let first = name.chars().next();
    let single_dash = name == "-";
    for (index, ch) in name.chars().enumerate() {
        let leading_digit =
            ch.is_ascii_digit() && (index == 0 || (index == 1 && first == Some('-')));
        if ch == '\0' {
            out.push(char::REPLACEMENT_CHARACTER);
        } else if ch.is_ascii_control() || leading_digit {
            out.push('\\');
            let _ = write!(out, "{:x} ", u32::from(ch));
        } else if single_dash {
            out.push_str("\\-");
        } else if !ch.is_ascii() || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('\\');
            out.push(ch);
        }
    }
    out
}

pub fn xml_escape_attribute(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn xml_unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let decoded = after.find(';').and_then(|semi| {
            let entity = &after[..semi];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .map_or_else(
                        || {
                            entity
                                .strip_prefix('#')
                                .and_then(|digits| digits.parse::<u32>().ok())
                        },
                        |hex| u32::from_str_radix(hex, 16).ok(),
                    )
                    .and_then(char::from_u32),
            }?;
            Some((ch, semi))
        });
        if let Some((ch, semi)) = decoded {
            out.push(ch);
            rest = &after[semi + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

const fn is_name_start_char(ch: char) -> bool {
    matches!(ch,
        ':' | 'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}'
        | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}'
        | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}'
        | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

const fn is_name_char(ch: char) -> bool {
    is_name_start_char(ch)
        || matches!(ch,
            '-' | '.' | '0'..='9' | '\u{B7}'
            | '\u{300}'..='\u{36F}'
            | '\u{203F}'..='\u{2040}')
}

pub fn is_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(is_name_start_char) && chars.all(is_name_char)
}

pub const fn is_xml_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{C}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_escape_round_trips_through_unescape() {
        for name in ["a.b", "1x", "-2", "-", "a b", "plain", "--accent", "é"] {
            assert_eq!(css_unescape(&css_escape_identifier(name)), name, "{name}");
        }
        assert_eq!(css_escape_identifier("a.b"), "a\\.b");
        assert_eq!(css_escape_identifier("1x"), "\\31 x");
        assert_eq!(css_escape_identifier("--accent"), "--accent");
        assert_eq!(css_escape_identifier("a\u{1}b"), "a\\1 b");
        assert_eq!(css_unescape(&css_escape_identifier("a\u{1}b")), "a\u{1}b");
        assert_eq!(css_escape_identifier("\0"), "\u{FFFD}");
    }

    #[test]
    fn css_unescape_decodes_hex_and_literal_escapes() {
        assert_eq!(css_unescape("a\\.b"), "a.b");
        assert_eq!(css_unescape("\\31 x"), "1x");
        assert_eq!(css_unescape("\\0"), "\u{FFFD}");
    }

    #[test]
    fn xml_escape_and_unescape_attribute_text() {
        assert_eq!(xml_escape_attribute("a&b\"c'<"), "a&amp;b&quot;c&apos;&lt;");
        assert_eq!(xml_unescape("a&amp;b&#65;&#x42;&bogus;&"), "a&bAB&bogus;&");
    }

    #[test]
    fn xml_name_rules() {
        assert!(is_xml_name("grad-1"));
        assert!(is_xml_name("_a.b"));
        assert!(is_xml_name("é"));
        assert!(!is_xml_name("1grad"));
        assert!(!is_xml_name("a b"));
        assert!(!is_xml_name(""));
        assert!(!is_xml_name("-a"));
    }

    #[test]
    fn strip_quotes_handles_matching_pairs_only() {
        assert_eq!(strip_quotes("\"a\""), (1, "a"));
        assert_eq!(strip_quotes("'a'"), (1, "a"));
        assert_eq!(strip_quotes("\"a'"), (0, "\"a'"));
        assert_eq!(strip_quotes("a"), (0, "a"));
    }

    #[test]
    fn advance_point_counts_rows_and_columns() {
        assert_eq!(advance_point(Point::new(2, 3), b"ab"), Point::new(2, 5));
        assert_eq!(advance_point(Point::new(2, 3), b"a\nbc"), Point::new(3, 2));
    }
}
