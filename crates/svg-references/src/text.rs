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
    offsets: Option<Vec<usize>>,
    text_lines: Vec<usize>,
    source_lines: Vec<usize>,
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(index, _)| index + 1))
        .collect()
}

fn point_in(lines: &[usize], offset: usize) -> Point {
    let row = lines
        .partition_point(|&start| start <= offset)
        .saturating_sub(1);
    Point::new(row, offset - lines.get(row).copied().unwrap_or_default())
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
        let (decoded, offsets) = if xml_escaped {
            xml_unescape_mapped(content)
        } else {
            (content.to_owned(), None)
        };
        let text = [prefix, &decoded, suffix].concat();
        Some(Self {
            tree: css_tree(&text)?,
            text_lines: line_starts(&text),
            source_lines: if offsets.is_some() {
                line_starts(content)
            } else {
                Vec::new()
            },
            text,
            prefix: prefix.len(),
            base_byte,
            base_point,
            xml_escaped,
            offsets,
        })
    }

    pub fn inner_byte(&self, source_byte: usize) -> Option<usize> {
        let offset = source_byte.checked_sub(self.base_byte)?;
        let inner = self.offsets.as_ref().map_or(offset, |offsets| {
            offsets
                .partition_point(|&mapped| mapped <= offset)
                .saturating_sub(1)
        });
        Some(self.prefix + inner)
    }

    fn source_offset(&self, inner: usize) -> Option<usize> {
        let offset = inner.checked_sub(self.prefix)?;
        self.offsets
            .as_ref()
            .map_or(Some(offset), |offsets| offsets.get(offset).copied())
    }

    pub fn source_byte(&self, inner: usize) -> Option<usize> {
        self.source_offset(inner)
            .map(|offset| self.base_byte + offset)
    }

    pub fn source_point(&self, inner_byte: usize, inner: Point) -> Option<Point> {
        if self.offsets.is_some() {
            let offset = self.source_offset(inner_byte)?;
            return Some(rebase_point(
                self.base_point,
                point_in(&self.source_lines, offset),
            ));
        }
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
        if start > end || end > self.text.len() {
            return None;
        }
        let start_point = point_in(&self.text_lines, start);
        let end_point = point_in(&self.text_lines, end);
        let source_start = self.source_point(start, start_point)?;
        let source_end = self.source_point(end, end_point)?;
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

pub use svg_tree::css_unescape;

const fn is_css_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') || !ch.is_ascii()
}

fn css_escape_len(text: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    if chars.next()?.1 != '\\' {
        return None;
    }
    let (at, first) = chars.next()?;
    if matches!(first, '\n' | '\r' | '\x0c') {
        return None;
    }
    if !first.is_ascii_hexdigit() {
        return Some(at + first.len_utf8());
    }
    let hex = text[at..]
        .bytes()
        .take(6)
        .take_while(u8::is_ascii_hexdigit)
        .count();
    let end = at + hex;
    let space = match &text.as_bytes()[end..] {
        [b'\r', b'\n', ..] => 2,
        [b' ' | b'\t' | b'\n' | b'\r' | b'\x0c', ..] => 1,
        _ => 0,
    };
    Some(end + space)
}

pub fn css_ident_len(text: &str) -> (usize, bool) {
    let mut len = 0;
    let mut escaped = false;
    loop {
        let rest = &text[len..];
        if let Some(escape) = css_escape_len(rest) {
            len += escape;
            escaped = true;
        } else if let Some(ch) = rest.chars().next().filter(|&ch| is_css_ident_char(ch)) {
            len += ch.len_utf8();
        } else {
            return (len, escaped);
        }
    }
}

/// How a string token at the start of CSS text ends (CSS Syntax 3 §4.3.5,
/// "consume a string token").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CssString {
    /// Closed by its quote, spanning this many bytes.
    Closed(usize),
    /// A bad string: an unescaped newline ends it, and scanning resumes at
    /// that newline, this many bytes in.
    Bad(usize),
    /// The input ends inside the string.
    Unterminated,
}

