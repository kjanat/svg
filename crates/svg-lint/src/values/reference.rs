//! URL and IRI references.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Url`: a URL.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_url(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `Iri`: an IRI reference.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_iri(value: &str) -> Option<String> {
    let _ = value;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // Claim: "The value of the ‘href’ attribute must be a URL."
    // https://svgwg.org/svg2-draft/linking.html#linkRefAttrs
    // Claim: "When an attribute defined as a URL is parsed, this is done by
    // invoking the URL parser with the attribute's value as input and the
    // document's URL as base [URL]."
    // https://svgwg.org/svg2-draft/types.html#attribute-url
    #[test]
    fn href_is_parsed_by_the_url_parser() {
        accepts(
            check_url,
            &[
                "#Lamppost",
                "someDrawing.svg#Lamppost",
                "http://example.com/someDrawing.svg#Lamppost",
                "../images/x.png",
                "data:image/png;base64,iVBORw0KGgo=",
                "javascript:alert(1)",
                "mailto:someone@example.com",
                "urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66",
                "file:///C:/drawings/x.svg",
                "//cdn.example.com/x.svg",
                "c:\\drawings\\x.svg",
            ],
        );
        rejects(
            check_url,
            &[
                "http://",
                "https://exa mple.com/",
                "http://example.com:99999/",
            ],
        );
    }

    // Claim: "An invalid URL does not make an SVG document non-conforming."
    // https://svgwg.org/svg2-draft/linking.html#linkRefAttrs
    // Only a parse *failure* is reported; a value that parses with validation
    // errors (spaces, backslashes, unescaped code points) is accepted.
    // Claim (path state): "If c is not a URL code point and not U+0025 (%),
    // invalid-URL-unit validation error."
    // https://url.spec.whatwg.org/#path-state
    #[test]
    fn validation_errors_that_still_parse_are_accepted() {
        accepts(
            check_url,
            &[
                "",
                "my drawing.svg",
                "http://example.com/a b/c d?e f#g h",
                "http://example.com/<x>|^`{}",
                "http://example.com/%zz",
                "http:\\\\example.com\\path",
                "http://\\example.com/",
                "https://user:pa ss@example.com/",
                "http://a@b@example.com/",
            ],
        );
    }

    // Claim: "Remove any leading and trailing C0 control or space from input."
    // Claim: "Remove all ASCII tab or newline from input."
    // https://url.spec.whatwg.org/#concept-basic-url-parser
    #[test]
    fn surrounding_spaces_and_inner_tabs_or_newlines_are_removed() {
        accepts(
            check_url,
            &[
                "  #frag  ",
                "\n\thttp://example.com/\n",
                "ht\ntp://example.com/",
                "http://exa\tmple.com/",
                "http://exa\r\nmple.com:8\n0/",
            ],
        );
    }

    // Whether a special scheme without two slashes reaches the authority
    // depends on the document's URL, so such values are never reported.
    // Claim (special relative or authority state): "Otherwise,
    // special-scheme-missing-following-solidus validation error, set state to
    // relative state and decrease pointer by 1."
    // https://url.spec.whatwg.org/#special-relative-or-authority-state
    #[test]
    fn base_dependent_values_are_accepted() {
        accepts(check_url, &["http:a b", "http:/a b", "https:", "a b c"]);
    }

    // Claim (authority state): "If atSignSeen is true and buffer is the empty
    // string, host-missing validation error, return failure."
    // https://url.spec.whatwg.org/#authority-state
    // Claim (host state): "If buffer is the empty string, host-missing
    // validation error, return failure."
    // Claim (host state): "If url is special and buffer is the empty string,
    // host-missing validation error, return failure."
    // https://url.spec.whatwg.org/#host-state
    #[test]
    fn host_missing() {
        accepts(
            check_url,
            &["foo://", "foo:///x", "foo://?q", "http:///example.com/"],
        );
        rejects(
            check_url,
            &[
                "http://",
                "https://",
                "http:///",
                "http://?q",
                "ws://#f",
                "http://user@/",
                "http://user:pass@",
                "http://:80/",
                "foo://:1/",
                "foo://user@/",
            ],
        );
    }

    // Claim: "A forbidden host code point is U+0000 NULL, U+0009 TAB, U+000A
    // LF, U+000D CR, U+0020 SPACE, U+0023 (#), U+002F (/), U+003A (:), U+003C
    // (<), U+003E (>), U+003F (?), U+0040 (@), U+005B ([), U+005C (\), U+005D
    // (]), U+005E (^), or U+007C (|)."
    // https://url.spec.whatwg.org/#forbidden-host-code-point
    // Claim (opaque-host parser): "If input contains a forbidden host code
    // point, host-invalid-code-point validation error, return failure."
    // https://url.spec.whatwg.org/#concept-opaque-host-parser
    #[test]
    fn opaque_host_has_no_forbidden_host_code_point() {
        accepts(
            check_url,
            &["foo://host/", "foo://a%zz/", "foo://ü/", "foo://a.b-c_d~/"],
        );
        rejects(
            check_url,
            &[
                "foo://a b/",
                "foo://a<b>/",
                "foo://a^b/",
                "foo://a|b/",
                "foo://a\\b/",
                "foo://a[b]/",
            ],
        );
    }

    // Claim: "A forbidden domain code point is a forbidden host code point, a
    // C0 control, U+0025 (%), or U+007F DELETE."
    // https://url.spec.whatwg.org/#forbidden-domain-code-point
    // Claim (domain parser): "If result contains a forbidden domain code
    // point, then return failure."
    // https://url.spec.whatwg.org/#concept-domain-to-ascii
    // Claim (host parser): "Let domain be the result of running UTF-8 decode
    // without BOM on the percent-decoding of input."
    // https://url.spec.whatwg.org/#concept-host-parser
    #[test]
    fn domain_has_no_forbidden_domain_code_point() {
        accepts(
            check_url,
            &[
                "http://EXAMPLE.com/",
                "http://ex_ample.com/",
                "http://%41.com/",
                "http://%E4%BE%8B.com/",
                "http://例え.テスト/",
                "http://xn--r8jz45g.xn--zckzah/",
                "http://.../",
            ],
        );
        rejects(
            check_url,
            &[
                "http://exa mple.com/",
                "http://a<b/",
                "http://a^b/",
                "http://a|b/",
                "http://a%20b/",
                "http://a%2Fb/",
                "http://a%zzb/",
                "http://a%00b/",
                "http://a\u{7f}b/",
                "https://例 え/",
            ],
        );
    }

    // Claim (port state): "If port is not a 16-bit unsigned integer,
    // port-out-of-range validation error, return failure."
    // Claim (port state): "Otherwise, port-invalid validation error, return
    // failure."
    // https://url.spec.whatwg.org/#port-state
    #[test]
    fn port_is_a_16_bit_decimal() {
        accepts(
            check_url,
            &[
                "http://example.com:/",
                "http://example.com:0/",
                "http://example.com:0080/",
                "http://example.com:65535/x",
                "http://[::1]:8080/",
                "foo://host:1234",
            ],
        );
        rejects(
            check_url,
            &[
                "http://example.com:65536/",
                "http://example.com:99999999999999999999999/",
                "http://example.com:8o/",
                "http://example.com:80:80/",
                "http://example.com:-1/",
                "foo://host:x/",
            ],
        );
    }

    // Claim (host parser): "If asciiDomain ends in a number: ... Return the
    // result of IPv4 parsing asciiDomain."
    // https://url.spec.whatwg.org/#concept-host-parser
    // Claim (IPv4 parser): "If parts’s size is greater than 4,
    // IPv4-too-many-parts validation error, return failure."
    // Claim: "If result is failure, IPv4-non-numeric-part validation error,
    // return failure."
    // Claim: "If any but the last item in numbers is greater than 255, then
    // return failure."
    // Claim: "If the last item in numbers is greater than or equal to 256(5 −
    // numbers’s size), then return failure."
    // https://url.spec.whatwg.org/#concept-ipv4-parser
    // Claim (IPv4 number parser): "If input contains a code point that is not a
    // radix-R digit, then return failure."
    // https://url.spec.whatwg.org/#ipv4-number-parser
    #[test]
    fn ipv4_hosts() {
        accepts(
            check_url,
            &[
                "http://127.0.0.1/",
                "http://1.2.3.4./",
                "http://127.1/",
                "http://0x7f.1/",
                "http://0177.0.0.1/",
                "http://4294967295/",
                "http://0x/",
                "http://example.com./",
                "http://1.2.3.example/",
                "http://example.0a/",
            ],
        );
        rejects(
            check_url,
            &[
                "http://1.2.3.256/",
                "http://256.1.1.1/",
                "http://1.2.3.4.5/",
                "http://4294967296/",
                "http://0x100000000/",
                "http://09/",
                "http://example.123/",
                "http://1..2/",
                "http://0xg.1/",
            ],
        );
    }

    // Claim (host parser): "If input does not end with U+005D (]),
    // IPv6-unclosed validation error, return failure."
    // https://url.spec.whatwg.org/#concept-host-parser
    // Claim (IPv6 parser): "If pieceIndex is 8, IPv6-too-many-pieces
    // validation error, return failure."
    // Claim: "If compress is non-null, IPv6-multiple-compression validation
    // error, return failure."
    // Claim: "Otherwise, if compress is null and pieceIndex is not 8,
    // IPv6-too-few-pieces validation error, return failure."
    // Claim: "If numbersSeen is not 4, IPv4-in-IPv6-too-few-parts validation
    // error, return failure."
    // https://url.spec.whatwg.org/#concept-ipv6-parser
    #[test]
    fn ipv6_hosts() {
        accepts(
            check_url,
            &[
                "http://[::1]/",
                "http://[::]/",
                "http://[2001:db8::1]:8080/p",
                "http://[1:2:3:4:5:6:7:8]/",
                "http://[::ffff:192.168.0.1]/",
                "http://[1:2:3:4:5:6:1.2.3.4]/",
                "http://[0001::]/",
                "foo://[::1]/",
            ],
        );
        rejects(
            check_url,
            &[
                "http://[::1/",
                "http://[/",
                "http://[]/",
                "http://[1:2:3:4:5:6:7:8:9]/",
                "http://[1::2::3]/",
                "http://[:1]/",
                "http://[12345::]/",
                "http://[g::]/",
                "http://[1:]/",
                "http://[1:2:3:4:5:6:7]/",
                "http://[::1.2.3]/",
                "http://[::1.2.3.256]/",
                "http://[::01.2.3.4]/",
                "http://[1:2:3:4:5:6:7:1.2.3.4]/",
                "foo://[x]/",
            ],
        );
    }

    // Claim (URL reference): "An URL reference is an Internationalized
    // Resource Identifier, as defined in Internationalized Resource
    // Identifiers [rfc3987]."
    // https://svgwg.org/svg2-draft/linking.html#TermURLReference
    // The URL parser takes any scalar value string, so an IRI is checked by
    // the same parse.
    #[test]
    fn iri_permits_non_ascii_and_fails_like_a_url() {
        accepts(
            check_iri,
            &[
                "http://例え.テスト/パス?q=値#片",
                "画像.png",
                "#ünï",
                "",
                "a b",
            ],
        );
        rejects(check_iri, &["http://", "http://exa mple/", "http://[::1/"]);
    }

    #[test]
    fn url_usage_from_the_catalog() {
        let def = svg_data::attribute("href");
        assert!(def.is_some(), "href is in the catalog");
        if let Some(def) = def {
            let check = |value| super::super::invalid_reason(&def.values, value);
            assert_eq!(check("#gradient-1"), None);
            assert_eq!(check("https://example.com/img.png"), None);
            assert!(check("https://exa mple.com/").is_some());
        }
    }
}
