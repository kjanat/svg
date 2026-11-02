//! Booleans, ids and token lists.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.
//!
//! A check sees only the value, not the attribute it belongs to, so each one
//! accepts the union of what its kind's attributes allow.

use super::reference::{url_parse_failure, url_scheme};
use super::syntax::microsyntax::{
    eq_ascii_case_insensitive, find_ascii_case_insensitive, is_ascii_whitespace,
    split_on_ascii_whitespace, strip_ascii_whitespace,
};

/// The values a `Boolean` attribute can have besides the empty string:
/// the ARIA true/false states (and the `1` / `0` of the recommended XML
/// Schema `boolean` mapping), and the canonical name of each HTML boolean
/// attribute in the catalog.
const BOOLEAN_VALUES: &[&str] = &["true", "false", "1", "0", "async", "autofocus", "defer"];

/// `Boolean`: an ARIA true/false state
/// (<https://www.w3.org/TR/wai-aria-1.2/#valuetype_true-false>) or an HTML
/// boolean attribute
/// (<https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#boolean-attribute>).
///
/// The union of the two is accepted, ASCII case-insensitively and with
/// surrounding whitespace (the XML Schema `boolean` mapping collapses it).
pub fn check_boolean(value: &str) -> Option<String> {
    let value = strip_ascii_whitespace(value);
    if value.is_empty() || find_ascii_case_insensitive(value, BOOLEAN_VALUES).is_some() {
        return None;
    }
    Some(format!(
        "\"{value}\" is neither \"true\" nor \"false\", nor empty or the attribute's own name"
    ))
}

/// Whether `value` is non-empty and only ASCII whitespace.
fn is_only_whitespace(value: &str) -> bool {
    !value.is_empty() && value.chars().all(is_ascii_whitespace)
}

/// `Id`: the `id` attribute
/// (<https://svgwg.org/svg2-draft/struct.html#IDAttribute>) or an ARIA ID
/// reference (<https://www.w3.org/TR/wai-aria-1.2/#valuetype_idref>).
///
/// ARIA allows a zero-length value for any state or property, and the current
/// ARIA draft makes `aria-details` and `aria-errormessage` ID reference
/// lists, so only a value of nothing but whitespace is reported.
pub fn check_id(value: &str) -> Option<String> {
    is_only_whitespace(value).then(|| "an ID cannot consist of whitespace only".to_owned())
}

/// `IdList`: an ARIA ID reference list, "a list of one or more ID references"
/// (<https://www.w3.org/TR/wai-aria-1.2/#valuetype_idref_list>). Repeated
/// references are allowed, and so is a zero-length value.
pub fn check_id_list(value: &str) -> Option<String> {
    is_only_whitespace(value)
        .then(|| "an ID reference list needs at least one ID, not only whitespace".to_owned())
}

/// `TokenList`: a set of space-separated tokens
/// (<https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#set-of-space-separated-tokens>).
///
/// Any string is such a set, and the kind is shared by `class` (any tokens,
/// repeats allowed), `rel`, `role` and `requiredExtensions`, so nothing is
/// reported: the per-attribute token rules need the attribute name.
pub const fn check_token_list(_value: &str) -> Option<String> {
    None
}

/// `CommaTokenList`: `systemLanguage`, a set of comma-separated tokens each of
/// which is a BCP 47 Language-Tag
/// (<https://svgwg.org/svg2-draft/struct.html#SystemLanguageAttribute>).
///
/// A value with no tokens at all (empty or whitespace) is accepted; once there
/// is a comma, every token, including an empty one, must be a language tag.
pub fn check_comma_token_list(value: &str) -> Option<String> {
    if strip_ascii_whitespace(value).is_empty() {
        return None;
    }
    value
        .split(',')
        .map(strip_ascii_whitespace)
        .find_map(|tag| {
            if tag.is_empty() {
                Some("an empty token is not a BCP 47 language tag".to_owned())
            } else if is_language_tag(tag) {
                None
            } else {
                Some(format!(
                    "\"{tag}\" is not a well-formed BCP 47 language tag"
                ))
            }
        })
}

