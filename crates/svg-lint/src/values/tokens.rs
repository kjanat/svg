//! Booleans, ids and token lists.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Boolean`: `true` or `false`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_boolean(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `Id`: an XML name / id token.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_id(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `IdList`: whitespace-separated id references.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_id_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `TokenList`: whitespace-separated tokens.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_token_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `CommaTokenList`: comma-separated tokens.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_comma_token_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `UrlTokenList`: whitespace-separated URLs.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_url_token_list(value: &str) -> Option<String> {
    let _ = value;
    None
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
    // catalog, so the canonical name of each one must be accepted.
    // Claim: "If the attribute is present, its value must either be the empty
    // string or a value that is an ASCII case-insensitive match for the
    // attribute's canonical name, with no leading or trailing whitespace."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#boolean-attribute
    #[test]
    fn every_catalog_boolean_attribute_accepts_its_own_name() {
        let mut seen = 0;
        for def in svg_data::attributes() {
            if value_spaces(def).any(|values| matches!(values, AttributeValues::Boolean)) {
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
