//! CSS Syntax 3 tokenization.
//!
//! [`tokenize`] turns a string into the token stream of CSS Syntax 3 §4
//! (<https://drafts.csswg.org/css-syntax-3/#tokenization>), one token at a
//! time. The §3.3 input preprocessing
//! (<https://drafts.csswg.org/css-syntax-3/#input-preprocessing>) is applied
//! on the fly rather than as a separate pass: CR, FF and CR LF count as one
//! newline, and NULL reads as U+FFFD. Token values borrow from the input and
//! only allocate when an escape or a NULL changes the text.
//!
//! The `<unicode-range-token>` is not produced: the tokenizer only emits it
//! for the `unicode-range` descriptor (§4.3.14), which no attribute uses.
//! Comments are consumed between tokens (§4.3.2) and never surface.

use std::borrow::Cow;

/// U+FFFD REPLACEMENT CHARACTER.
const REPLACEMENT: char = '\u{FFFD}';

/// The sign character of a numeric token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    /// U+002B PLUS SIGN.
    Plus,
    /// U+002D HYPHEN-MINUS.
    Minus,
}

/// The type flag of a `<number-token>` or `<dimension-token>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberType {
    /// Written with digits only: no `.` and no exponent.
    Integer,
    /// Written with a fractional part or an exponent.
    Number,
}

/// The numeric part of a `<number-token>`, `<percentage-token>` or
/// `<dimension-token>`, as "consume a number" (§4.3.13) returns it.
///
/// A `<percentage-token>` has no type flag in the spec; `kind` still records
/// how its number was written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Number {
    /// The numeric value.
    pub value: f64,
    /// The type flag.
    pub kind: NumberType,
    /// The sign character, when one was written.
    pub sign: Option<Sign>,
}

impl Number {
    /// Whether the type flag is "integer".
    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.kind == NumberType::Integer
    }
}

/// The type flag of a `<hash-token>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashType {
    /// The value would start an ident sequence (a valid ID selector).
    Id,
    /// Any other value, such as `123` in `#123`.
    Unrestricted,
}

