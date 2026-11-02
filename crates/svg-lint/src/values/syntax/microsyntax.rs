//! Whitespace, separators, numbers and string operations outside CSS.
//!
//! - XML 1.0 production `[3] S` (<https://www.w3.org/TR/xml/#NT-S>).
//! - The SVG 2 path data grammar (<https://svgwg.org/svg2-draft/paths.html#PathDataBNF>),
//!   where SVG 2 defines `wsp`, `comma_wsp`, `number`, `coordinate` and
//!   `flag` for every EBNF microsyntax. Its `wsp` includes U+000C FORM FEED,
//!   unlike XML `S`. The number scanners are greedy, as the grammar's
//!   processing rule requires, and return the unconsumed rest.
//! - Infra string operations (<https://infra.spec.whatwg.org/#strings>) that
//!   HTML microsyntaxes are written in: ASCII whitespace, ASCII
//!   case-insensitive matching, stripping, and splitting on ASCII whitespace
//!   or on commas.

/// XML 1.0 `S` code points: U+0020, U+0009, U+000D, U+000A.
#[must_use]
pub const fn is_xml_whitespace(c: char) -> bool {
    let _ = c;
    false
}

/// SVG 2 `wsp`: U+0009, U+0020, U+000A, U+000C, U+000D.
#[must_use]
pub const fn is_svg_wsp(c: char) -> bool {
    let _ = c;
    false
}

/// Infra ASCII whitespace: U+0009, U+000A, U+000C, U+000D, U+0020.
#[must_use]
pub const fn is_ascii_whitespace(c: char) -> bool {
    let _ = c;
    false
}

/// `s` without leading `wsp*`.
#[must_use]
pub const fn skip_wsp(s: &str) -> &str {
    s
}

/// `s` without leading and trailing `wsp*`.
#[must_use]
pub const fn trim_wsp(s: &str) -> &str {
    s
}

/// The rest of `s` after one `comma_wsp` at its start, consumed greedily, or
/// `None` when `s` does not start with one.
#[must_use]
pub const fn strip_comma_wsp(s: &str) -> Option<&str> {
    let _ = s;
    None
}

/// The rest of `s` after an optional `comma_wsp?` at its start.
#[must_use]
pub fn skip_comma_wsp(s: &str) -> &str {
    strip_comma_wsp(s).unwrap_or(s)
}

/// `s` without one trailing `comma_wsp`, for the list attributes that
/// allow "a single trailing comma-wsp".
#[must_use]
pub const fn strip_trailing_comma_wsp(s: &str) -> &str {
    s
}

/// The items of a list separated by `comma_wsp`; see [`split_comma_wsp`].
#[derive(Clone, Debug)]
pub struct CommaWspItems<'a> {
    rest: &'a str,
    expect_item: bool,
}

/// Split `s` into the items between `comma_wsp` separators.
///
/// Leading and trailing `wsp` are skipped. Every comma separates two items,
/// so a leading, doubled or trailing comma yields an empty item: `"1,,2"`
/// gives `"1"`, `""`, `"2"` and `"1,"` gives `"1"`, `""`. A whitespace-only
/// string has no items. Callers that allow a single trailing `comma_wsp`
/// strip it first with [`strip_trailing_comma_wsp`].
#[must_use]
pub const fn split_comma_wsp(s: &str) -> CommaWspItems<'_> {
    CommaWspItems {
        rest: s,
        expect_item: false,
    }
}

impl<'a> Iterator for CommaWspItems<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let _ = (self.rest, self.expect_item);
        None
    }
}

/// Scan the SVG 2 `number` production (unsigned) at the start of `s`,
/// greedily: its value and the rest of `s`, or `None` when `s` does not
/// start with a `number`.
#[must_use]
pub fn scan_number(s: &str) -> Option<(f64, &str)> {
    let _ = s;
    None
}

/// Scan the SVG 2 `coordinate` production (`sign? number`) at the start of
/// `s`.
#[must_use]
pub fn scan_coordinate(s: &str) -> Option<(f64, &str)> {
    let _ = s;
    None
}