fn css_string(text: &str) -> CssString {
    let Some(quote) = text.chars().next() else {
        return CssString::Unterminated;
    };
    let mut at = quote.len_utf8();
    while let Some(ch) = text[at..].chars().next() {
        match ch {
            '\\' => {
                let after = &text[at + 1..];
                at += 1 + if after.starts_with("\r\n") {
                    2
                } else if after.starts_with(['\n', '\r', '\x0c']) {
                    1
                } else {
                    css_escape_len(&text[at..]).map_or(0, |escape| escape - 1)
                };
            }
            '\n' | '\r' | '\x0c' => return CssString::Bad(at),
            _ if ch == quote => return CssString::Closed(at + 1),
            _ => at += ch.len_utf8(),
        }
    }
    CssString::Unterminated
}

/// The bytes of whitespace and comments at the start of `text`.
fn css_whitespace_and_comments_len(text: &str) -> usize {
    let mut len = 0;
    loop {
        let rest = &text[len..];
        let trimmed = rest.trim_start_matches([' ', '\t', '\n', '\r', '\x0c']);
        len += rest.len() - trimmed.len();
        match trimmed.strip_prefix("/*") {
            Some(comment) => len += comment.find("*/").map_or(trimmed.len(), |end| end + 4),
            None => return len,
        }
    }
}

/// A CSS function token whose name contains an escape, such as `v\61 r(`.
pub struct EscapedCssFunction {
    pub name: String,
    pub arguments: usize,
}