/// A CSS token (CSS Syntax 3 §4).
#[derive(Clone, Debug, PartialEq)]
pub enum Token<'a> {
    /// `<ident-token>`.
    Ident(Cow<'a, str>),
    /// `<function-token>`: the name, without the `(`.
    Function(Cow<'a, str>),
    /// `<at-keyword-token>`: the name, without the `@`.
    AtKeyword(Cow<'a, str>),
    /// `<hash-token>`: the value, without the `#`.
    Hash {
        /// The value.
        value: Cow<'a, str>,
        /// The type flag.
        kind: HashType,
    },
    /// `<string-token>`: the value, without the quotes.
    String(Cow<'a, str>),
    /// `<bad-string-token>`: a string broken by an unescaped newline.
    BadString,
    /// `<url-token>`: the value of an unquoted `url(...)`.
    Url(Cow<'a, str>),
    /// `<bad-url-token>`.
    BadUrl,
    /// `<delim-token>`.
    Delim(char),
    /// `<number-token>`.
    Number(Number),
    /// `<percentage-token>`.
    Percentage(Number),
    /// `<dimension-token>`.
    Dimension {
        /// The numeric part.
        number: Number,
        /// The unit, as written (units match ASCII case-insensitively).
        unit: Cow<'a, str>,
    },
    /// `<whitespace-token>`.
    Whitespace,
    /// `<CDO-token>`: `<!--`.
    Cdo,
    /// `<CDC-token>`: `-->`.
    Cdc,
    /// `<colon-token>`.
    Colon,
    /// `<semicolon-token>`.
    Semicolon,
    /// `<comma-token>`.
    Comma,
    /// `<[-token>`.
    OpenSquare,
    /// `<]-token>`.
    CloseSquare,
    /// `<(-token>`.
    OpenParen,
    /// `<)-token>`.
    CloseParen,
    /// `<{-token>`.
    OpenCurly,
    /// `<}-token>`.
    CloseCurly,
}

/// The tokens of a string, produced on demand by "consume a token"
/// (§4.3.1). Iteration ends at the `<EOF-token>`.
#[derive(Clone, Debug)]
pub struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

/// Tokenize `input` (§4 "Tokenization").
#[must_use]
pub const fn tokenize(input: &str) -> Tokenizer<'_> {
    Tokenizer { input, pos: 0 }
}

impl<'a> Tokenizer<'a> {
    /// The byte offset of the next unconsumed code point.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.pos
    }

    /// The unconsumed input.
    fn rest(&self) -> &'a str {
        &self.input[self.pos..]
    }

    /// The next input code point as preprocessing delivers it (NULL reads
    /// as U+FFFD), or `None` at EOF.
    fn peek(&self) -> Option<char> {
        self.rest().chars().next().map(preprocess)
    }

    /// The code point after the next one, preprocessed (a CR LF pair is one
    /// code point).
    fn peek_second(&self) -> Option<char> {
        let rest = self.rest();
        let first = if rest.starts_with("\r\n") {
            2
        } else {
            rest.chars().next()?.len_utf8()
        };
        rest[first..].chars().next().map(preprocess)
    }

    /// Consume the next input code point. CR LF counts as one newline.
    fn bump(&mut self) {
        let rest = self.rest();
        if rest.starts_with("\r\n") {
            self.pos += 2;
        } else if let Some(c) = rest.chars().next() {
            self.pos += c.len_utf8();
        }
    }

    fn consume_whitespace(&mut self) {
        while self.peek().is_some_and(is_whitespace) {
            self.bump();
        }
    }

    /// §4.3.2 "Consume comments".
    fn consume_comments(&mut self) {
        while let Some(body) = self.rest().strip_prefix("/*") {
            self.pos = body.find("*/").map_or(self.input.len(), |end| {
                self.input.len() - body.len() + end + 2
            });
        }
    }

    /// §4.3.7 "Consume an escaped code point", with the `\` already
    /// consumed.
    fn consume_escaped_code_point(&mut self) -> char {
        let Some(first) = self.peek() else {
            return REPLACEMENT;
        };
        if !first.is_ascii_hexdigit() {
            self.bump();
            return first;
        }
        let mut value = 0_u32;
        for _ in 0..6 {
            match self.peek().and_then(|c| c.to_digit(16)) {
                Some(digit) => {
                    value = value * 16 + digit;
                    self.bump();
                }
                None => break,
            }
        }
        if self.peek().is_some_and(is_whitespace) {
            self.bump();
        }
        if value == 0 {
            REPLACEMENT
        } else {
            // Surrogates and values past U+10FFFF are not `char`s.
            char::from_u32(value).unwrap_or(REPLACEMENT)
        }
    }

    /// §4.3.12 "Consume an ident sequence".
    fn consume_ident_sequence(&mut self) -> Cow<'a, str> {
        let mut value = Value::new(self.pos);
        loop {
            let at = self.pos;
            match self.peek() {
                Some(c) if is_ident_code_point(c) => {
                    self.bump();
                    value.literal(self.input, at, c);
                }
                Some('\\') if starts_with_valid_escape(self.rest()) => {
                    self.bump();
                    let c = self.consume_escaped_code_point();
                    value.decoded(self.input, at, c);
                }
                _ => return value.finish(self.input, at),
            }
        }
    }

    /// §4.3.13 "Consume a number".
    fn consume_number(&mut self) -> Number {
        let start = self.pos;
        let mut kind = NumberType::Integer;
        let sign = match self.peek() {
            Some('+') => Some(Sign::Plus),
            Some('-') => Some(Sign::Minus),
            _ => None,
        };
        if sign.is_some() {
            self.bump();
        }
        self.consume_digits();
        if self.peek() == Some('.') && self.peek_second().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            self.consume_digits();
            kind = NumberType::Number;
        }
        let rest = self.rest().as_bytes();
        if let [b'e' | b'E', next, ..] = rest {
            let digit_at = usize::from(matches!(next, b'+' | b'-')) + 1;
            if rest.get(digit_at).is_some_and(u8::is_ascii_digit) {
                self.pos += digit_at;
                self.consume_digits();
                kind = NumberType::Number;
            }
        }
        // The consumed text is a valid Rust float literal: an optional sign,
        // digits with an optional fraction, an optional exponent.
        let value = self.input[start..self.pos].parse().unwrap_or_default();
        Number { value, kind, sign }
    }

    fn consume_digits(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
    }

    /// §4.3.3 "Consume a numeric token".
    fn consume_numeric_token(&mut self) -> Token<'a> {
        let number = self.consume_number();
        if would_start_ident_sequence(self.rest()) {
            let unit = self.consume_ident_sequence();
            Token::Dimension { number, unit }
        } else if self.peek() == Some('%') {
            self.bump();
            Token::Percentage(number)
        } else {
            Token::Number(number)
        }
    }

    /// §4.3.4 "Consume an ident-like token".
    fn consume_ident_like_token(&mut self) -> Token<'a> {
        let name = self.consume_ident_sequence();
        if self.peek() != Some('(') {
            return Token::Ident(name);
        }
        self.bump();
        if !name.eq_ignore_ascii_case("url") {
            return Token::Function(name);
        }
        while self.peek().is_some_and(is_whitespace)
            && self.peek_second().is_some_and(is_whitespace)
        {
            self.bump();
        }
        let quote = |c: Option<char>| matches!(c, Some('"' | '\''));
        let next = self.peek();
        if quote(next) || (next.is_some_and(is_whitespace) && quote(self.peek_second())) {
            Token::Function(name)
        } else {
            self.consume_url_token()
        }
    }

    /// §4.3.5 "Consume a string token", with the opening quote consumed.
    fn consume_string_token(&mut self, ending: char) -> Token<'a> {
        let mut value = Value::new(self.pos);
        loop {
            let at = self.pos;
            match self.peek() {
                None => return Token::String(value.finish(self.input, at)),
                Some(c) if c == ending => {
                    self.bump();
                    return Token::String(value.finish(self.input, at));
                }
                // Reconsume: the newline starts the next token.
                Some(c) if is_newline(c) => return Token::BadString,
                Some('\\') => {
                    self.bump();
                    match self.peek() {
                        None => value.skip(self.input, at),
                        Some(c) if is_newline(c) => {
                            self.bump();
                            value.skip(self.input, at);
                        }
                        Some(_) => {
                            let c = self.consume_escaped_code_point();
                            value.decoded(self.input, at, c);
                        }
                    }
                }
                Some(c) => {
                    self.bump();
                    value.literal(self.input, at, c);
                }
            }
        }
    }

    /// §4.3.6 "Consume a url token", with `url(` consumed.
    fn consume_url_token(&mut self) -> Token<'a> {
        self.consume_whitespace();
        let mut value = Value::new(self.pos);
        loop {
            let at = self.pos;
            match self.peek() {
                None => return Token::Url(value.finish(self.input, at)),
                Some(')') => {
                    self.bump();
                    return Token::Url(value.finish(self.input, at));
                }
                Some(c) if is_whitespace(c) => {
                    self.consume_whitespace();
                    match self.peek() {
                        None => return Token::Url(value.finish(self.input, at)),
                        Some(')') => {
                            self.bump();
                            return Token::Url(value.finish(self.input, at));
                        }
                        Some(_) => {
                            self.consume_remnants_of_bad_url();
                            return Token::BadUrl;
                        }
                    }
                }
                Some(c) if matches!(c, '"' | '\'' | '(') || is_non_printable_code_point(c) => {
                    self.bump();
                    self.consume_remnants_of_bad_url();
                    return Token::BadUrl;
                }
                Some('\\') => {
                    let valid = starts_with_valid_escape(self.rest());
                    self.bump();
                    if !valid {
                        self.consume_remnants_of_bad_url();
                        return Token::BadUrl;
                    }
                    let c = self.consume_escaped_code_point();
                    value.decoded(self.input, at, c);
                }
                Some(c) => {
                    self.bump();
                    value.literal(self.input, at, c);
                }
            }
        }
    }

    /// §4.3.15 "Consume the remnants of a bad url".
    fn consume_remnants_of_bad_url(&mut self) {
        loop {
            match self.peek() {
                None => return,
                Some(')') => {
                    self.bump();
                    return;
                }
                Some('\\') if starts_with_valid_escape(self.rest()) => {
                    self.bump();
                    self.consume_escaped_code_point();
                }
                Some(_) => self.bump(),
            }
        }
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    /// §4.3.1 "Consume a token" (with "unicode ranges allowed" false).
    fn next(&mut self) -> Option<Token<'a>> {
        self.consume_comments();
        let c = self.peek()?;
        let rest = self.rest();
        let token = match c {
            c if is_whitespace(c) => {
                self.consume_whitespace();
                Token::Whitespace
            }
            '"' | '\'' => {
                self.bump();
                self.consume_string_token(c)
            }
            '#' => {
                self.bump();
                let after = self.rest();
                if self.peek().is_some_and(is_ident_code_point) || starts_with_valid_escape(after) {
                    let kind = if would_start_ident_sequence(after) {
                        HashType::Id
                    } else {
                        HashType::Unrestricted
                    };
                    let value = self.consume_ident_sequence();
                    Token::Hash { value, kind }
                } else {
                    Token::Delim('#')
                }
            }
            '+' | '-' | '.' if would_start_number(rest) => self.consume_numeric_token(),
            '-' if rest.starts_with("-->") => {
                self.pos += 3;
                Token::Cdc
            }
            '-' if would_start_ident_sequence(rest) => self.consume_ident_like_token(),
            '<' if rest.starts_with("<!--") => {
                self.pos += 4;
                Token::Cdo
            }
            '@' if would_start_ident_sequence(&rest[1..]) => {
                self.bump();
                Token::AtKeyword(self.consume_ident_sequence())
            }
            '\\' if starts_with_valid_escape(rest) => self.consume_ident_like_token(),
            c if c.is_ascii_digit() => self.consume_numeric_token(),
            c if is_ident_start_code_point(c) => self.consume_ident_like_token(),
            _ => {
                self.bump();
                match c {
                    '(' => Token::OpenParen,
                    ')' => Token::CloseParen,
                    ',' => Token::Comma,
                    ':' => Token::Colon,
                    ';' => Token::Semicolon,
                    '[' => Token::OpenSquare,
                    ']' => Token::CloseSquare,
                    '{' => Token::OpenCurly,
                    '}' => Token::CloseCurly,
                    // Includes `#`, `+`, `-`, `.`, `<`, `@` and `\` when their
                    // longer forms did not apply.
                    other => Token::Delim(other),
                }
            }
        };
        Some(token)
    }
}