/// `UrlTokenList`: `ping`, a set of space-separated tokens, "each of which
/// must be a valid non-empty URL whose scheme is an HTTP(S) scheme"
/// (<https://html.spec.whatwg.org/multipage/links.html#ping>).
///
/// A token is reported when the URL parser fails on it whatever the base URL
/// is, or when it names a scheme other than `http` or `https`. A relative
/// token takes the document's scheme and is accepted.
pub fn check_url_token_list(value: &str) -> Option<String> {
    split_on_ascii_whitespace(value).find_map(|token| {
        if let Some(why) = url_parse_failure(token) {
            return Some(format!("\"{token}\" is not a URL: {why}"));
        }
        url_scheme(token)
            .filter(|scheme| scheme != "http" && scheme != "https")
            .map(|scheme| {
                format!(
                    "\"{token}\" has the scheme \"{scheme}\", but a ping URL must be http or https"
                )
            })
    })
}

/// The `irregular` grandfathered tags of RFC 5646, which do not match the
/// `langtag` production. (The `regular` ones do.)
/// <https://www.rfc-editor.org/rfc/rfc5646#section-2.1>
const IRREGULAR_LANGUAGE_TAGS: &[&str] = &[
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
];

/// Whether `tag` matches the RFC 5646 `Language-Tag` production
/// (well-formedness; the registry is not consulted). Subtags are ASCII
/// case-insensitive.
fn is_language_tag(tag: &str) -> bool {
    if find_ascii_case_insensitive(tag, IRREGULAR_LANGUAGE_TAGS).is_some() {
        return true;
    }
    let subtags: Vec<&str> = tag.split('-').collect();
    if !subtags.iter().all(|subtag| {
        (1..=8).contains(&subtag.len()) && subtag.bytes().all(|b| b.is_ascii_alphanumeric())
    }) {
        return false;
    }
    match subtags.split_first() {
        Some((first, rest)) if eq_ascii_case_insensitive(first, "x") => !rest.is_empty(),
        Some(_) => is_langtag(&subtags),
        None => false,
    }
}

fn is_alpha(subtag: &str) -> bool {
    subtag.bytes().all(|b| b.is_ascii_alphabetic())
}

fn is_digits(subtag: &str) -> bool {
    subtag.bytes().all(|b| b.is_ascii_digit())
}