/// Function tokens with escaped names, which tree-sitter-css splits into separate nodes.
pub fn escaped_css_functions(text: &str) -> Vec<EscapedCssFunction> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(rest) = text.get(at..).filter(|rest| !rest.is_empty()) {
        if let Some(comment) = rest.strip_prefix("/*") {
            at += comment.find("*/").map_or(rest.len(), |end| end + 4);
        } else if rest.starts_with(['"', '\'']) {
            at += match css_string(rest) {
                CssString::Closed(len) | CssString::Bad(len) => len,
                CssString::Unterminated => rest.len(),
            };
        } else if let Some(name) = rest.strip_prefix(['#', '@']) {
            // A hash token or at-keyword: its name is part of the token, so
            // `#v\61 r(` is no function.
            at += 1 + css_ident_len(name).0;
        } else if rest.starts_with(|ch: char| ch.is_ascii_digit()) {
            at += 1 + css_ident_len(&rest[1..]).0;
        } else if let (len @ 1.., escaped) = css_ident_len(rest) {
            at += len;
            if escaped && text[at..].starts_with('(') {
                out.push(EscapedCssFunction {
                    name: css_unescape(&rest[..len]),
                    arguments: at + 1,
                });
            }
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    out
}

/// The leading custom property name in a `var()` argument list starting at `arguments`.
pub fn css_var_argument(text: &str, arguments: usize) -> Option<(usize, usize)> {
    let start = arguments + css_whitespace_and_comments_len(text.get(arguments..)?);
    let (len, _) = css_ident_len(&text[start..]);
    css_unescape(&text[start..start + len])
        .starts_with("--")
        .then_some((start, start + len))
}

/// The content range of a `url()` argument list starting at `arguments`, inside any quotes.
pub fn css_url_argument(text: &str, arguments: usize) -> Option<(usize, usize)> {
    let rest = text.get(arguments..)?;
    let start = arguments + rest.len() - rest.trim_start().len();
    let body = &text[start..];
    let (start, end) = if body.starts_with(['"', '\'']) {
        let CssString::Closed(len) = css_string(body) else {
            return None;
        };
        (start + 1, start + len - 1)
    } else {
        (start, start + css_unquoted_url_len(body))
    };
    (end > start).then_some((start, end))
}

/// The bytes of an unquoted `url()` argument at the start of `text`, up to
/// the first unescaped `)` or whitespace. An escape's whitespace terminator
/// belongs to the escape (CSS Syntax 3 §4.3.6, "consume a url token").
fn css_unquoted_url_len(text: &str) -> usize {
    let mut len = 0;
    loop {
        let rest = &text[len..];
        if let Some(escape) = css_escape_len(rest) {
            len += escape;
        } else {
            match rest.chars().next() {
                Some(ch) if ch != ')' && !ch.is_ascii_whitespace() => len += ch.len_utf8(),
                _ => return len,
            }
        }
    }
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

pub fn open_hex_escape(text: &str, skip: usize) -> bool {
    text.starts_with('\\')
        && text[1..].starts_with(|ch: char| ch.is_ascii_hexdigit())
        && !text[..skip].ends_with(|ch: char| ch.is_ascii_whitespace())
}

pub fn css_fragment_start(text: &str) -> Option<usize> {
    if text.starts_with('#') {
        return Some(1);
    }
    let rest = text.strip_prefix('\\')?;
    let hex = rest
        .bytes()
        .take(6)
        .take_while(u8::is_ascii_hexdigit)
        .count();
    if hex == 0 {
        return rest.starts_with('#').then_some(2);
    }
    let end = 1 + hex;
    (u32::from_str_radix(&rest[..hex], 16).ok()? == 0x23)
        .then(|| end + usize::from(text[end..].starts_with(|ch: char| ch.is_ascii_whitespace())))
}

pub fn fragment_start(reference: &str, css: bool) -> Option<usize> {
    let (decoded, offsets) = xml_unescape_mapped(reference);
    let skip = if css {
        css_fragment_start(&decoded)?
    } else {
        decoded.starts_with('#').then_some(1)?
    };
    offsets.map_or(Some(skip), |offsets| offsets.get(skip).copied())
}

pub fn fragment_id(fragment: &str, css: bool) -> String {
    let decoded = xml_unescape(fragment);
    if css {
        css_fragment_id(&decoded)
    } else {
        percent_decode(&decoded)
    }
}

pub fn css_fragment_id(fragment: &str) -> String {
    percent_decode(&css_unescape(fragment))
}

pub fn percent_decode(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escaped {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

pub fn xml_tokens(text: &str) -> Vec<(String, usize, usize)> {
    let (decoded, offsets) = xml_unescape_mapped(text);
    let source = |index: usize| {
        offsets
            .as_ref()
            .map_or(Some(index), |offsets| offsets.get(index).copied())
    };
    let mut tokens = Vec::new();
    let mut start = None;
    let ends = decoded
        .char_indices()
        .chain(std::iter::once((decoded.len(), ' ')));
    for (index, ch) in ends {
        if !is_xml_whitespace(ch) {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take()
            && let (Some(raw_start), Some(raw_end)) = (source(begin), source(index))
        {
            tokens.push((decoded[begin..index].to_owned(), raw_start, raw_end));
        }
    }
    tokens
}

pub fn xml_unescape(text: &str) -> String {
    xml_unescape_mapped(text).0
}

pub fn xml_unescape_mapped(text: &str) -> (String, Option<Vec<usize>>) {
    if !text.contains('&') {
        return (text.to_owned(), None);
    }
    let mut out = String::with_capacity(text.len());
    let mut offsets = Vec::with_capacity(text.len() + 1);
    let mut rest = text;
    let mut consumed = 0;
    let copy = |out: &mut String, offsets: &mut Vec<usize>, chunk: &str, at: usize| {
        out.push_str(chunk);
        offsets.extend(at..at + chunk.len());
    };
    while let Some(amp) = rest.find('&') {
        copy(&mut out, &mut offsets, &rest[..amp], consumed);
        consumed += amp;
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
            offsets.extend(std::iter::repeat_n(consumed, ch.len_utf8()));
            consumed += semi + 2;
            rest = &after[semi + 1..];
        } else {
            copy(&mut out, &mut offsets, "&", consumed);
            consumed += 1;
            rest = after;
        }
    }
    copy(&mut out, &mut offsets, rest, consumed);
    offsets.push(text.len());
    (out, Some(offsets))
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
    fn escaped_function_tokens_skip_comments_strings_and_numbers() {
        let text = concat!(
            "/* v\\61 r(a) */ 'u\\72l(b)' 1\\76 ar(c) var(d) v\\\nr(e) ",
            "u\\rl(f) v\\61\r\nr(g) \\75 RL(h) ",
        );
        let functions: Vec<_> = escaped_css_functions(text)
            .into_iter()
            .map(|function| {
                (
                    function.name,
                    &text[function.arguments..=function.arguments],
                )
            })
            .collect();
        assert_eq!(
            functions,
            [
                ("url".to_owned(), "f"),
                ("var".to_owned(), "g"),
                ("uRL".to_owned(), "h"),
            ]
        );
        assert_eq!(escaped_css_functions("'v\\61 r(unterminated").len(), 0);
    }

    #[test]
    fn strings_consume_whole_escapes_and_recover_at_newlines() {
        let names = |text: &str| -> Vec<String> {
            escaped_css_functions(text)
                .into_iter()
                .map(|function| function.name)
                .collect()
        };
        assert_eq!(names("'\\61\n v\\61 r(' u\\72l(a)"), ["url"]);
        assert_eq!(names("'a\\\r\n v\\61 r(' u\\72l(a)"), ["url"]);
        assert_eq!(names("content:'broken\nb: u\\72l(b.png)"), ["url"]);
        assert_eq!(css_string("'a\\\r\nb'"), CssString::Closed(7));
        assert_eq!(css_string("'ab\nc'"), CssString::Bad(3));
        assert_eq!(css_string("'ab"), CssString::Unterminated);
    }

    #[test]
    fn hash_and_at_keyword_names_are_no_functions() {
        let text = "--raw: #v\\61 r(--c) @v\\61 r(--d) v\\61 r(--e)";
        let arguments: Vec<_> = escaped_css_functions(text)
            .into_iter()
            .map(|function| &text[function.arguments..function.arguments + 3])
            .collect();
        assert_eq!(arguments, ["--e"]);
    }

    #[test]
    fn var_and_url_arguments_name_their_content() {
        let var = "v\\61 r( --a\\62 , red)";
        let (start, end) = css_var_argument(var, 7).unwrap_or_default();
        assert_eq!(&var[start..end], "--a\\62 ");
        assert_eq!(css_var_argument("v\\61 r(red)", 7), None);
        let var = "v\\61 r(/* theme */ --c)";
        let (start, end) = css_var_argument(var, 7).unwrap_or_default();
        assert_eq!(&var[start..end], "--c");

        let url = "u\\72l( 'x y' )";
        let (start, end) = css_url_argument(url, 6).unwrap_or_default();
        assert_eq!(&url[start..end], "x y");
        let url = "u\\72l(a.svg#b )";
        let (start, end) = css_url_argument(url, 6).unwrap_or_default();
        assert_eq!(&url[start..end], "a.svg#b");
        assert_eq!(css_url_argument("u\\72l()", 6), None);
        assert_eq!(css_url_argument("u\\72l('open)", 6), None);
        for (url, argument) in [
            ("u\\72l(#gr\\61 d)", "#gr\\61 d"),
            ("u\\72l(a\\20 b.svg )", "a\\20 b.svg"),
            ("u\\72l(a\\)b)", "a\\)b"),
        ] {
            let (start, end) = css_url_argument(url, 6).unwrap_or_default();
            assert_eq!(&url[start..end], argument, "{url}");
        }
    }

    #[test]
    fn xml_escape_and_unescape_attribute_text() {
        assert_eq!(xml_escape_attribute("a&b\"c'<"), "a&amp;b&quot;c&apos;&lt;");
        assert_eq!(xml_unescape("a&amp;b&#65;&#x42;&bogus;&"), "a&bAB&bogus;&");
        let (decoded, offsets) = xml_unescape_mapped("a&amp;b&#xe9;&x");
        assert_eq!(decoded, "a&b\u{e9}&x");
        assert_eq!(offsets, Some(vec![0, 1, 6, 7, 7, 13, 14, 15]));
    }

    #[test]
    fn percent_decoding_keeps_invalid_sequences() {
        assert_eq!(percent_decode("a%2Eb"), "a.b");
        assert_eq!(percent_decode("%C3%A9t%C3%A9"), "\u{e9}t\u{e9}");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("a%zzb"), "a%zzb");
        assert_eq!(percent_decode("%FF"), "%FF");
    }

    #[test]
    fn fragment_starts_follow_xml_and_css_escapes() {
        assert_eq!(css_fragment_start("#g"), Some(1));
        assert_eq!(css_fragment_start("\\23 g"), Some(4));
        assert_eq!(css_fragment_start("\\000023g"), Some(7));
        assert_eq!(css_fragment_start("\\#g"), Some(2));
        assert_eq!(css_fragment_start("\\24 g"), None);
        assert_eq!(css_fragment_start("g"), None);
        assert_eq!(fragment_start("&#35;g", false), Some(5));
        assert_eq!(fragment_start("\\23 g", false), None);
        assert_eq!(fragment_start("&#92;23 g", true), Some(8));
        assert!(open_hex_escape("\\23g", 3));
        assert!(!open_hex_escape("\\23 g", 4));
        assert!(!open_hex_escape("\\#g", 2));
        assert!(!open_hex_escape("#g", 1));
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