/// §3.3 preprocessing of one code point: NULL becomes U+FFFD. (CR and FF are
/// handled by [`is_newline`], and CR LF by the cursor.)
const fn preprocess(c: char) -> char {
    if c == '\0' { REPLACEMENT } else { c }
}

/// A token value under construction: borrowed from the input until an
/// escape, a NULL or a skipped code point forces a copy.
struct Value {
    start: usize,
    owned: Option<String>,
}

impl Value {
    const fn new(start: usize) -> Self {
        Self { start, owned: None }
    }

    /// Make the value owned, copying the input before `at`.
    fn own<'s>(&'s mut self, input: &str, at: usize) -> &'s mut String {
        let start = self.start;
        self.owned
            .get_or_insert_with(|| input[start..at].to_owned())
    }

    /// Append `c`, written literally at `at` (as NULL when it reads as
    /// U+FFFD).
    fn literal(&mut self, input: &str, at: usize, c: char) {
        if let Some(owned) = &mut self.owned {
            owned.push(c);
        } else if input.as_bytes()[at] == 0 {
            self.own(input, at).push(c);
        }
    }

    /// Append `c`, decoded from an escape that starts at `at`.
    fn decoded(&mut self, input: &str, at: usize, c: char) {
        self.own(input, at).push(c);
    }

    /// Drop the input from `at` up to the current position from the value.
    fn skip(&mut self, input: &str, at: usize) {
        self.own(input, at);
    }

    /// The finished value, which ends at `end` if still borrowed.
    fn finish(self, input: &str, end: usize) -> Cow<'_, str> {
        self.owned
            .map_or_else(|| Cow::Borrowed(&input[self.start..end]), Cow::Owned)
    }
}

/// A newline after preprocessing (§4.2 "newline", §3.3): LF, CR or FF.
#[must_use]
pub const fn is_newline(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\x0C')
}

/// "whitespace" (§4.2): a newline, U+0009 or U+0020.
#[must_use]
pub const fn is_whitespace(c: char) -> bool {
    is_newline(c) || matches!(c, '\t' | ' ')
}