/// The `langtag` production over subtags already known to be 1 to 8 ASCII
/// alphanumerics.
fn is_langtag(subtags: &[&str]) -> bool {
    let at = |index: usize| subtags.get(index).copied();
    // language = 2*3ALPHA ["-" extlang] / 4ALPHA / 5*8ALPHA
    let Some(language) = at(0).filter(|language| language.len() >= 2 && is_alpha(language)) else {
        return false;
    };
    let mut index = 1;
    if language.len() <= 3 {
        // extlang = 3ALPHA *2("-" 3ALPHA)
        while index <= 3 && at(index).is_some_and(|subtag| subtag.len() == 3 && is_alpha(subtag)) {
            index += 1;
        }
    }
    // script = 4ALPHA
    if at(index).is_some_and(|subtag| subtag.len() == 4 && is_alpha(subtag)) {
        index += 1;
    }
    // region = 2ALPHA / 3DIGIT
    if at(index).is_some_and(|subtag| {
        (subtag.len() == 2 && is_alpha(subtag)) || (subtag.len() == 3 && is_digits(subtag))
    }) {
        index += 1;
    }
    // variant = 5*8alphanum / (DIGIT 3alphanum)
    while at(index).is_some_and(|subtag| {
        subtag.len() >= 5 || (subtag.len() == 4 && subtag.starts_with(|c: char| c.is_ascii_digit()))
    }) {
        index += 1;
    }
    // extension = singleton 1*("-" (2*8alphanum))
    while at(index)
        .is_some_and(|subtag| subtag.len() == 1 && !eq_ascii_case_insensitive(subtag, "x"))
    {
        index += 1;
        let start = index;
        while at(index).is_some_and(|subtag| subtag.len() >= 2) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    // privateuse = "x" 1*("-" (1*8alphanum))
    if at(index).is_some_and(|subtag| eq_ascii_case_insensitive(subtag, "x")) {
        return index + 1 < subtags.len();
    }
    index == subtags.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use svg_data::AttributeValues;

    type Check = fn(&str) -> Option<String>;

    fn accepts(check: Check, values: &[&str]) {
        for value in values {
            assert_eq!(check(value), None, "{value:?} must be accepted");
        }
    }

    fn rejects(check: Check, values: &[&str]) {
        for value in values {
            assert!(check(value).is_some(), "{value:?} must be rejected");
        }
    }

    /// Every value space the catalog gives `def`, on any element and in any
    /// profile.
    fn value_spaces(def: &svg_data::AttributeDef) -> impl Iterator<Item = &AttributeValues> {
        std::iter::once(&def.values)
            .chain(def.value_overrides.iter().map(|(_, values)| values))
            .chain(def.element_values.iter().map(|scoped| &scoped.values))
    }

    /// Run `value` through the catalog's own dispatch for `attr`.
    fn via_catalog(attr: &str, value: &str) -> Option<String> {
        let def = svg_data::attribute(attr);
        assert!(def.is_some(), "{attr} is in the catalog");
        def.and_then(|def| super::super::invalid_reason(&def.values, value))
    }

    // Claim: "If the attribute is present, its value must either be the empty
    // string or a value that is an ASCII case-insensitive match for the
    // attribute's canonical name, with no leading or trailing whitespace."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#boolean-attribute
    // Claim (autofocus, Value column): "boolean attribute [HTML]"
    // https://svgwg.org/svg2-draft/struct.html#SVGElementAutofocusAttribute
    #[test]
    fn html_boolean_attribute_is_empty_or_its_own_name() {
        accepts(
            check_boolean,
            &[
                "",
                "async",
                "ASYNC",
                "defer",
                "Defer",
                "autofocus",
                "AutoFocus",
            ],
        );
        rejects(
            check_boolean,
            &[
                "yes",
                "no",
                "on",
                "off",
                "enabled",
                "async defer",
                "autofocused",
            ],
        );
    }

    // The Boolean kind is shared by every HTML boolean attribute in the
    // catalog (every Boolean attribute that is not an ARIA true/false state),
    // so the canonical name of each one must be accepted.
    // Claim: "If the attribute is present, its value must either be the empty
    // string or a value that is an ASCII case-insensitive match for the
    // attribute's canonical name, with no leading or trailing whitespace."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#boolean-attribute
    #[test]
    fn every_catalog_boolean_attribute_accepts_its_own_name() {
        let mut seen = 0;
        for def in svg_data::attributes() {
            if !def.name.starts_with("aria-")
                && value_spaces(def).any(|values| matches!(values, AttributeValues::Boolean))
            {
                seen += 1;
                let upper = def.name.to_ascii_uppercase();
                accepts(check_boolean, &[def.name, &upper]);
            }
        }
        assert!(seen > 0, "the catalog has boolean attributes");
    }

    // Claim (ARIA value type true/false): "Value representing either true or
    // false."
    // https://www.w3.org/TR/wai-aria-1.2/#valuetype_true-false
    // The recommended XML Schema mapping for true/false is "boolean", whose
    // lexical space also has "1" and "0" and which collapses whitespace
    // (https://www.w3.org/TR/wai-aria-1.2/#typemapping); the HTML mapping is
    // an enumerated attribute, matched ASCII case-insensitively.
    #[test]
    fn aria_true_false_state() {
        accepts(
            check_boolean,
            &[
                "true",
                "false",
                "TRUE",
                "False",
                " true ",
                "\nfalse\t",
                "1",
                "0",
            ],
        );
        rejects(
            check_boolean,
            &[
                "mixed",
                "undefined",
                "yes",
                "truthy",
                "true false",
                "2",
                "-1",
            ],
        );
    }

    // Claim: "Authors MAY specify a zero-length string ("") for any supported
    // (but not required) state or property."
    // https://www.w3.org/TR/wai-aria-1.2/#state_property_processing
    #[test]
    fn aria_zero_length_string_is_allowed() {
        accepts(check_boolean, &[""]);
        accepts(check_id, &[""]);
        accepts(check_id_list, &[""]);
    }

    #[test]
    fn boolean_usages_from_the_catalog() {
        for (attr, value) in [
            ("aria-atomic", "true"),
            ("aria-busy", "false"),
            ("aria-disabled", "true"),
            ("aria-modal", "false"),
            ("aria-multiline", "true"),
            ("aria-multiselectable", "false"),
            ("aria-readonly", "true"),
            ("aria-required", "false"),
            ("async", "async"),
            ("autofocus", ""),
            ("defer", "defer"),
        ] {
            assert_eq!(via_catalog(attr, value), None, "{attr}={value:?}");
        }
        assert!(via_catalog("aria-busy", "busy").is_some());
        assert!(via_catalog("autofocus", "yes").is_some());
    }

    // Claim: "The ‘id’ attribute must be unique within the node tree, must not
    // be an empty string, and must not contain any whitespace characters."
    // https://svgwg.org/svg2-draft/struct.html#IDAttribute
    // Claim (ARIA value type ID reference): "Reference to the ID of another
    // element in the same document"
    // https://www.w3.org/TR/wai-aria-1.2/#valuetype_idref
    #[test]
    fn id_and_id_reference_are_not_only_whitespace() {
        accepts(
            check_id,
            &["a", "x-1", "Lamppost", "gradient_2", "ünï", "1abc"],
        );
        rejects(check_id, &[" ", "   ", "\t", "\n \r\n", "\u{c}"]);
    }

    // The Id kind also carries ARIA references, and in the current ARIA draft
    // two of them are lists, so neither a zero-length value nor inner
    // whitespace can be reported for the kind as a whole.
    // Claim (aria-details): "Value: ID reference list"
    // https://w3c.github.io/aria/#aria-details
    // Claim (aria-errormessage): "Value: ID reference list"
    // https://w3c.github.io/aria/#aria-errormessage
    #[test]
    fn id_kind_accepts_aria_reference_lists() {
        accepts(
            check_id,
            &["note-1 note-2", "err1\terr2", " lead", "trail "],
        );
    }

    #[test]
    fn id_usages_from_the_catalog() {
        for (attr, value) in [
            ("aria-activedescendant", "option-3"),
            ("aria-details", "details-1"),
            ("aria-errormessage", "err-name"),
            ("id", "Lamppost"),
            ("interestfor", "tooltip"),
        ] {
            assert_eq!(via_catalog(attr, value), None, "{attr}={value:?}");
        }
        assert!(via_catalog("id", "  ").is_some());
        assert!(via_catalog("aria-activedescendant", " ").is_some());
    }

    // Claim (ARIA value type ID reference list): "A list of one or more ID
    // references."
    // https://www.w3.org/TR/wai-aria-1.2/#valuetype_idref_list
    #[test]
    fn id_reference_list_has_at_least_one_reference() {
        accepts(
            check_id_list,
            &["a", "a b", " a\tb\n", "title-1 desc-1 note"],
        );
        rejects(check_id_list, &[" ", "\t\n", "  \r\n  "]);
    }

    // Claim: "If the same element is specified multiple times in a single
    // WAI-ARIA relation, user agents SHOULD return multiple pointers to the
    // same element."
    // https://www.w3.org/TR/wai-aria-1.2/#x8-6-1-id-reference-error-processing
    #[test]
    fn id_reference_list_may_repeat_a_reference() {
        accepts(check_id_list, &["a a", "x y x"]);
    }

    #[test]
    fn id_list_usages_from_the_catalog() {
        for (attr, value) in [
            ("aria-controls", "panel-1"),
            ("aria-describedby", "desc-1 desc-2"),
            ("aria-flowto", "next-step"),
            ("aria-labelledby", "title-1"),
            ("aria-owns", "child-a child-b"),
        ] {
            assert_eq!(via_catalog(attr, value), None, "{attr}={value:?}");
        }
        assert!(via_catalog("aria-labelledby", " ").is_some());
    }

    // Claim: "A set of space-separated tokens is a string containing zero or
    // more words (known as tokens) separated by one or more ASCII whitespace,
    // where words consist of any string of one or more characters, none of
    // which are ASCII whitespace."
    // Claim: "A string containing a set of space-separated tokens may have
    // leading or trailing ASCII whitespace."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#set-of-space-separated-tokens
    #[test]
    fn set_of_space_separated_tokens_accepts_any_string() {
        accepts(
            check_token_list,
            &[
                "",
                " ",
                "a",
                " a  b ",
                "a\tb\nc",
                "日本 語",
                "a,b;c",
                "#x",
                "\u{a0}",
            ],
        );
    }

    // Claim (class, Value column): "set of space-separated tokens [HTML]"
    // https://svgwg.org/svg2-draft/styling.html#ClassAttribute
    // Only an *unordered set of unique* space-separated tokens forbids
    // duplicates; class is a plain set.
    // Claim: "An unordered set of unique space-separated tokens is a set of
    // space-separated tokens where none of the tokens are duplicated."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#unordered-set-of-unique-space-separated-tokens
    #[test]
    fn class_may_repeat_a_token() {
        accepts(check_token_list, &["big big", "a b a"]);
    }

    // Claim: "The ‘role’ attribute must be a set of space-separated tokens
    // having values defined in Definition of Roles ([wai-aria], section
    // 5.4)."
    // https://svgwg.org/svg2-draft/struct.html#RoleAttribute
    // Claim (rel): "The attribute's value must be an unordered set of unique
    // space-separated tokens."
    // https://html.spec.whatwg.org/multipage/links.html#attr-hyperlink-rel
    // Claim (requiredExtensions): "The value is a list of URL references which
    // identify the required extensions, with the individual values separated
    // by white space."
    // https://svgwg.org/svg2-draft/struct.html#RequiredExtensionsAttribute
    #[test]
    fn token_list_usages_from_the_catalog() {
        for (attr, value) in [
            ("class", "legend legend--big"),
            ("rel", "noopener noreferrer"),
            (
                "requiredExtensions",
                "http://example.org/SVGExtensionXYZ/1.0",
            ),
            ("role", "graphics-symbol img"),
        ] {
            assert_eq!(via_catalog(attr, value), None, "{attr}={value:?}");
        }
    }

    // Claim: "The value is a set of comma-separated tokens, each of which must
    // be a Language-Tag value, as defined in BCP 47 [BCP47]."
    // https://svgwg.org/svg2-draft/struct.html#SystemLanguageAttribute
    #[test]
    fn system_language_tokens_are_language_tags() {
        accepts(
            check_comma_token_list,
            &[
                "en",
                "en-US",
                "en-US, fr",
                "en,fr,de-CH",
                "zh-Hant-TW",
                "mn-Cyrl-MN",
            ],
        );
        rejects(
            check_comma_token_list,
            &[
                "en_US",
                "en US",
                "english language",
                "en-US fr",
                "e",
                "123",
                "en;fr",
            ],
        );
    }

    // Claim: "A set of comma-separated tokens is a string containing zero or
    // more tokens each separated from the next by a single U+002C COMMA
    // character (,), where tokens consist of any string of zero or more
    // characters, neither beginning nor ending with ASCII whitespace, nor
    // containing any U+002C COMMA characters (,), and optionally surrounded by
    // ASCII whitespace."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#set-of-comma-separated-tokens
    // An empty token is syntactically a token, but it is not a Language-Tag.
    #[test]
    fn comma_separated_tokens_are_trimmed_and_none_is_empty() {
        accepts(
            check_comma_token_list,
            &["", " ", " en , fr ", "\ten-GB,\nfr-CA\n", "en"],
        );
        rejects(
            check_comma_token_list,
            &["en,,fr", "en,", ",en", " , ", "en, ,fr"],
        );
    }

    // Claim: "langtag = language ["-" script] ["-" region] *("-" variant)
    // *("-" extension) ["-" privateuse]"
    // Claim: "language = 2*3ALPHA ["-" extlang] / 4ALPHA / 5*8ALPHA"
    // Claim: "extlang = 3ALPHA *2("-" 3ALPHA)"
    // Claim: "variant = 5*8alphanum / (DIGIT 3alphanum)"
    // Claim: "extension = singleton 1*("-" (2*8alphanum))"
    // Claim: "privateuse = "x" 1*("-" (1*8alphanum))"
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1
    #[test]
    fn language_tag_abnf() {
        accepts(
            check_comma_token_list,
            &[
                "de",
                "zh-yue",
                "zh-yue-HK",
                "zh-min-nan",
                "abcd",
                "english",
                "sr-Latn-RS",
                "es-419",
                "de-CH-1996",
                "sl-rozaj-biske",
                "de-CH-1901-x-phonebk",
                "en-a-bbb-x-a-ccc",
                "en-US-u-islamcal",
                "x-whatever",
                "X-a-b-c",
                "qaa-Qaaa-QM-x-southern",
                "en-abcd",
            ],
        );
        rejects(
            check_comma_token_list,
            &[
                "toolonglanguage",
                "en--US",
                "en-",
                "-en",
                "en-US-",
                "en-x",
                "en-a",
                "en-a-x-foo",
                "en-a-b-x-foo",
                "x",
                "x-",
                "x-toolongsubtag",
                "zh-min-nan-hak-hsn",
                "en-Latn-Latn",
                "en-US-GB",
                "1en",
                "en-ü",
                "en-123456789",
            ],
        );
    }

    // Claim: "At all times, language tags and their subtags, including private
    // use and extensions, are to be treated as case insensitive"
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1.1
    #[test]
    fn language_tags_are_case_insensitive() {
        accepts(
            check_comma_token_list,
            &["EN-us", "MN-cYRL-mn", "I-KLINGON", "En-Gb-OeD", "X-PRIVATE"],
        );
    }

    // Claim: "irregular = "en-GB-oed" / "i-ami" / "i-bnn" / "i-default" /
    // "i-enochian" / "i-hak" / "i-klingon" / "i-lux" / "i-mingo" / "i-navajo"
    // / "i-pwn" / "i-tao" / "i-tay" / "i-tsu" / "sgn-BE-FR" / "sgn-BE-NL" /
    // "sgn-CH-DE""
    // Claim: "regular = "art-lojban" / "cel-gaulish" / "no-bok" / "no-nyn" /
    // "zh-guoyu" / "zh-hakka" / "zh-min" / "zh-min-nan" / "zh-xiang""
    // https://www.rfc-editor.org/rfc/rfc5646#section-2.1
    #[test]
    fn grandfathered_language_tags() {
        accepts(
            check_comma_token_list,
            &[
                "en-GB-oed",
                "i-ami",
                "i-default",
                "i-klingon",
                "sgn-BE-FR",
                "sgn-CH-DE",
                "art-lojban",
                "cel-gaulish",
                "no-bok",
                "zh-min-nan",
                "zh-xiang",
            ],
        );
        rejects(
            check_comma_token_list,
            &["i-foo", "i", "en-GB-oe", "sgn-BE-DE"],
        );
    }

    #[test]
    fn comma_token_list_usages_from_the_catalog() {
        assert_eq!(via_catalog("systemLanguage", "en-US, fr-CA"), None);
        assert!(via_catalog("systemLanguage", "en_US").is_some());
    }

    // Claim (ping): "The value must be a set of space-separated tokens, each of
    // which must be a valid non-empty URL whose scheme is an HTTP(S) scheme."
    // https://html.spec.whatwg.org/multipage/links.html#ping
    // Claim: "An HTTP(S) scheme is "http" or "https"."
    // https://fetch.spec.whatwg.org/#http-scheme
    // Claim (SVG ping, Value column): "space-separated valid non-empty URL
    // tokens [HTML]"
    // https://svgwg.org/svg2-draft/linking.html#AElementPingAttribute
    #[test]
    fn ping_tokens_are_http_or_https_urls() {
        accepts(
            check_url_token_list,
            &[
                "",
                "https://example.com/track",
                "http://a.example/p https://b.example/q",
                " /ping\t//cdn.example/p  track.php?x=1 ",
                "HTTPS://EXAMPLE.COM/",
                "Http:relative",
            ],
        );
        rejects(
            check_url_token_list,
            &[
                "mailto:audit@example.com",
                "javascript:void(0)",
                "ftp://example.com/ping",
                "https://example.com/ data:,x",
                "file:///tmp/ping",
                "wss://example.com/",
            ],
        );
    }

    // Each ping token is a URL, so a token the URL parser fails on is not a
    // valid URL.
    // Claim (host state): "If url is special and buffer is the empty string,
    // host-missing validation error, return failure."
    // https://url.spec.whatwg.org/#host-state
    #[test]
    fn ping_tokens_must_parse() {
        rejects(
            check_url_token_list,
            &[
                "http://",
                "https://example.com/ok https://:80/",
                "https://[::1/",
            ],
        );
    }

    #[test]
    fn url_token_list_usages_from_the_catalog() {
        assert_eq!(
            via_catalog("ping", "https://example.com/a https://example.com/b"),
            None
        );
        assert!(via_catalog("ping", "mailto:x@example.com").is_some());
    }
}
