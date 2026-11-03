//! Language tags, media types and queries, referrer policies, file names.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.
//!
//! - [`check_language_tag`]: well-formedness against the RFC 5646 §2.1
//!   `Language-Tag` ABNF (<https://www.rfc-editor.org/rfc/rfc5646#section-2.1>).
//!   Validity against the IANA subtag registry is not checked.
//! - [`check_media_type`]: the RFC 9110 `media-type` production that the
//!   MIME Sniffing "valid MIME type string" refers to
//!   (<https://mimesniff.spec.whatwg.org/#valid-mime-type>).
//! - [`check_media_query_list`]: the Media Queries 4 `<media-query-list>`
//!   grammar (<https://drafts.csswg.org/mediaqueries-4/#mq-syntax>) over CSS
//!   component values; the contents of parentheses and functions are only
//!   held to `<general-enclosed>`, which accepts any balanced value.
//! - [`check_referrer_policy`]: the Referrer Policy keywords, matched as an
//!   HTML enumerated attribute.
//! - [`check_suggested_file_name`]: any value.

use super::syntax::{
    microsyntax::{eq_ascii_case_insensitive, find_ascii_case_insensitive, is_xml_whitespace},
    parser::{BlockKind, ComponentValue, comma_separated, parse_component_values},
    tokenizer::Token,
};

// ----- LanguageTag -----

/// RFC 5646 `irregular` and `regular` grandfathered tags. The `regular` ones
/// also match `langtag`; they are listed so the table mirrors the ABNF.
const GRANDFATHERED_TAGS: &[&str] = &[
    "en-GB-oed",
    "i-ami",
    "i-bnn",
    "i-default",
    "i-enochian",
    "i-hak",
    "i-klingon",
    "i-lux",
    "i-mingo",
    "i-navajo",
    "i-pwn",
    "i-tao",
    "i-tay",
    "i-tsu",
    "sgn-BE-FR",
    "sgn-BE-NL",
    "sgn-CH-DE",
    "art-lojban",
    "cel-gaulish",
    "no-bok",
    "no-nyn",
    "zh-guoyu",
    "zh-hakka",
    "zh-min",
    "zh-min-nan",
    "zh-xiang",
];

/// `LanguageTag`: a BCP 47 tag or empty.
///
/// The empty string is accepted (`lang` and `xml:lang` allow it), and so is
/// white space around the value, which an SVG 1.1 DTD discards from the
/// `NMTOKEN` `xml:lang` before the value is used. A comma-separated list of
/// tags is accepted as well: SVG 1.1 gives `lang` on `glyph` and
/// `font-face` that form, and the catalog has one value space for `lang`.
pub fn check_language_tag(value: &str) -> Option<String> {
    let value = value.trim_matches(is_xml_whitespace);
    if value.is_empty() {
        return None;
    }
    if !value.contains(',') {
        return (!is_well_formed_language_tag(value))
            .then(|| format!("\"{value}\" is not a well-formed BCP 47 language tag"));
    }
    for item in value.split(',') {
        let item = item.trim_matches(is_xml_whitespace);
        if item.is_empty() {
            return Some("a comma-separated list of language tags has an empty entry".to_owned());
        }
        if !is_well_formed_language_tag(item) {
            return Some(format!(
                "\"{item}\" is not a well-formed BCP 47 language tag"
            ));
        }
    }
    None
}

/// RFC 5646 §2.1 `Language-Tag = langtag / privateuse / grandfathered`,
/// matched ASCII case-insensitively.
fn is_well_formed_language_tag(tag: &str) -> bool {
    if find_ascii_case_insensitive(tag, GRANDFATHERED_TAGS).is_some() {
        return true;
    }
    let subtags: Vec<&str> = tag.split('-').collect();
    // Every production is built from 1 to 8 `alphanum` subtags joined by "-".
    if !subtags
        .iter()
        .all(|subtag| (1..=8).contains(&subtag.len()) && is_alphanum(subtag))
    {
        return false;
    }
    let Some((&first, rest)) = subtags.split_first() else {
        return false;
    };
    if is_private_use_singleton(first) {
        // privateuse = "x" 1*("-" (1*8alphanum))
        return !rest.is_empty();
    }
    // language = 2*3ALPHA ["-" extlang] / 4ALPHA / 5*8ALPHA
    if first.len() < 2 || !is_alpha(first) {
        return false;
    }
    let mut at = 1;
    let next = |at: usize| subtags.get(at).copied();
    if first.len() <= 3 {
        // extlang = 3ALPHA *2("-" 3ALPHA)
        let mut extlangs = 0;
        while extlangs < 3 && next(at).is_some_and(|s| s.len() == 3 && is_alpha(s)) {
            extlangs += 1;
            at += 1;
        }
    }
    // script = 4ALPHA
    if next(at).is_some_and(|s| s.len() == 4 && is_alpha(s)) {
        at += 1;
    }
    // region = 2ALPHA / 3DIGIT
    if next(at).is_some_and(|s| (s.len() == 2 && is_alpha(s)) || (s.len() == 3 && is_digit(s))) {
        at += 1;
    }
    // variant = 5*8alphanum / (DIGIT 3alphanum)
    while next(at).is_some_and(|s| {
        s.len() >= 5 || (s.len() == 4 && s.starts_with(|c: char| c.is_ascii_digit()))
    }) {
        at += 1;
    }
    // extension = singleton 1*("-" (2*8alphanum))
    while next(at).is_some_and(|s| s.len() == 1 && !is_private_use_singleton(s)) {
        at += 1;
        let start = at;
        while next(at).is_some_and(|s| s.len() >= 2) {
            at += 1;
        }
        if at == start {
            return false;
        }
    }
    // ["-" privateuse]
    if next(at).is_some_and(is_private_use_singleton) {
        return at + 1 < subtags.len();
    }
    at == subtags.len()
}