/// "non-ASCII ident code point" (§4.2).
#[must_use]
pub const fn is_non_ascii_ident_code_point(c: char) -> bool {
    matches!(
        c,
        '\u{B7}'
            | '\u{C0}'..='\u{D6}'
            | '\u{D8}'..='\u{F6}'
            | '\u{F8}'..='\u{37D}'
            | '\u{37F}'..='\u{1FFF}'
            | '\u{200C}'
            | '\u{200D}'
            | '\u{203F}'
            | '\u{2040}'
            | '\u{2070}'..='\u{218F}'
            | '\u{2C00}'..='\u{2FEF}'
            | '\u{3001}'..='\u{D7FF}'
            | '\u{F900}'..='\u{FDCF}'
            | '\u{FDF0}'..='\u{FFFD}'
            | '\u{10000}'..
    )
}

/// "ident-start code point" (§4.2).
#[must_use]
pub const fn is_ident_start_code_point(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || is_non_ascii_ident_code_point(c)
}

/// "ident code point" (§4.2).
#[must_use]
pub const fn is_ident_code_point(c: char) -> bool {
    is_ident_start_code_point(c) || c.is_ascii_digit() || c == '-'
}

/// "non-printable code point" (§4.2).
#[must_use]
pub const fn is_non_printable_code_point(c: char) -> bool {
    matches!(c, '\0'..='\x08' | '\x0B' | '\x0E'..='\x1F' | '\x7F')
}

/// The first three code points of `stream`, preprocessed.
fn first_three(stream: &str) -> (Option<char>, Option<char>, Option<char>) {
    let mut chars = stream.chars().map(preprocess);
    (chars.next(), chars.next(), chars.next())
}

/// §4.3.8 on two explicit code points.
fn is_valid_escape(first: Option<char>, second: Option<char>) -> bool {
    first == Some('\\') && !second.is_some_and(is_newline)
}

/// "Check if two code points are a valid escape" (§4.3.8), applied to the
/// start of `stream`.
#[must_use]
pub fn starts_with_valid_escape(stream: &str) -> bool {
    let (first, second, _) = first_three(stream);
    is_valid_escape(first, second)
}

/// "Check if three code points would start an ident sequence" (§4.3.9),
/// applied to the start of `stream`.
#[must_use]
pub fn would_start_ident_sequence(stream: &str) -> bool {
    match first_three(stream) {
        (Some('-'), second, third) => {
            second.is_some_and(|c| c == '-' || is_ident_start_code_point(c))
                || is_valid_escape(second, third)
        }
        (Some('\\'), second, _) => is_valid_escape(Some('\\'), second),
        (Some(c), ..) => is_ident_start_code_point(c),
        (None, ..) => false,
    }
}