/// Scan the SVG 2 `flag` production (`"0" | "1"`) at the start of `s`.
#[must_use]
pub fn scan_flag(s: &str) -> Option<(bool, &str)> {
    let _ = s;
    None
}

/// Infra "ASCII case-insensitive match".
#[must_use]
pub fn eq_ascii_case_insensitive(a: &str, b: &str) -> bool {
    let _ = (a, b);
    false
}

/// The entry of `list` that `s` matches ASCII case-insensitively.
#[must_use]
pub fn find_ascii_case_insensitive<'k>(s: &str, list: &[&'k str]) -> Option<&'k str> {
    let _ = (s, list);
    None
}

/// Infra "strip leading and trailing ASCII whitespace".
#[must_use]
pub const fn strip_ascii_whitespace(s: &str) -> &str {
    s
}

/// Infra "split a string on ASCII whitespace".
pub fn split_on_ascii_whitespace(s: &str) -> impl Iterator<Item = &str> {
    let _ = s;
    std::iter::empty()
}

/// Infra "split a string on commas": each token is stripped of ASCII
/// whitespace and may be empty; a final comma does not add a token.
pub fn split_on_commas(s: &str) -> impl Iterator<Item = &str> {
    let _ = s;
    std::iter::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(s: &str) -> Vec<&str> {
        split_comma_wsp(s).collect()
    }

    fn numbers(s: &str) -> Option<(f64, &str)> {
        scan_number(s)
    }

    // Claim: "[3] S ::= (#x20 | #x9 | #xD | #xA)+"
    // https://www.w3.org/TR/xml/#NT-S
    #[test]
    fn xml_whitespace() {
        for c in [' ', '\t', '\r', '\n'] {
            assert!(is_xml_whitespace(c), "{c:?}");
        }
        for c in ['\x0C', '\x0B', '\u{A0}', '\u{2028}', 'a'] {
            assert!(!is_xml_whitespace(c), "{c:?}");
        }
    }

    // Claim: "wsp::= (#x9 | #x20 | #xA | #xC | #xD)"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn svg_wsp() {
        for c in ['\t', ' ', '\n', '\x0C', '\r'] {
            assert!(is_svg_wsp(c), "{c:?}");
        }
        for c in ['\x0B', '\u{A0}', ',', 'a'] {
            assert!(!is_svg_wsp(c), "{c:?}");
        }
        assert_eq!(skip_wsp(" \t\x0C1 "), "1 ");
        assert_eq!(trim_wsp("\n 1 2 \r"), "1 2");
        assert_eq!(trim_wsp("\u{A0}1"), "\u{A0}1");
    }

    // Claim: "comma_wsp::= (wsp+ "," ? wsp*) | ("," wsp*)"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn comma_wsp_production() {
        assert_eq!(strip_comma_wsp(" 1"), Some("1"));
        assert_eq!(strip_comma_wsp(",1"), Some("1"));
        assert_eq!(strip_comma_wsp(" , 1"), Some("1"));
        assert_eq!(strip_comma_wsp(",\t\n1"), Some("1"));
        assert_eq!(strip_comma_wsp(" ,,1"), Some(",1"));
        assert_eq!(strip_comma_wsp("1"), None);
        assert_eq!(strip_comma_wsp(""), None);
        assert_eq!(skip_comma_wsp("1"), "1");
        assert_eq!(skip_comma_wsp(" ,1"), "1");
    }

    // Claim: "Allow a single trailing comma-wsp in various list-typed
    // attributes." https://svgwg.org/svg2-draft/changes.html#types
    #[test]
    fn trailing_comma_wsp() {
        assert_eq!(strip_trailing_comma_wsp("1 2, "), "1 2");
        assert_eq!(strip_trailing_comma_wsp("1 2 "), "1 2");
        assert_eq!(strip_trailing_comma_wsp("1,"), "1");
        assert_eq!(strip_trailing_comma_wsp("1,,"), "1,");
        assert_eq!(strip_trailing_comma_wsp("1"), "1");
    }

    // Claim: "comma_wsp::= (wsp+ "," ? wsp*) | ("," wsp*)" as the separator of
    // sequences such as "coordinate_sequence::= coordinate | (coordinate
    // comma_wsp? coordinate_sequence)"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn comma_wsp_list_items() {
        assert_eq!(items("1, 2 3,4"), ["1", "2", "3", "4"]);
        assert_eq!(items("  1 \t,\n 2  "), ["1", "2"]);
        assert_eq!(items("1,,2"), ["1", "", "2"]);
        assert_eq!(items("1 , , 2"), ["1", "", "2"]);
        assert_eq!(items(",1"), ["", "1"]);
        assert_eq!(items("1,"), ["1", ""]);
        assert_eq!(items(","), ["", ""]);
        assert_eq!(items(""), Vec::<&str>::new());
        assert_eq!(items(" \n "), Vec::<&str>::new());
        assert_eq!(items("a\u{A0}b"), ["a\u{A0}b"]);
        assert_eq!(items(strip_trailing_comma_wsp("1, 2, ")), ["1", "2"]);
    }

    // Claim: "number::= fractional-constant exponent?" with
    // "fractional-constant::= (digit* "." digit+) | digit+" and "exponent::=
    // ("e" | "E") sign? digit+"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn number_production() {
        assert_eq!(numbers("10"), Some((10.0, "")));
        assert_eq!(numbers(".5"), Some((0.5, "")));
        assert_eq!(numbers("0.25x"), Some((0.25, "x")));
        assert_eq!(numbers("1e3"), Some((1000.0, "")));
        assert_eq!(numbers("1E-2"), Some((0.01, "")));
        assert_eq!(numbers("2.5e+1 "), Some((25.0, " ")));
        // The exponent needs digits; the "e" is left over.
        assert_eq!(numbers("1e"), Some((1.0, "e")));
        assert_eq!(numbers("1e+"), Some((1.0, "e+")));
        // Unsigned: the sign belongs to `coordinate`.
        assert_eq!(numbers("-1"), None);
        assert_eq!(numbers("+1"), None);
        assert_eq!(numbers("."), None);
        assert_eq!(numbers("e1"), None);
        assert_eq!(numbers(""), None);
        assert_eq!(numbers(" 1"), None);
    }

    // Claim: "The processing of the EBNF must consume as much of a given EBNF
    // production as possible, stopping at the point when a character is
    // encountered which no longer satisfies the production." and "for the
    // string "M 0.6.5", the first coordinate of the "moveto" consumes the
    // characters "0.6" and stops upon encountering the second decimal point"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    // Claim: "SVG 2 harmonizes number parsing with CSS [css-syntax-3],
    // disallowing the relaxed grammar for numbers. ... Authors and authoring
    // tools must not use the disallowed number format."
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn number_scanning_is_greedy() {
        assert_eq!(numbers("0.6.5"), Some((0.6, ".5")));
        assert_eq!(numbers("100-200"), Some((100.0, "-200")));
        assert_eq!(numbers("1.5e2.5"), Some((150.0, ".5")));
        // A trailing decimal point is not part of the number.
        assert_eq!(numbers("23."), Some((23.0, ".")));
        assert_eq!(numbers("23.e1"), Some((23.0, ".e1")));
    }

    // Claim: "coordinate::= sign? number" and "sign::= "+"|"-""
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn coordinate_production() {
        assert_eq!(scan_coordinate("-200"), Some((-200.0, "")));
        assert_eq!(scan_coordinate("+.5,"), Some((0.5, ",")));
        assert_eq!(scan_coordinate("3"), Some((3.0, "")));
        assert_eq!(scan_coordinate("-"), None);
        assert_eq!(scan_coordinate("--1"), None);
        assert_eq!(scan_coordinate("- 1"), None);
    }

    // Claim: "flag::= ("0" | "1")"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn flag_production() {
        assert_eq!(scan_flag("0"), Some((false, "")));
        assert_eq!(scan_flag("10"), Some((true, "0")));
        assert_eq!(scan_flag("2"), None);
        assert_eq!(scan_flag(""), None);
        assert_eq!(scan_flag(" 1"), None);
    }

    // Claim: "ASCII whitespace is U+0009 TAB, U+000A LF, U+000C FF, U+000D CR,
    // or U+0020 SPACE." https://infra.spec.whatwg.org/#ascii-whitespace
    // Claim: "To strip leading and trailing ASCII whitespace from a string,
    // remove all ASCII whitespace that are at the start or the end of the
    // string."
    // https://infra.spec.whatwg.org/#strip-leading-and-trailing-ascii-whitespace
    #[test]
    fn ascii_whitespace() {
        for c in ['\t', '\n', '\x0C', '\r', ' '] {
            assert!(is_ascii_whitespace(c), "{c:?}");
        }
        for c in ['\x0B', '\u{A0}', '\u{3000}'] {
            assert!(!is_ascii_whitespace(c), "{c:?}");
        }
        assert_eq!(strip_ascii_whitespace(" \x0Ca b\t"), "a b");
        assert_eq!(strip_ascii_whitespace("\u{A0}a"), "\u{A0}a");
    }

    // Claim: "A string A is an ASCII case-insensitive match for a string B, if
    // the ASCII lowercase of A is the ASCII lowercase of B."
    // https://infra.spec.whatwg.org/#ascii-case-insensitive
    #[test]
    fn ascii_case_insensitive_match() {
        assert!(eq_ascii_case_insensitive("Auto", "aUTO"));
        assert!(eq_ascii_case_insensitive("", ""));
        assert!(!eq_ascii_case_insensitive("\u{212A}", "k"));
        assert!(!eq_ascii_case_insensitive("\u{E9}", "\u{C9}"));
        assert!(!eq_ascii_case_insensitive("auto", "autos"));
        assert_eq!(
            find_ascii_case_insensitive("MEET", &["meet", "slice"]),
            Some("meet")
        );
        assert_eq!(
            find_ascii_case_insensitive("none", &["meet", "slice"]),
            None
        );
    }

    // Claim: "To split a string input on ASCII whitespace: ... Skip ASCII
    // whitespace within input given position. While position is not past the
    // end of input: Let token be the result of collecting a sequence of code
    // points that are not ASCII whitespace from input, given position. Append
    // token to tokens. Skip ASCII whitespace within input given position.
    // Return tokens." https://infra.spec.whatwg.org/#split-on-ascii-whitespace
    #[test]
    fn split_on_ascii_whitespace_tokens() {
        let tokens: Vec<_> = split_on_ascii_whitespace(" a\x0Cb\r\n c ").collect();
        assert_eq!(tokens, ["a", "b", "c"]);
        assert_eq!(split_on_ascii_whitespace("  ").count(), 0);
        let nbsp: Vec<_> = split_on_ascii_whitespace("a\u{A0}b").collect();
        assert_eq!(nbsp, ["a\u{A0}b"]);
    }

    // Claim: "While position is not past the end of input: Let token be the
    // result of collecting a sequence of code points that are not U+002C (,)
    // from input, given position. token might be the empty string. Strip
    // leading and trailing ASCII whitespace from token. Append token to
    // tokens. If position is not past the end of input: Assert: the code point
    // at position within input is U+002C (,). Advance position by 1."
    // https://infra.spec.whatwg.org/#split-on-commas
    #[test]
    fn split_on_commas_tokens() {
        let split = |s| split_on_commas(s).collect::<Vec<_>>();
        assert_eq!(split("a, b ,c"), ["a", "b", "c"]);
        assert_eq!(split("a,,b"), ["a", "", "b"]);
        assert_eq!(split(",a"), ["", "a"]);
        assert_eq!(split("a,"), ["a"]);
        assert_eq!(split(","), [""]);
        assert_eq!(split(" "), [""]);
        assert_eq!(split(""), Vec::<&str>::new());
    }
}