const fn is_private_use_singleton(subtag: &str) -> bool {
    eq_ascii_case_insensitive(subtag, "x")
}

fn is_alpha(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphabetic())
}

fn is_digit(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit())
}

fn is_alphanum(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric())
}

// ----- MediaType -----

/// Filter Effects 1 transfer function types (`type` on `feFuncR`, `feFuncG`,
/// `feFuncB` and `feFuncA`), which reach this check because the catalog
/// scopes them to `feComponentTransfer`.
const TRANSFER_FUNCTION_TYPES: &[&str] = &["identity", "table", "discrete", "linear", "gamma"];

/// `MediaType`: a MIME type.
///
/// A valid MIME type string: RFC 9110 `media-type = type "/" subtype
/// parameters`. The transfer function types of `feFunc*` are accepted too
/// (see [`TRANSFER_FUNCTION_TYPES`]).
pub fn check_media_type(value: &str) -> Option<String> {
    // The SVG 1.1 DTD declares the feFunc* `type` as an enumeration, so an
    // XML processor reading that DTD trims it.
    if TRANSFER_FUNCTION_TYPES.contains(&value.trim_matches(is_xml_whitespace)) {
        return None;
    }
    media_type_problem(value).map(|problem| format!("not a valid MIME type string: {problem}"))
}

/// Why `value` does not match the RFC 9110 `media-type` production.
fn media_type_problem(value: &str) -> Option<&'static str> {
    let (media_type, rest) = split_token(value);
    if media_type.is_empty() {
        return Some("expected a type such as \"text\" before \"/\"");
    }
    let Some(rest) = rest.strip_prefix('/') else {
        return Some("expected \"/\" and a subtype after the type");
    };
    let (subtype, mut rest) = split_token(rest);
    if subtype.is_empty() {
        return Some("expected a subtype after \"/\"");
    }
    // parameters = *( OWS ";" OWS [ parameter ] )
    while !rest.is_empty() {
        let Some(after) = rest.trim_start_matches(is_ows).strip_prefix(';') else {
            return Some("unexpected character; parameters must start with \";\"");
        };
        rest = after.trim_start_matches(is_ows);
        let (name, after_name) = split_token(rest);
        if name.is_empty() {
            continue;
        }
        // parameter = parameter-name "=" parameter-value
        let Some(after_eq) = after_name.strip_prefix('=') else {
            return Some("a parameter needs \"=\" and a value directly after its name");
        };
        rest = if after_eq.starts_with('"') {
            match strip_quoted_string(after_eq) {
                Some(after_value) => after_value,
                None => return Some("a quoted parameter value is not terminated"),
            }
        } else {
            let (parameter_value, after_value) = split_token(after_eq);
            if parameter_value.is_empty() {
                return Some("a parameter value is missing after \"=\"");
            }
            after_value
        };
    }
    None
}

/// RFC 9110 `tchar`.
const fn is_tchar(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

/// RFC 9110 `OWS = *( SP / HTAB )`, one character of it.
const fn is_ows(c: char) -> bool {
    matches!(c, ' ' | '\t')
}

/// The longest `token` (`*tchar`) at the start of `s`, and what follows.
fn split_token(s: &str) -> (&str, &str) {
    let end = s.find(|c: char| !is_tchar(c)).unwrap_or(s.len());
    s.split_at(end)
}

/// RFC 9110 `quoted-string` at the start of `s` (which starts with `"`):
/// the rest after the closing quote, or `None` when it is unterminated or
/// holds a character the production excludes. Non-ASCII characters are
/// taken as `obs-text`.
fn strip_quoted_string(s: &str) -> Option<&str> {
    let mut chars = s.char_indices().skip(1);
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Some(&s[at + 1..]),
            // quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
            '\\' => {
                let (_, escaped) = chars.next()?;
                if !(escaped == '\t'
                    || escaped == ' '
                    || escaped.is_ascii_graphic()
                    || !escaped.is_ascii())
                {
                    return None;
                }
            }
            // qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
            c if c == '\t' || c == ' ' || c.is_ascii_graphic() || !c.is_ascii() => {}
            _ => return None,
        }
    }
    None
}

// ----- MediaQueryList -----

/// The keywords `<media-type>` excludes (Media Queries 4 §3).
const RESERVED_MEDIA_TYPES: &[&str] = &["only", "not", "and", "or", "layer"];

/// `MediaQueryList`: a media query list.
///
/// Each top-level comma-separated entry must match `<media-query>`. The
/// inside of a parenthesized block or a function is only required to be a
/// `<general-enclosed>` (no bad string or URL token, no unmatched closing
/// bracket): media features, ranges and future syntax are not modelled. An
/// empty or whitespace-only list, unclosed blocks at the end of the value
/// and a whitespace-only last entry are accepted.
pub fn check_media_query_list(value: &str) -> Option<String> {
    let list = parse_component_values(value);
    if list.depth_limited {
        return None;
    }
    let groups: Vec<_> = comma_separated(&list.values).collect();
    let final_entry = groups.len().saturating_sub(1);
    for (index, group) in groups.iter().enumerate() {
        let query: Vec<&ComponentValue<'_>> = group
            .iter()
            .filter(|value| !value.is_whitespace())
            .collect();
        if query.is_empty() {
            if index == final_entry && !group.is_empty() {
                continue;
            }
            return Some("the media query list has an empty entry".to_owned());
        }
        if let Some(problem) = media_query_problem(&query) {
            return Some(problem);
        }
    }
    None
}