/// "Check if three code points would start a number" (§4.3.10), applied to
/// the start of `stream`.
#[must_use]
pub fn would_start_number(stream: &str) -> bool {
    let digit = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit());
    match first_three(stream) {
        (Some('+' | '-'), second, third) => digit(second) || (second == Some('.') && digit(third)),
        (Some('.'), second, _) => digit(second),
        (first, ..) => digit(first),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(input: &str) -> Vec<Token<'_>> {
        tokenize(input).collect()
    }

    fn ident(value: &str) -> Token<'_> {
        Token::Ident(Cow::Borrowed(value))
    }

    fn function(name: &str) -> Token<'_> {
        Token::Function(Cow::Borrowed(name))
    }

    fn string(value: &str) -> Token<'_> {
        Token::String(Cow::Borrowed(value))
    }

    fn url(value: &str) -> Token<'_> {
        Token::Url(Cow::Borrowed(value))
    }

    const fn int(value: f64, sign: Option<Sign>) -> Number {
        Number {
            value,
            kind: NumberType::Integer,
            sign,
        }
    }

    const fn num(value: f64, sign: Option<Sign>) -> Number {
        Number {
            value,
            kind: NumberType::Number,
            sign,
        }
    }

    fn dim(number: Number, unit: &str) -> Token<'_> {
        Token::Dimension {
            number,
            unit: Cow::Borrowed(unit),
        }
    }

    // Claim: "A newline, U+0009 CHARACTER TABULATION, or U+0020 SPACE."
    // https://drafts.csswg.org/css-syntax-3/#whitespace
    // Claim: "Replace any U+000D CARRIAGE RETURN (CR) code points, U+000C FORM
    // FEED (FF) code points, or pairs of U+000D CARRIAGE RETURN (CR) followed
    // by U+000A LINE FEED (LF) in input by a single U+000A LINE FEED (LF) code
    // point."
    // https://drafts.csswg.org/css-syntax-3/#input-preprocessing
    #[test]
    fn whitespace_code_points_include_preprocessed_newlines() {
        for c in [' ', '\t', '\n', '\r', '\x0C'] {
            assert!(is_whitespace(c), "{c:?} is whitespace");
        }
        for c in ['\r', '\n', '\x0C'] {
            assert!(is_newline(c), "{c:?} is a newline");
        }
        for c in ['\x0B', '\u{A0}', 'a', '\0'] {
            assert!(!is_whitespace(c), "{c:?} is not whitespace");
        }
        assert!(!is_newline('\t'));
    }

    // Claim: "Consume as much whitespace as possible. Return a
    // <whitespace-token>."
    // https://drafts.csswg.org/css-syntax-3/#consume-token
    #[test]
    fn whitespace_run_is_one_token() {
        assert_eq!(toks(" \t\r\n\x0C "), [Token::Whitespace]);
        assert_eq!(toks("a  b"), [ident("a"), Token::Whitespace, ident("b")]);
        assert_eq!(toks(""), []);
    }

    // Claim: "Replace any U+0000 NULL or surrogate code points in input with
    // U+FFFD REPLACEMENT CHARACTER (�)."
    // https://drafts.csswg.org/css-syntax-3/#input-preprocessing
    #[test]
    fn null_reads_as_replacement_character() {
        assert_eq!(toks("a\0b"), [ident("a\u{FFFD}b")]);
        assert_eq!(toks("'\0'"), [string("\u{FFFD}")]);
        assert_ne!(toks("a\0b"), [ident("a\0b")]);
    }

    // Claim: "If the next two input code point are U+002F SOLIDUS (/) followed
    // by a U+002A ASTERISK (*), consume them and all following code points up
    // to and including the first U+002A ASTERISK (*) followed by a U+002F
    // SOLIDUS (/), or up to an EOF code point."
    // https://drafts.csswg.org/css-syntax-3/#consume-comment
    #[test]
    fn comments_are_consumed_between_tokens() {
        let ten = Token::Number(int(10.0, None));
        assert_eq!(toks("10/**/px"), [ten, ident("px")]);
        assert_eq!(
            toks("/* a */ b /* c"),
            [Token::Whitespace, ident("b"), Token::Whitespace]
        );
        assert_eq!(toks("/**//**/"), []);
        assert_eq!(
            toks("/ *"),
            [Token::Delim('/'), Token::Whitespace, Token::Delim('*')]
        );
    }

    // Claim: "Check if two code points are a valid escape ... If the first code
    // point is not U+005C REVERSE SOLIDUS (\), return false. Otherwise, if the
    // second code point is a newline, return false. Otherwise, return true."
    // https://drafts.csswg.org/css-syntax-3/#starts-with-a-valid-escape
    #[test]
    fn valid_escape_check() {
        assert!(starts_with_valid_escape("\\a"));
        assert!(starts_with_valid_escape("\\"));
        assert!(starts_with_valid_escape("\\\\"));
        assert!(!starts_with_valid_escape("\\\n"));
        assert!(!starts_with_valid_escape("\\\r\n"));
        assert!(!starts_with_valid_escape("\\\x0C"));
        assert!(!starts_with_valid_escape("a\\"));
        assert!(!starts_with_valid_escape(""));
    }

    // Claim: "ident-start code point: A letter, a non-ASCII ident code point,
    // or U+005F LOW LINE (_)." and "ident code point: An ident-start code
    // point, a digit, or U+002D HYPHEN-MINUS (-)."
    // https://drafts.csswg.org/css-syntax-3/#ident-start-code-point
    // Claim: "non-ASCII ident code point: A code point whose value is any of:
    // U+00B7 between U+00C0 and U+00D6 between U+00D8 and U+00F6 between
    // U+00F8 and U+037D between U+037F and U+1FFF U+200C U+200D U+203F U+2040
    // between U+2070 and U+218F between U+2C00 and U+2FEF between U+3001 and
    // U+D7FF between U+F900 and U+FDCF between U+FDF0 and U+FFFD greater than
    // or equal to U+10000"
    // https://drafts.csswg.org/css-syntax-3/#non-ascii-ident-code-point
    #[test]
    fn ident_code_point_classes() {
        for c in [
            'a',
            'Z',
            '_',
            '\u{B7}',
            '\u{C0}',
            '\u{37F}',
            '\u{200C}',
            '\u{3001}',
            '\u{FFFD}',
            '\u{10000}',
        ] {
            assert!(is_ident_start_code_point(c), "{c:?} starts an ident");
        }
        for c in ['-', '0', '9'] {
            assert!(is_ident_code_point(c), "{c:?} is an ident code point");
            assert!(
                !is_ident_start_code_point(c),
                "{c:?} does not start an ident"
            );
        }
        for c in [
            '\u{D7}', '\u{F7}', '\u{37E}', '\u{2000}', '\u{200E}', '\u{FDD0}', '\u{FFFE}', '$', ' ',
        ] {
            assert!(!is_non_ascii_ident_code_point(c), "{c:?} is excluded");
            assert!(!is_ident_code_point(c), "{c:?} is not an ident code point");
        }
        assert!(is_non_ascii_ident_code_point('\u{E9}'));
    }

    // Claim: "non-printable code point: A code point between U+0000 NULL and
    // U+0008 BACKSPACE inclusive, or U+000B LINE TABULATION, or a code point
    // between U+000E SHIFT OUT and U+001F INFORMATION SEPARATOR ONE inclusive,
    // or U+007F DELETE."
    // https://drafts.csswg.org/css-syntax-3/#non-printable-code-point
    #[test]
    fn non_printable_code_points() {
        for c in ['\0', '\x01', '\x08', '\x0B', '\x0E', '\x1F', '\x7F'] {
            assert!(is_non_printable_code_point(c), "{c:?} is non-printable");
        }
        for c in ['\t', '\n', '\x0C', '\r', ' ', 'a', '\u{80}'] {
            assert!(!is_non_printable_code_point(c), "{c:?} is printable");
        }
    }

    // Claim: "Look at the first code point: U+002D HYPHEN-MINUS If the second
    // code point is an ident-start code point or a U+002D HYPHEN-MINUS, or the
    // second and third code points are a valid escape, return true. Otherwise,
    // return false. ident-start code point Return true. U+005C REVERSE SOLIDUS
    // (\) If the first and second code points are a valid escape, return
    // true. Otherwise, return false. anything else Return false."
    // https://drafts.csswg.org/css-syntax-3/#would-start-an-identifier
    #[test]
    fn would_start_ident_sequence_check() {
        for s in [
            "a", "-a", "--", "-_", "-\\31", "\\31", "_", "\u{E9}", "-\u{E9}",
        ] {
            assert!(would_start_ident_sequence(s), "{s:?} starts an ident");
        }
        for s in ["", "-", "-1", "1", "-\\\n", "\\\n", " a", "-.", "#a"] {
            assert!(
                !would_start_ident_sequence(s),
                "{s:?} does not start an ident"
            );
        }
    }

    // Claim: "Look at the first code point: U+002B PLUS SIGN (+) U+002D
    // HYPHEN-MINUS (-) If the second code point is a digit, return true.
    // Otherwise, if the second code point is a U+002E FULL STOP (.) and the
    // third code point is a digit, return true. Otherwise, return false.
    // U+002E FULL STOP (.) If the second code point is a digit, return true.
    // Otherwise, return false. digit Return true. anything else Return false."
    // https://drafts.csswg.org/css-syntax-3/#starts-with-a-number
    #[test]
    fn would_start_number_check() {
        for s in ["1", "+1", "-1", ".5", "+.5", "-.5", "0x"] {
            assert!(would_start_number(s), "{s:?} starts a number");
        }
        for s in ["", "+", "-", ".", "+.", "-.a", "+a", "a1", "e1", "--1"] {
            assert!(!would_start_number(s), "{s:?} does not start a number");
        }
    }

    // Claim: "Let type be the string "integer". ... If the next 2 input code
    // points are U+002E FULL STOP (.) followed by a digit, then: ... Set type
    // to "number". If the next 2 or 3 input code points are U+0045 LATIN
    // CAPITAL LETTER E (E) or U+0065 LATIN SMALL LETTER E (e), optionally
    // followed by U+002D HYPHEN-MINUS (-) or U+002B PLUS SIGN (+), followed by
    // a digit, then: ... Set type to "number". ... Return value, type, and
    // sign character."
    // https://drafts.csswg.org/css-syntax-3/#consume-number
    #[test]
    fn numbers_carry_value_type_and_sign() {
        assert_eq!(toks("12"), [Token::Number(int(12.0, None))]);
        assert_eq!(toks("+12"), [Token::Number(int(12.0, Some(Sign::Plus)))]);
        assert_eq!(toks("-0"), [Token::Number(int(-0.0, Some(Sign::Minus)))]);
        assert_eq!(toks("1.5"), [Token::Number(num(1.5, None))]);
        assert_eq!(toks(".5"), [Token::Number(num(0.5, None))]);
        assert_eq!(toks("-.5"), [Token::Number(num(-0.5, Some(Sign::Minus)))]);
        assert_eq!(toks("1e3"), [Token::Number(num(1000.0, None))]);
        assert_eq!(toks("1E+3"), [Token::Number(num(1000.0, None))]);
        assert_eq!(toks("25e-1"), [Token::Number(num(2.5, None))]);
        // "1." is a number followed by a delim: the "." needs a digit after it.
        assert_eq!(
            toks("1."),
            [Token::Number(int(1.0, None)), Token::Delim('.')]
        );
        // A sign cannot follow digits, so "1-2" is two numbers.
        assert_eq!(
            toks("1-2"),
            [
                Token::Number(int(1.0, None)),
                Token::Number(int(-2.0, Some(Sign::Minus)))
            ]
        );
        assert_eq!(
            toks("0.6.5"),
            [Token::Number(num(0.6, None)), Token::Number(num(0.5, None))]
        );
    }

    // Claim: "If the next 3 input code points would start an ident sequence,
    // then: Create a <dimension-token> ... Otherwise, if the next input code
    // point is U+0025 PERCENTAGE SIGN (%), consume it. Create a
    // <percentage-token> ... Otherwise, create a <number-token>"
    // https://drafts.csswg.org/css-syntax-3/#consume-numeric-token
    #[test]
    fn numeric_tokens() {
        assert_eq!(toks("10px"), [dim(int(10.0, None), "px")]);
        assert_eq!(toks("1.5EM"), [dim(num(1.5, None), "EM")]);
        assert_eq!(toks("50%"), [Token::Percentage(int(50.0, None))]);
        assert_eq!(
            toks("-2.5%"),
            [Token::Percentage(num(-2.5, Some(Sign::Minus)))]
        );
        // An "e" without exponent digits starts the unit instead.
        assert_eq!(toks("1e"), [dim(int(1.0, None), "e")]);
        assert_eq!(toks("1e+"), [dim(int(1.0, None), "e"), Token::Delim('+')]);
        assert_eq!(toks("1--x"), [dim(int(1.0, None), "--x")]);
        assert_eq!(
            toks("1\\70x"),
            [Token::Dimension {
                number: int(1.0, None),
                unit: Cow::Owned("px".to_owned()),
            }]
        );
        assert_eq!(
            toks("1 px"),
            [
                Token::Number(int(1.0, None)),
                Token::Whitespace,
                ident("px")
            ]
        );
    }

    // Claim: "Consume the next input code point. hex digit Consume as many hex
    // digits as possible, but no more than 5. Note that this means 1-6 hex
    // digits have been consumed in total. If the next input code point is
    // whitespace, consume it as well. ... If this number is zero, or is for a
    // surrogate, or is greater than the maximum allowed code point, return
    // U+FFFD REPLACEMENT CHARACTER (�). ... EOF This is a parse error. Return
    // U+FFFD REPLACEMENT CHARACTER (�). anything else Return the current input
    // code point."
    // https://drafts.csswg.org/css-syntax-3/#consume-escaped-code-point
    #[test]
    fn escaped_code_points() {
        assert_eq!(toks("\\41 b"), [ident("Ab")]);
        assert_eq!(toks("\\41\r\nb"), [ident("Ab")]);
        assert_eq!(toks("\\41  b"), [ident("A"), Token::Whitespace, ident("b")]);
        assert_eq!(toks("\\0000411"), [ident("\u{41}1")]);
        assert_eq!(toks("\\0"), [ident("\u{FFFD}")]);
        assert_eq!(toks("\\D800"), [ident("\u{FFFD}")]);
        assert_eq!(toks("\\110000"), [ident("\u{FFFD}")]);
        assert_eq!(toks("a\\"), [ident("a\u{FFFD}")]);
        assert_eq!(toks("\\."), [ident(".")]);
        assert_eq!(toks("\\\n"), [Token::Delim('\\'), Token::Whitespace]);
    }

    // Claim: "If string’s value is an ASCII case-insensitive match for "url",
    // and the next input code point is U+0028 LEFT PARENTHESIS ((), consume
    // it. While the next two input code points are whitespace, consume the
    // next input code point. If the next one or two input code points are
    // U+0022 QUOTATION MARK ("), U+0027 APOSTROPHE ('), or whitespace followed
    // by U+0022 QUOTATION MARK (") or U+0027 APOSTROPHE ('), then create a
    // <function-token> with its value set to string and return it. Otherwise,
    // consume a url token, and return it. Otherwise, if the next input code
    // point is U+0028 LEFT PARENTHESIS ((), consume it. Create a
    // <function-token> with its value set to string and return it.
    // Otherwise, create an <ident-token> with its value set to string and
    // return it."
    // https://drafts.csswg.org/css-syntax-3/#consume-ident-like-token
    #[test]
    fn ident_like_tokens() {
        assert_eq!(toks("red"), [ident("red")]);
        assert_eq!(toks("-moz-x"), [ident("-moz-x")]);
        assert_eq!(toks("--custom"), [ident("--custom")]);
        assert_eq!(toks("rgb("), [function("rgb")]);
        assert_eq!(toks("url(a)"), [url("a")]);
        assert_eq!(toks("URL(a)"), [url("a")]);
        assert_eq!(
            toks("url('a')"),
            [function("url"), string("a"), Token::CloseParen]
        );
        assert_eq!(
            toks("url(  \"a\")"),
            [
                function("url"),
                Token::Whitespace,
                string("a"),
                Token::CloseParen
            ]
        );
        assert_ne!(
            toks("url(a)"),
            [function("url"), ident("a"), Token::CloseParen]
        );
    }

    // Claim: "Consume as much whitespace as possible. Repeatedly consume the
    // next input code point from the stream: U+0029 RIGHT PARENTHESIS ())
    // Return the <url-token>. EOF This is a parse error. Return the
    // <url-token>. whitespace Consume as much whitespace as possible. If the
    // next input code point is U+0029 RIGHT PARENTHESIS ()) or EOF, consume it
    // and return the <url-token> (if EOF was encountered, this is a parse
    // error); otherwise, consume the remnants of a bad url, create a
    // <bad-url-token>, and return it. U+0022 QUOTATION MARK (") U+0027
    // APOSTROPHE (') U+0028 LEFT PARENTHESIS (() non-printable code point This
    // is a parse error. Consume the remnants of a bad url, create a
    // <bad-url-token>, and return it."
    // https://drafts.csswg.org/css-syntax-3/#consume-url-token
    // Claim: "the input stream starts with a valid escape Consume an escaped
    // code point. This allows an escaped right parenthesis ("\)") to be
    // encountered without ending the <bad-url-token>."
    // https://drafts.csswg.org/css-syntax-3/#consume-remnants-of-bad-url
    #[test]
    fn url_and_bad_url_tokens() {
        assert_eq!(toks("url( a.svg#x )"), [url("a.svg#x")]);
        assert_eq!(toks("url(a"), [url("a")]);
        assert_eq!(toks("url()"), [url("")]);
        assert_eq!(
            toks("url(a\\)b)"),
            [Token::Url(Cow::Owned("a)b".to_owned()))]
        );
        assert_eq!(
            toks("url(a b) c"),
            [Token::BadUrl, Token::Whitespace, ident("c")]
        );
        assert_eq!(toks("url(a\"b)"), [Token::BadUrl]);
        assert_eq!(toks("url(a(b)"), [Token::BadUrl]);
        assert_eq!(toks("url(a\x01)"), [Token::BadUrl]);
        assert_eq!(toks("url(a\\\n)"), [Token::BadUrl]);
        assert_eq!(
            toks("url(a b\\)c) d"),
            [Token::BadUrl, Token::Whitespace, ident("d")]
        );
    }

    // Claim: "ending code point Return the <string-token>. EOF This is a parse
    // error. Return the <string-token>. newline This is a parse error.
    // Reconsume the current input code point, create a <bad-string-token>,
    // and return it. U+005C REVERSE SOLIDUS (\) If the next input code point
    // is EOF, do nothing. Otherwise, if the next input code point is a
    // newline, consume it. Otherwise, (the stream starts with a valid escape)
    // consume an escaped code point and append the returned code point to the
    // <string-token>’s value."
    // https://drafts.csswg.org/css-syntax-3/#consume-string-token
    #[test]
    fn string_and_bad_string_tokens() {
        assert_eq!(toks("\"a b\""), [string("a b")]);
        assert_eq!(toks("'it\"s'"), [string("it\"s")]);
        assert_eq!(toks("'open"), [string("open")]);
        assert_eq!(
            toks("'a\\'b'"),
            [Token::String(Cow::Owned("a'b".to_owned()))]
        );
        assert_eq!(
            toks("'a\\\nb'"),
            [Token::String(Cow::Owned("ab".to_owned()))]
        );
        assert_eq!(
            toks("'a\\\r\nb'"),
            [Token::String(Cow::Owned("ab".to_owned()))]
        );
        assert_eq!(toks("'a\\"), [Token::String(Cow::Owned("a".to_owned()))]);
        assert_eq!(
            toks("'a\\62 c'"),
            [Token::String(Cow::Owned("abc".to_owned()))]
        );
        assert_eq!(
            toks("'a\nb'"),
            [
                Token::BadString,
                Token::Whitespace,
                ident("b"),
                Token::String(Cow::Borrowed(""))
            ]
        );
        assert_eq!(
            toks("'a\rb"),
            [Token::BadString, Token::Whitespace, ident("b")]
        );
    }

    // Claim: "U+0023 NUMBER SIGN (#) If the next input code point is an ident
    // code point or the next two input code points are a valid escape, then:
    // Create a <hash-token>. If the next 3 input code points would start an
    // ident sequence, set the <hash-token>’s type flag to "id". Consume an
    // ident sequence, and set the <hash-token>’s value to the returned string.
    // Return the <hash-token>. Otherwise, return a <delim-token> with its
    // value set to the current input code point."
    // https://drafts.csswg.org/css-syntax-3/#consume-token
    #[test]
    fn hash_tokens() {
        let hash = |value, kind| Token::Hash {
            value: Cow::Borrowed(value),
            kind,
        };
        assert_eq!(toks("#fff"), [hash("fff", HashType::Id)]);
        assert_eq!(toks("#123"), [hash("123", HashType::Unrestricted)]);
        assert_eq!(toks("#-1"), [hash("-1", HashType::Unrestricted)]);
        assert_eq!(toks("#-a"), [hash("-a", HashType::Id)]);
        assert_eq!(toks("#"), [Token::Delim('#')]);
        assert_eq!(
            toks("# a"),
            [Token::Delim('#'), Token::Whitespace, ident("a")]
        );
    }

    // Claim: "U+002D HYPHEN-MINUS (-) If the input stream starts with a number,
    // reconsume the current input code point, consume a numeric token, and
    // return it. Otherwise, if the next 2 input code points are U+002D
    // HYPHEN-MINUS U+003E GREATER-THAN SIGN (->), consume them and return a
    // <CDC-token>. Otherwise, if the input stream starts with an ident
    // sequence, reconsume the current input code point, consume an ident-like
    // token, and return it. Otherwise, return a <delim-token> with its value
    // set to the current input code point."
    // https://drafts.csswg.org/css-syntax-3/#consume-token
    #[test]
    fn hyphen_minus_branches() {
        assert_eq!(toks("-5"), [Token::Number(int(-5.0, Some(Sign::Minus)))]);
        assert_eq!(toks("-->"), [Token::Cdc]);
        assert_eq!(toks("-x"), [ident("-x")]);
        assert_eq!(toks("- "), [Token::Delim('-'), Token::Whitespace]);
        assert_eq!(toks("+5"), [Token::Number(int(5.0, Some(Sign::Plus)))]);
        assert_eq!(toks("+x"), [Token::Delim('+'), ident("x")]);
        assert_eq!(toks(".x"), [Token::Delim('.'), ident("x")]);
    }

    // Claim: the single-code-point branches of "Consume a token": "U+0028 LEFT
    // PARENTHESIS (() Return a <(-token>. U+0029 RIGHT PARENTHESIS ()) Return
    // a <)-token>. ... U+002C COMMA (,) Return a <comma-token>. ... U+003A
    // COLON (:) Return a <colon-token>. U+003B SEMICOLON (;) Return a
    // <semicolon-token>. U+003C LESS-THAN SIGN (<) If the next 3 input code
    // points are U+0021 EXCLAMATION MARK U+002D HYPHEN-MINUS U+002D
    // HYPHEN-MINUS (!--), consume them and return a <CDO-token>. ... U+0040
    // COMMERCIAL AT (@) If the next 3 input code points would start an ident
    // sequence, consume an ident sequence, create an <at-keyword-token> ...
    // U+005B LEFT SQUARE BRACKET ([) Return a <[-token>. U+005C REVERSE
    // SOLIDUS (\) If the input stream starts with a valid escape, reconsume
    // the current input code point, consume an ident-like token, and return
    // it. Otherwise, this is a parse error. Return a <delim-token> ... U+005D
    // RIGHT SQUARE BRACKET (]) Return a <]-token>. U+007B LEFT CURLY BRACKET
    // ({) Return a <{-token>. U+007D RIGHT CURLY BRACKET (}) Return a
    // <}-token>. ... anything else Return a <delim-token> with its value set
    // to the current input code point."
    // https://drafts.csswg.org/css-syntax-3/#consume-token
    #[test]
    fn punctuation_tokens() {
        assert_eq!(
            toks("()[]{},:;"),
            [
                Token::OpenParen,
                Token::CloseParen,
                Token::OpenSquare,
                Token::CloseSquare,
                Token::OpenCurly,
                Token::CloseCurly,
                Token::Comma,
                Token::Colon,
                Token::Semicolon,
            ]
        );
        assert_eq!(toks("<!--"), [Token::Cdo]);
        assert_eq!(
            toks("<!-"),
            [Token::Delim('<'), Token::Delim('!'), Token::Delim('-')]
        );
        assert_eq!(toks("@media"), [Token::AtKeyword(Cow::Borrowed("media"))]);
        assert_eq!(
            toks("@1"),
            [Token::Delim('@'), Token::Number(int(1.0, None))]
        );
        assert_eq!(toks("!*"), [Token::Delim('!'), Token::Delim('*')]);
        assert_eq!(toks("\\"), [ident("\u{FFFD}")]);
    }

    #[test]
    fn offset_tracks_consumed_input() {
        let mut tokens = tokenize("ab cd");
        assert_eq!(tokens.offset(), 0);
        assert_eq!(tokens.next(), Some(ident("ab")));
        assert_eq!(tokens.offset(), 2);
        assert!(
            Number {
                value: 1.0,
                kind: NumberType::Integer,
                sign: None
            }
            .is_integer()
        );
        assert!(!num(1.0, None).is_integer());
    }
}