/// Why `query` (one entry, whitespace removed) does not match
/// `<media-query> = <media-condition> | [ not | only ]? <media-type>
/// [ and <media-condition-without-or> ]?`.
fn media_query_problem(query: &[&ComponentValue<'_>]) -> Option<String> {
    if media_condition_end(query, 0, true) == Some(query.len()) {
        return None;
    }
    let mut at = usize::from(
        query
            .first()
            .is_some_and(|value| is_keyword(value, "not") || is_keyword(value, "only")),
    );
    let Some(media_type) = query.get(at).and_then(|value| value.ident()) else {
        return Some(
            "expected a media type or a condition in parentheses, as in \"screen and (color)\""
                .to_owned(),
        );
    };
    if let Some(reserved) = find_ascii_case_insensitive(media_type, RESERVED_MEDIA_TYPES) {
        return Some(format!("\"{reserved}\" cannot be used as a media type"));
    }
    at += 1;
    if at == query.len() {
        return None;
    }
    if query.get(at).is_some_and(|value| is_keyword(value, "and"))
        && media_condition_end(query, at + 1, false) == Some(query.len())
    {
        return None;
    }
    Some(format!(
        "a media type can only be followed by \"and\" and a condition without \"or\", after \
         \"{media_type}\""
    ))
}

/// Where a `<media-condition>` (or, without `or`, a
/// `<media-condition-without-or>`) starting at `start` ends:
/// `<media-not> | <media-in-parens> [ <media-and>* | <media-or>* ]`.
fn media_condition_end(
    query: &[&ComponentValue<'_>],
    start: usize,
    allow_or: bool,
) -> Option<usize> {
    let first = query.get(start)?;
    if is_keyword(first, "not") {
        return query
            .get(start + 1)
            .is_some_and(|value| is_media_in_parens(value))
            .then_some(start + 2);
    }
    if !is_media_in_parens(first) {
        return None;
    }
    let mut at = start + 1;
    let mut joiner = None;
    while let Some(keyword) = query.get(at).and_then(|value| value.ident()) {
        let this = if eq_ascii_case_insensitive(keyword, "and") {
            "and"
        } else if allow_or && eq_ascii_case_insensitive(keyword, "or") {
            "or"
        } else {
            break;
        };
        // and and or cannot mix at one level.
        if joiner.is_some_and(|joiner| joiner != this)
            || !query
                .get(at + 1)
                .is_some_and(|value| is_media_in_parens(value))
        {
            break;
        }
        joiner = Some(this);
        at += 2;
    }
    Some(at)
}

/// `<media-in-parens>`, held only to its `<general-enclosed>` bound: a
/// `()-block` or a function whose contents are an `<any-value>`.
fn is_media_in_parens(value: &ComponentValue<'_>) -> bool {
    match value {
        ComponentValue::Block(block) if block.kind == BlockKind::Paren => {
            is_any_value(&block.value)
        }
        ComponentValue::Function(function) => is_any_value(&function.value),
        _ => false,
    }
}

/// CSS Syntax 3 `<any-value>`: no `<bad-string-token>`, `<bad-url-token>`
/// or unmatched `)`, `]` or `}` at any depth.
fn is_any_value(values: &[ComponentValue<'_>]) -> bool {
    values.iter().all(|value| match value {
        ComponentValue::Token(token) => !matches!(
            token,
            Token::BadString
                | Token::BadUrl
                | Token::CloseParen
                | Token::CloseSquare
                | Token::CloseCurly
        ),
        ComponentValue::Function(function) => is_any_value(&function.value),
        ComponentValue::Block(block) => is_any_value(&block.value),
    })
}

fn is_keyword(value: &ComponentValue<'_>, keyword: &str) -> bool {
    value
        .ident()
        .is_some_and(|ident| eq_ascii_case_insensitive(ident, keyword))
}

// ----- ReferrerPolicy -----

/// The referrer policies of Referrer Policy §3, the empty string included.
const REFERRER_POLICIES: &[&str] = &[
    "",
    "no-referrer",
    "no-referrer-when-downgrade",
    "same-origin",
    "origin",
    "strict-origin",
    "origin-when-cross-origin",
    "strict-origin-when-cross-origin",
    "unsafe-url",
];

/// `ReferrerPolicy`: a referrer policy token, matched ASCII
/// case-insensitively with no surrounding white space (an HTML enumerated
/// attribute).
pub fn check_referrer_policy(value: &str) -> Option<String> {
    if find_ascii_case_insensitive(value, REFERRER_POLICIES).is_some() {
        return None;
    }
    Some(format!(
        "\"{value}\" is not a referrer policy; expected one of {}",
        REFERRER_POLICIES[1..].join(", ")
    ))
}

// ----- SuggestedFileName -----

/// `SuggestedFileName`: a download file name. HTML places no restriction on
/// the value, so every value is valid.
pub const fn check_suggested_file_name(value: &str) -> Option<String> {
    let _ = value;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn accepts(check: fn(&str) -> Option<String>, values: &[&str]) {
        for value in values {
            assert_eq!(check(value), None, "{value:?} must be accepted");
        }
    }

    #[track_caller]
    fn rejects(check: fn(&str) -> Option<String>, values: &[&str]) {
        for value in values {
            assert!(check(value).is_some(), "{value:?} must be reported");
        }
    }

    // ----- LanguageTag (hreflang, lang, xml:lang) -----

    // Claim: "Language-Tag = langtag / privateuse / grandfathered" and
    // "langtag = language ["-" script] ["-" region] *("-" variant)
    // *("-" extension) ["-" privateuse]"
    // Claim: "language = 2*3ALPHA ["-" extlang] / 4ALPHA / 5*8ALPHA",
    // "extlang = 3ALPHA *2("-" 3ALPHA)", "script = 4ALPHA",
    // "region = 2ALPHA / 3DIGIT", "variant = 5*8alphanum / (DIGIT 3alphanum)"
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1
    // Claim (lang): "Its value must be a valid BCP 47 language tag, or the
    // empty string." https://svgwg.org/svg2-draft/struct.html#LangAttribute
    #[test]
    fn language_tag_langtag_production() {
        accepts(
            check_language_tag,
            &[
                "en",
                "fr",
                "de-CH",
                "en-US",
                "es-419",
                "zh-Hant-TW",
                "sr-Latn-RS",
                "zh-yue-HK",
                "zh-cmn-Hans-CN",
                "abc-def-ghi-jkl",
                "qaa",
                "abcd",
                "english",
                "de-CH-1901",
                "de-1996",
                "sl-rozaj-biske",
                "hy-Latn-IT-arevela",
                "en-Latn-GB-boont-r-extended-sequence-x-private",
            ],
        );
        rejects(
            check_language_tag,
            &[
                "e",
                "1",
                "123",
                "en_US",
                "en-",
                "-en",
                "en--US",
                "abcdefghi",
                "en-abcdefghi",
                "en-Latn-Latn",
                "en-US-US",
                "abc-def-ghi-jkl-mno",
                "abcd-def",
                "en-1",
                "en-12",
                "en-1234567890",
                "en-Latn-a",
                "en.US",
                "en-\u{fc}",
            ],
        );
    }

    // Claim: "extension = singleton 1*("-" (2*8alphanum))"
    // Claim: "singleton = DIGIT / %x41-57 / %x59-5A / %x61-77 / %x79-7A"
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1
    // Claim: "For example, "a-value" is not a well-formed language tag"
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.2.6
    #[test]
    fn language_tag_extension_production() {
        accepts(
            check_language_tag,
            &[
                "en-a-bbb",
                "en-a-bb-ccc",
                "de-u-co-phonebk",
                "ja-JP-u-ca-japanese",
                "en-0-abcd",
                "en-a-bbb-b-ccc",
            ],
        );
        rejects(
            check_language_tag,
            &[
                "a-value",
                "en-a",
                "en-a-b",
                "en-a-",
                "en-a-bbbbbbbbb",
                "en-a-x-y",
            ],
        );
    }

    // Claim: "privateuse = "x" 1*("-" (1*8alphanum))"
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1
    #[test]
    fn language_tag_privateuse_production() {
        accepts(
            check_language_tag,
            &[
                "x-whatever",
                "X-a",
                "x-a-b-c",
                "en-x-a",
                "en-US-x-12345678",
                "x-1",
            ],
        );
        rejects(
            check_language_tag,
            &["x", "x-", "en-x", "x-123456789", "en-x-", "x--a"],
        );
    }

    // Claim: "irregular = "en-GB-oed" / "i-ami" / "i-bnn" / "i-default" /
    // "i-enochian" / "i-hak" / "i-klingon" / "i-lux" / "i-mingo" /
    // "i-navajo" / "i-pwn" / "i-tao" / "i-tay" / "i-tsu" / "sgn-BE-FR" /
    // "sgn-BE-NL" / "sgn-CH-DE""
    // Claim: "regular = "art-lojban" / "cel-gaulish" / "no-bok" / "no-nyn" /
    // "zh-guoyu" / "zh-hakka" / "zh-min" / "zh-min-nan" / "zh-xiang""
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1
    #[test]
    fn language_tag_grandfathered_production() {
        accepts(
            check_language_tag,
            &[
                "en-GB-oed",
                "i-ami",
                "i-bnn",
                "i-default",
                "i-enochian",
                "i-hak",
                "i-klingon",
                "i-lux",
                "i-mingo",
                "i-navajo",
                "i-pwn",
                "i-tao",
                "i-tay",
                "i-tsu",
                "sgn-BE-FR",
                "sgn-BE-NL",
                "sgn-CH-DE",
                "art-lojban",
                "cel-gaulish",
                "no-bok",
                "no-nyn",
                "zh-guoyu",
                "zh-hakka",
                "zh-min",
                "zh-min-nan",
                "zh-xiang",
                "I-KLINGON",
                "EN-gb-OED",
            ],
        );
        rejects(
            check_language_tag,
            &["i-foo", "i", "en-GB-oe", "sgn-BE-DE", "i-klingon-x"],
        );
    }

    // Claim: "At all times, language tags and their subtags, including
    // private use and extensions, are to be treated as case insensitive"
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1.1
    #[test]
    fn language_tag_case_insensitive() {
        accepts(
            check_language_tag,
            &["EN", "mn-Cyrl-MN", "MN-cYRL-mn", "En-a-BbB", "EN-X-ABC"],
        );
        rejects(check_language_tag, &["EN_us", "X"]);
    }

    // Claim (lang): "Its value must be a valid BCP 47 language tag, or the
    // empty string. Setting the attribute to the empty string indicates that
    // the primary language is unknown."
    // https://svgwg.org/svg2-draft/struct.html#LangAttribute
    // Claim (xml:lang): "The values of the attribute are language identifiers
    // as defined by [IETF BCP 47], Tags for the Identification of Languages;
    // in addition, the empty string may be specified."
    // https://www.w3.org/TR/xml/#sec-lang-tag
    #[test]
    fn language_tag_empty_string() {
        accepts(check_language_tag, &["", "en", "en-GB"]);
        rejects(check_language_tag, &["-", "--"]);
    }

    // Claim: "Whitespace is not permitted in a language tag."
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1
    // Claim: "If the attribute type is not CDATA, then the XML processor MUST
    // further process the normalized attribute value by discarding any
    // leading and trailing space (#x20) characters"
    // https://www.w3.org/TR/xml/#AVNormalize
    // Claim: "<!ENTITY % LanguageCode.datatype "NMTOKEN" >" and
    // ""xml:lang %LanguageCode.datatype; #IMPLIED""
    // https://www.w3.org/TR/SVG11/svgdtd.html
    // (an SVG 1.1 DTD makes xml:lang an NMTOKEN, so surrounding white space
    // is discarded before the value is seen; the check cannot tell which
    // attribute or document it is in, so it accepts that white space)
    #[test]
    fn language_tag_whitespace() {
        accepts(check_language_tag, &[" en ", "\ten-US\n", "  "]);
        rejects(check_language_tag, &["en US", "en -US", "en- US", "en\tGB"]);
    }

    // Claim (SVG 1.1 glyph lang): "The attribute value is a comma-separated
    // list of language names as defined in BCP 47 [BCP47]."
    // https://www.w3.org/TR/SVG11/fonts.html#GlyphElementLangAttribute
    // (the catalog gives `lang` one value space, so a list of tags is
    // accepted wherever `lang` appears)
    #[test]
    fn language_tag_svg11_comma_list() {
        accepts(
            check_language_tag,
            &["en,fr", "en, fr-CA", "en , de-CH ,ja"],
        );
        rejects(
            check_language_tag,
            &["en,,fr", "en,", ",en", "en, f", "en,fr_CA"],
        );
    }

    // Claim (hreflang): "The value must be a valid BCP 47 language tag."
    // https://html.spec.whatwg.org/multipage/links.html#attr-hyperlink-hreflang
    // Claim: "A BCP 47 language tag string [HTML]"
    // https://svgwg.org/svg2-draft/linking.html#AElementHreflangAttribute
    #[test]
    fn language_tag_hreflang_values() {
        accepts(check_language_tag, &["fr-CA", "en", "pt-BR", "zh-Hans"]);
        rejects(check_language_tag, &["fr_CA", "en-", "en-GB-"]);
    }

    // ----- MediaType (type on <a>, and the `type` fallback) -----

    // Claim: "The value must be a valid MIME type string."
    // https://html.spec.whatwg.org/multipage/links.html#attr-hyperlink-type
    // Claim: "A valid MIME type string is a string that matches the
    // media-type token production. In particular, a valid MIME type string
    // may include parameters. [HTTP-SEMANTICS]"
    // https://mimesniff.spec.whatwg.org/#valid-mime-type
    // Claim: "media-type = type "/" subtype parameters", "type = token",
    // "subtype = token" and "The type and subtype tokens are
    // case-insensitive."
    // https://www.rfc-editor.org/rfc/rfc9110#section-8.3.1
    #[test]
    fn media_type_type_slash_subtype() {
        accepts(
            check_media_type,
            &[
                "text/html",
                "image/svg+xml",
                "application/xhtml+xml",
                "TEXT/HTML",
                "image/png",
                "application/vnd.ms-excel",
                "x-foo/x_bar",
                "a/b",
            ],
        );
        rejects(
            check_media_type,
            &[
                "",
                "text",
                "text/",
                "/html",
                "text/html/x",
                "text//html",
                "text /html",
                "text/ html",
                " text/html",
                "text/html ",
                "svg",
            ],
        );
    }

    // Claim: "token = 1*tchar" and "tchar = "!" / "#" / "$" / "%" / "&" /
    // "'" / "*" / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~" / DIGIT /
    // ALPHA"
    // https://www.rfc-editor.org/rfc/rfc9110#section-5.6.2
    #[test]
    fn media_type_token_characters() {
        accepts(
            check_media_type,
            &[
                "a!#$%&'*+-.^_`|~/z09",
                "image/x-icon",
                "application/ld+json",
            ],
        );
        rejects(
            check_media_type,
            &[
                "text/(html)",
                "text/ht\"ml",
                "te{xt/html",
                "text/htm@l",
                "t\u{eb}xt/html",
                "text/html,",
                "text/ht\\ml",
            ],
        );
    }

    // Claim: "parameters = *( OWS ";" OWS [ parameter ] )",
    // "parameter = parameter-name "=" parameter-value",
    // "parameter-name = token", "parameter-value = ( token / quoted-string )"
    // https://www.rfc-editor.org/rfc/rfc9110#section-5.6.6
    // Claim: "OWS = *( SP / HTAB )"
    // https://www.rfc-editor.org/rfc/rfc9110#section-5.6.3
    // Claim: "quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE",
    // "qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text",
    // "quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )"
    // https://www.rfc-editor.org/rfc/rfc9110#section-5.6.4
    #[test]
    fn media_type_parameters() {
        accepts(
            check_media_type,
            &[
                "text/html;charset=utf-8",
                "text/html; charset=utf-8",
                "text/html ; charset=utf-8",
                "text/html;\tcharset=UTF-8",
                "text/html; charset=\"utf-8\"",
                "multipart/mixed; boundary=\"a b; c\"",
                "text/plain; a=\"\\\"q\\\"\"",
                "text/plain; a=\"\"",
                "text/plain; a=b; c=d",
                "text/html;",
                "text/html; ",
                "text/html;;charset=utf-8",
            ],
        );
        rejects(
            check_media_type,
            &[
                "text/html; charset",
                "text/html; charset=",
                "text/html; charset = utf-8",
                "text/html; charset =utf-8",
                "text/html; =utf-8",
                "text/html; charset=\"utf-8",
                "text/html; charset=\"utf-8\"x",
                "text/html; charset=utf 8",
                "text/html charset=utf-8",
                "text/html; a=\"\\",
            ],
        );
    }

    // Claim: "For a white space character (#x20, #xD, #xA, #x9), append a
    // space character (#x20) to the normalized value."
    // Claim: "Note that if the unnormalized attribute value contains a
    // character reference to a white space character other than space
    // (#x20), the normalized value contains the referenced character itself
    // (#xD, #xA or #x9). This contrasts with the case where the unnormalized
    // value contains a white space character (not a reference), which is
    // replaced with a space character (#x20) in the normalized value"
    // https://www.w3.org/TR/xml/#AVNormalize
    // Claim: "parameters = *( OWS ";" OWS [ parameter ] )"
    // https://www.rfc-editor.org/rfc/rfc9110#section-5.6.6
    // Claim: "OWS = *( SP / HTAB )"
    // https://www.rfc-editor.org/rfc/rfc9110#section-5.6.3
    // (the check sees references decoded, so a line break may be a literal
    // one, which XML delivers as a space; it is taken as a space)
    #[test]
    fn media_type_xml_normalized_line_breaks() {
        accepts(
            check_media_type,
            &[
                "text/html;\ncharset=utf-8",
                "text/html\n; charset=utf-8",
                "text/html;\r\n charset=utf-8",
                "text/html;\n     charset=utf-8",
                "text/html;\rcharset=utf-8",
                "text/plain; a=\"b\nc\"",
                "text/plain; a=\"b\\\nc\"",
            ],
        );
        rejects(
            check_media_type,
            &[
                "\ntext/html",
                "text/html\n",
                "text\n/html",
                "text/\nhtml",
                "text/html\ncharset=utf-8",
                "text/html; charset\n=utf-8",
                "text/html; charset=\nutf-8",
            ],
        );
    }

    // Claim: "type = "identity | table | discrete | linear | gamma""
    // https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fecomponenttransfer-type
    // (the catalog scopes these keywords to feComponentTransfer, so `type` on
    // the feFuncR/G/B/A children falls back to the media type value space; the
    // check accepts them rather than report a valid transfer function type)
    #[test]
    fn media_type_accepts_transfer_function_types() {
        accepts(
            check_media_type,
            &["identity", "table", "discrete", "linear", "gamma"],
        );
        rejects(check_media_type, &["lineer", "matrix"]);
    }

    // Claim: "type = "identity | table | discrete | linear | gamma""
    // https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fecomponenttransfer-type
    // Claim: "Using the CSS Value Definition Syntax [css-values]. This is the
    // notation used to define the syntax for most attributes in this
    // specification and is the default."
    // https://svgwg.org/svg2-draft/types.html#syntax
    // Claim: "When any other attribute defined using the CSS Value
    // Definition Syntax is parsed, this is done by parsing the attribute's
    // value according to the grammar given in attribute definition table."
    // https://svgwg.org/svg2-draft/types.html#attribute-css-value
    // Claim: "Keywords are identifiers and are interpreted ASCII
    // case-insensitively (i.e., [a-z] and [A-Z] are equivalent)."
    // https://drafts.csswg.org/css-values-4/#keywords
    // (the transfer function type is parsed as CSS, so case variants,
    // white space, comments and escapes are accepted; a function the check
    // does not model is accepted as well)
    #[test]
    fn media_type_transfer_function_types_parse_as_css() {
        accepts(
            check_media_type,
            &[
                "Linear",
                "GAMMA",
                "Identity",
                " table ",
                "\ndiscrete\n",
                "linear/**/",
                "/* c */ gamma",
                "\\6c inear",
                "var(--t)",
            ],
        );
        rejects(
            check_media_type,
            &[
                "lineer",
                "Lineer",
                "linear gamma",
                "linear,",
                "'linear'",
                "\"gamma\"",
                "#linear",
            ],
        );
    }

    // Claim: "type = "identity | table | discrete | linear | gamma""
    // https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fecomponenttransfer-type
    // (a value without "/" cannot be a MIME type; the reason names the
    // transfer function types too, since `type` on feFuncR/G/B/A reaches
    // this check)
    #[test]
    fn media_type_reason_names_transfer_function_types() {
        for value in ["lineer", "matrix", "text"] {
            let reason = check_media_type(value).unwrap_or_default();
            assert!(
                reason.contains("identity, table, discrete, linear, gamma"),
                "{value:?}: {reason:?}"
            );
        }
        accepts(check_media_type, &["linear", "text/html"]);
        rejects(check_media_type, &["text/"]);
        let reason = check_media_type("text/").unwrap_or_default();
        assert!(!reason.contains("gamma"), "{reason:?}");
    }

    // ----- MediaQueryList (media on <style>) -----

    // Claim: "A string is a valid media query list if it matches the
    // <media-query-list> production of Media Queries."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#valid-media-query-list
    // Claim: "To parse a <media-query-list> production, parse a
    // comma-separated list of component values, then parse each entry in the
    // returned list as a <media-query>."
    // Claim: "Note: This definition of <media-query-list> parsing
    // intentionally accepts an empty list."
    // https://drafts.csswg.org/mediaqueries-4/#mq-syntax
    // Claim (style media): "Its value is parsed as a media_query_list."
    // https://svgwg.org/svg2-draft/styling.html#StyleElementMediaAttribute
    #[test]
    fn media_query_list_entries() {
        accepts(
            check_media_query_list,
            &[
                "",
                "   ",
                "all",
                "screen",
                "print",
                "screen, print",
                "screen,print",
                " screen , print ",
                "screen, projection and (color)",
                "screen,",
            ],
        );
        rejects(
            check_media_query_list,
            &[",", ", screen", "screen,,print", "screen, ,print", ",,"],
        );
    }

    // Claim: "<media-query> = <media-condition> | [ not | only ]?
    // <media-type> [ and <media-condition-without-or> ]?"
    // Claim: "<media-type> = <ident>"
    // https://drafts.csswg.org/mediaqueries-4/#typedef-media-query
    #[test]
    fn media_query_type_form() {
        accepts(
            check_media_query_list,
            &[
                "screen and (color)",
                "only screen and (min-width: 600px)",
                "not print",
                "not screen and (color)",
                "only screen",
                "unknown",
                "not unknown",
                "screen and (min-width: 600px) and (max-width: 900px)",
                "screen and not (color)",
                "speech and (example, all,)",
                "screen and foo(bar)",
            ],
        );
        rejects(
            check_media_query_list,
            &[
                "screen print",
                "screen and",
                "and screen",
                "not",
                "only",
                "not only screen",
                "only not screen",
                "screen (color)",
                "screen and screen",
                "screen and (color) or (hover)",
                "screen and not (color) and (hover)",
                "screen and (color) and",
                "&test",
                "screen;",
                "screen 3",
                "\"screen\"",
                "#screen",
                "screen and [color]",
                "{screen}",
                "screen)",
                "<!-- screen",
            ],
        );
    }

    // Claim: "<media-condition> = <media-not> | <media-in-parens> [
    // <media-and>* | <media-or>* ]", "<media-condition-without-or> =
    // <media-not> | <media-in-parens> <media-and>*", "<media-not> = not
    // <media-in-parens>", "<media-and> = and <media-in-parens>",
    // "<media-or> = or <media-in-parens>"
    // https://drafts.csswg.org/mediaqueries-4/#mq-syntax
    // Claim: "It is invalid to mix and and or and not at the same “level” of
    // a media query."
    // https://drafts.csswg.org/mediaqueries-4/#media-conditions
    #[test]
    fn media_query_condition_form() {
        accepts(
            check_media_query_list,
            &[
                "(color)",
                "(min-width: 600px)",
                "(400px <= width <= 700px)",
                "not (color)",
                "(color) and (hover)",
                "(color) or (hover)",
                "(color) or (hover) or (pointer: fine)",
                "not ((color) or (hover))",
                "(not (color)) and (not (hover))",
                "(color) and ((pointer) or (hover))",
                "((color) and (pointer)) or (hover)",
                "(color)and (hover)",
            ],
        );
        rejects(
            check_media_query_list,
            &[
                "(color) and (pointer) or (hover)",
                "(color) or (pointer) and (hover)",
                "not (color) and (hover)",
                "not not (color)",
                "(color) (hover)",
                "(color) and",
                "(color) or",
                "or (color)",
                "and (color)",
                "(color) not (hover)",
                "(color) xor (hover)",
            ],
        );
    }

    // Claim: "The <media-type> production does not include the keywords only,
    // not, and, or, and layer."
    // https://drafts.csswg.org/mediaqueries-4/#typedef-media-type
    // Claim: "Keywords are identifiers and are interpreted ASCII
    // case-insensitively (i.e., [a-z] and [A-Z] are equivalent)."
    // https://drafts.csswg.org/css-values-4/#keywords
    #[test]
    fn media_query_reserved_media_types() {
        accepts(
            check_media_query_list,
            &[
                "SCREEN AND (COLOR)",
                "Only Screen",
                "NOT (color)",
                "layered",
                "inherit",
                "orientation",
                "(color) OR (hover)",
            ],
        );
        rejects(
            check_media_query_list,
            &[
                "or",
                "and",
                "layer",
                "LAYER",
                "only layer",
                "not and",
                "or and (color)",
                "Only OR",
            ],
        );
    }

    // Claim: "<media-in-parens> = ( <media-condition> ) | ( <media-feature> )
    // | <general-enclosed>"
    // Claim: "<general-enclosed> = [ <function-token> <any-value>? ) ] | [ (
    // <any-value>? ) ]"
    // https://drafts.csswg.org/mediaqueries-4/#typedef-general-enclosed
    // Claim: "The <declaration-value> production matches any sequence of one
    // or more tokens, so long as the sequence does not contain
    // <bad-string-token>, <bad-url-token>, unmatched <)-token>, <]-token>, or
    // <}-token>" and "The <any-value> production is identical to
    // <declaration-value>, but also allows top-level <semicolon-token> tokens
    // and <delim-token> tokens with a value of "!"."
    // https://drafts.csswg.org/css-syntax-3/#typedef-any-value
    #[test]
    fn media_query_general_enclosed() {
        accepts(
            check_media_query_list,
            &[
                "(max-weight: 3kg) and (color), (color)",
                "(min-orientation: portrait)",
                "(color:20example)",
                "()",
                "( )",
                "(example, all,), speech",
                "(a; b !important)",
                "(width: calc(100px + 2em))",
                "(width: var(--x))",
                "foo(bar)",
                "foo()",
                "screen and (min-width: env(safe-area-inset-left))",
                "(color) and unknown-function(1, 2)",
                "([a] {b})",
                "(color",
                "screen and (min-width: 600px",
                "(width: \"x\")",
            ],
        );
        rejects(
            check_media_query_list,
            &[
                "(a])",
                "(a})",
                "foo(a])",
                "(\"bad\u{c}string\")",
                "(width: url(a b))",
                "screen and (color))",
            ],
        );
    }

    // Claim: "For a white space character (#x20, #xD, #xA, #x9), append a
    // space character (#x20) to the normalized value."
    // https://www.w3.org/TR/xml/#AVNormalize
    // Claim: "<general-enclosed> = [ <function-token> <any-value>? ) ] | [ (
    // <any-value>? ) ]"
    // https://drafts.csswg.org/mediaqueries-4/#typedef-general-enclosed
    // Claim: "<media-type> = <ident>"
    // https://drafts.csswg.org/mediaqueries-4/#typedef-media-type
    // (the check sees references decoded, so a line break may be a literal
    // one, which XML delivers as a space: a line break in a string is then
    // not a bad string, and a backslash before it escapes a space)
    #[test]
    fn media_query_xml_normalized_line_breaks() {
        accepts(
            check_media_query_list,
            &[
                "(width: \"a\nb\")",
                "(x: \"a\r\nb\")",
                "(x: 'a\rb')",
                "a\\\nb",
                "screen\nand (color)",
                "screen,\nprint",
                "\nscreen\n",
                "foo(\"a\nb\")",
            ],
        );
        rejects(
            check_media_query_list,
            &[
                "screen\nprint",
                "\n,\n",
                "(a\n])",
                "screen\nand\n",
                "(\"bad\u{c}string\")",
            ],
        );
    }

    // Claim: "Note: Whitespace is required between a not, and, or or keyword
    // and the following ( character, because without it that would instead
    // parse as a <function-token>. This is not made explicitly invalid because
    // it’s already covered by the above grammar."
    // https://drafts.csswg.org/mediaqueries-4/#mq-syntax
    #[test]
    fn media_query_keyword_function_tokens() {
        accepts(
            check_media_query_list,
            &["screen and (color)", "(color) and (hover)", "not (color)"],
        );
        rejects(
            check_media_query_list,
            &[
                "screen and(color)",
                "(color) and(hover)",
                "(color) or(hover)",
            ],
        );
    }

    // Claim: "Consume comments." (the first step of "consume a token": a
    // comment produces no token)
    // https://drafts.csswg.org/css-syntax-3/#consume-token
    #[test]
    fn media_query_comments() {
        accepts(
            check_media_query_list,
            &["screen /* note */ and (color)", "/* only */ print"],
        );
        rejects(check_media_query_list, &["screen /* x */ print"]);
    }

    // ----- ReferrerPolicy (referrerpolicy on <a>, <image>, <script>) -----

    // Claim: "A referrer policy is the empty string, "no-referrer",
    // "no-referrer-when-downgrade", "same-origin", "origin",
    // "strict-origin", "origin-when-cross-origin",
    // "strict-origin-when-cross-origin", or "unsafe-url"."
    // https://w3c.github.io/webappsec-referrer-policy/#referrer-policies
    // Claim: "A referrer policy attribute is an enumerated attribute. Each
    // referrer policy, including the empty string, is a keyword for this
    // attribute, mapping to a state of the same name."
    // https://html.spec.whatwg.org/multipage/urls-and-fetching.html#referrer-policy-attribute
    // Claim: "The referrerpolicy attribute is a referrer policy attribute"
    // https://svgwg.org/svg2-draft/embedded.html#ImageElementReferrerpolicyAttribute
    // https://svgwg.org/svg2-draft/interact.html#ScriptElementReferrerpolicyAttribute
    // https://svgwg.org/svg2-draft/linking.html#AElementReferrerpolicyAttribute
    #[test]
    fn referrer_policy_keywords() {
        accepts(
            check_referrer_policy,
            &[
                "",
                "no-referrer",
                "no-referrer-when-downgrade",
                "same-origin",
                "origin",
                "strict-origin",
                "origin-when-cross-origin",
                "strict-origin-when-cross-origin",
                "unsafe-url",
            ],
        );
        rejects(
            check_referrer_policy,
            &[
                "none",
                "no-referer",
                "unsafe",
                "origin-only",
                "always",
                "never",
                "default",
                "no-referrer, origin",
                "no-referrer origin",
            ],
        );
    }

    // Claim: "For authoring conformance purposes, if an enumerated attribute
    // is specified, the attribute's value must be one of: An ASCII
    // case-insensitive match for one of the conforming keywords for that
    // attribute, with no leading or trailing whitespace."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#enumerated-attribute
    #[test]
    fn referrer_policy_enumerated_matching() {
        accepts(
            check_referrer_policy,
            &[
                "NO-REFERRER",
                "Origin",
                "Strict-Origin-When-Cross-Origin",
                "UNSAFE-URL",
            ],
        );
        rejects(
            check_referrer_policy,
            &[
                " origin",
                "origin ",
                " ",
                "\tno-referrer\n",
                "no-referrer\u{a0}",
                "or\u{130}gin",
            ],
        );
    }

    // ----- SuggestedFileName (download on <a>) -----

    // Claim: "any value (if non-empty, value represents a suggested file
    // name)"
    // https://svgwg.org/svg2-draft/linking.html#AElementDownloadAttribute
    // Claim: "There are no restrictions on allowed values, but authors are
    // cautioned that most file systems have limitations with regard to what
    // punctuation is supported in filenames"
    // https://html.spec.whatwg.org/multipage/links.html#attr-hyperlink-download
    #[test]
    fn suggested_file_name_any_value() {
        accepts(
            check_suggested_file_name,
            &[
                "",
                " ",
                "report.pdf",
                "drawing.svg",
                "a/b\\c:d*e?f\"g<h>i|j",
                "\u{0}",
                "\u{65e5}\u{672c}.txt",
                "  leading and trailing  ",
            ],
        );
    }
}
