//! URL and IRI references.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.
//!
//! SVG 2 parses an attribute defined as a URL with the URL Standard's URL
//! parser against the document's URL, and when that parse fails "the
//! attribute is assumed to have been specified as the given initial value"
//! (<https://svgwg.org/svg2-draft/types.html#attribute-url>). So the values
//! reported are those the parser fails on whatever the document's URL is: a
//! scheme-ful or scheme-relative URL whose authority cannot be parsed. Such a
//! reference is ignored; SVG 2 does not make the document non-conforming over
//! it (<https://svgwg.org/svg2-draft/linking.html#linkRefAttrs>). Validation
//! errors that still parse (spaces, backslashes, unescaped code points) are
//! accepted, as is anything whose outcome depends on the base URL.
//!
//! An IRI is held to the RFC 3987 grammar instead, and only to the shape of
//! its authority: IRI values come from SVG 1.1's `xlink:href`, which `XLink`
//! 1.1 makes a LEIRI, allowing more characters than RFC 3987 does.

/// `Url`: a URL, parsed by the URL Standard's URL parser.
pub fn check_url(value: &str) -> Option<String> {
    url_parse_failure(value).map(|why| {
        format!(
            "the URL parser fails on it ({why}), so the attribute is treated as its initial value"
        )
    })
}

/// `Iri`: an IRI reference
/// (<https://www.rfc-editor.org/rfc/rfc3987#section-2.2>).
pub fn check_iri(value: &str) -> Option<String> {
    iri_authority_failure(value).map(|why| format!("it is not an IRI reference: {why}"))
}

/// Why the authority of an IRI reference, when it has one, does not match
/// `iauthority = [ iuserinfo "@" ] ihost [ ":" port ]`. Only the IP-literal
/// brackets and the port are checked: LEIRIs widen the other characters.
fn iri_authority_failure(value: &str) -> Option<&'static str> {
    let input = preprocess(value);
    // `IRI = scheme ":" ihier-part ...` and `irelative-part = "//" iauthority
    // ...`; RFC 3987's `scheme` is the one the URL parser's scheme state reads.
    let hier_part = split_scheme(&input).map_or(input.as_str(), |(_, rest)| rest);
    let authority = hier_part.strip_prefix("//")?;
    let authority = &authority[..authority.find(['/', '?', '#']).unwrap_or(authority.len())];
    let host_and_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let port = if let Some(literal) = host_and_port.strip_prefix('[') {
        let Some((address, after)) = literal.split_once(']') else {
            return Some("the IP-literal has no closing \"]\"");
        };
        if let Some(future) = address.strip_prefix(['v', 'V']) {
            if let Some(failure) = ip_future_failure(future) {
                return Some(failure);
            }
        } else if ipv6_failure(address).is_some() {
            return Some("the IP-literal is neither an IPv6 address nor IPvFuture");
        }
        if after.is_empty() {
            return None;
        }
        let Some(port) = after.strip_prefix(':') else {
            return Some("only a port can follow the IP-literal");
        };
        port
    } else {
        let (host, port) = host_and_port.split_once(':').unwrap_or((host_and_port, ""));
        if host.contains(['[', ']']) {
            return Some("\"[\" and \"]\" can only enclose an IP-literal host");
        }
        port
    };
    (!port.bytes().all(|b| b.is_ascii_digit())).then_some("the port is not all digits")
}

/// Why `IPvFuture = "v" 1*HEXDIG "." 1*( unreserved / sub-delims / ":" )`
/// does not match, given what follows the "v".
fn ip_future_failure(future: &str) -> Option<&'static str> {
    let Some((version, address)) = future.split_once('.') else {
        return Some("IPvFuture needs a \".\" after its version");
    };
    (version.is_empty()
        || !version.bytes().all(|b| b.is_ascii_hexdigit())
        || address.is_empty()
        || address.contains('['))
    .then_some("IPvFuture is \"v\", hex digits, \".\" and an address")
}

/// The special schemes that have a network host ("file" is special too, but
/// its host is parsed by a separate state that is not modelled here).
/// <https://url.spec.whatwg.org/#special-scheme>
const SPECIAL_NETWORK_SCHEMES: &[&str] = &["ftp", "http", "https", "ws", "wss"];

/// The scheme the URL parser reads from `input`, lowercased, when `input`
/// starts with one (<https://url.spec.whatwg.org/#scheme-state>). Without a
/// scheme the URL is relative and takes the base URL's scheme.
pub(super) fn url_scheme(input: &str) -> Option<String> {
    split_scheme(&preprocess(input)).map(|(scheme, _)| scheme)
}

/// Why the basic URL parser returns failure on `input` regardless of the base
/// URL, or `None` when it does not (or when the outcome depends on the base).
pub(super) fn url_parse_failure(input: &str) -> Option<&'static str> {
    let input = preprocess(input);
    let Some((scheme, rest)) = split_scheme(&input) else {
        return scheme_relative_failure(&input);
    };
    if SPECIAL_NETWORK_SCHEMES.contains(&scheme.as_str()) {
        // "special relative or authority state", "relative slash state" and
        // "special authority slashes state" all reach the authority when two
        // of `/` or `\` follow the scheme, whatever the base URL is; with
        // fewer, a same-scheme base URL makes the value relative.
        let mut leading = rest.chars();
        if !matches!(
            (leading.next(), leading.next()),
            (Some('/' | '\\'), Some('/' | '\\'))
        ) {
            return None;
        }
        // "special authority ignore slashes state".
        authority_failure(rest.trim_start_matches(['/', '\\']), true)
    } else if scheme == "file" {
        None
    } else {
        // "path or authority state": `scheme://` starts an authority.
        authority_failure(rest.strip_prefix("//")?, false)
    }
}

/// Why a scheme-relative `input` fails against every possible base URL.
///
/// Its two leading slashes reach the "special authority ignore slashes state"
/// against a special base other than "file", the "authority state" against a
/// non-special base, and the "file host state" against a "file" base; a base
/// with an opaque path fails it in the "no scheme state" anyway. It is
/// reported only when all three fail.
fn scheme_relative_failure(input: &str) -> Option<&'static str> {
    let rest = input.strip_prefix("//")?;
    let failure = authority_failure(rest.trim_start_matches(['/', '\\']), true)?;
    authority_failure(rest, false)?;
    file_host_failure(rest)?;
    Some(failure)
}

/// The "file host state" over `input`, which starts right after the two
/// slashes of the "file slash state".
fn file_host_failure(input: &str) -> Option<&'static str> {
    let buffer = &input[..input.find(['/', '\\', '?', '#']).unwrap_or(input.len())];
    if buffer.is_empty() || is_windows_drive_letter(buffer) {
        return None;
    }
    // "host parsing buffer with url is not special": "file" is special, so
    // the host is a domain, not an opaque host.
    host_failure(buffer, false)
}

/// "A Windows drive letter is two code points, of which the first is an ASCII
/// alpha and the second is either U+003A (:) or U+007C (|)."
/// <https://url.spec.whatwg.org/#windows-drive-letter>
fn is_windows_drive_letter(buffer: &str) -> bool {
    let mut chars = buffer.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':' | '|'), None) if letter.is_ascii_alphabetic()
    )
}

/// The input preprocessing of the basic URL parser: "Remove any leading and
/// trailing C0 control or space from input." and "Remove all ASCII tab or
/// newline from input."
fn preprocess(input: &str) -> String {
    input
        .trim_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect()
}

/// Split off the scheme the "scheme start state" and "scheme state" read: an
/// ASCII alpha, then ASCII alphanumerics, `+`, `-` or `.`, then `:`.
fn split_scheme(input: &str) -> Option<(String, &str)> {
    if !input.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let end = input.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))?;
    let rest = input[end..].strip_prefix(':')?;
    Some((input[..end].to_ascii_lowercase(), rest))
}

/// The "authority state", "host state" and "port state" over `input`, which
/// starts right after the authority's leading slashes.
fn authority_failure(input: &str, special: bool) -> Option<&'static str> {
    let end = input
        .find(|c| matches!(c, '/' | '?' | '#') || (special && c == '\\'))
        .unwrap_or(input.len());
    let authority = &input[..end];
    let host_and_port = match authority.rfind('@') {
        Some(at) if at + 1 == authority.len() => {
            return Some("host-missing: no host after the credentials");
        }
        Some(at) => &authority[at + 1..],
        None => authority,
    };
    let mut inside_brackets = false;
    let mut colon = None;
    for (at, c) in host_and_port.char_indices() {
        match c {
            '[' => inside_brackets = true,
            ']' => inside_brackets = false,
            ':' if !inside_brackets => {
                colon = Some(at);
                break;
            }
            _ => {}
        }
    }
    match colon {
        Some(0) => Some("host-missing: no host before the port"),
        Some(at) => host_failure(&host_and_port[..at], !special)
            .or_else(|| port_failure(&host_and_port[at + 1..])),
        None if special && host_and_port.is_empty() => {
            Some("host-missing: a special-scheme URL needs a host")
        }
        None => host_failure(host_and_port, !special),
    }
}

/// The "port state": ASCII digits forming a 16-bit unsigned integer.
fn port_failure(port: &str) -> Option<&'static str> {
    if !port.bytes().all(|b| b.is_ascii_digit()) {
        return Some("port-invalid: the port is not a decimal number");
    }
    let value = port.bytes().fold(0_u32, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add(u32::from(digit - b'0'))
    });
    (value > u32::from(u16::MAX)).then_some("port-out-of-range: the port is above 65535")
}

/// "A forbidden host code point".
/// <https://url.spec.whatwg.org/#forbidden-host-code-point>
const fn is_forbidden_host_code_point(c: char) -> bool {
    matches!(
        c,
        '\0' | '\t'
            | '\n'
            | '\r'
            | ' '
            | '#'
            | '/'
            | ':'
            | '<'
            | '>'
            | '?'
            | '@'
            | '['
            | '\\'
            | ']'
            | '^'
            | '|'
    )
}

/// "A forbidden domain code point": a forbidden host code point, a C0
/// control, `%` or DELETE.
/// <https://url.spec.whatwg.org/#forbidden-domain-code-point>
const fn is_forbidden_domain_code_point(c: char) -> bool {
    is_forbidden_host_code_point(c) || c.is_ascii_control() || c == '%'
}

/// The "host parser" (<https://url.spec.whatwg.org/#concept-host-parser>).
fn host_failure(input: &str, opaque: bool) -> Option<&'static str> {
    if let Some(inner) = input.strip_prefix('[') {
        let Some(address) = inner.strip_suffix(']') else {
            return Some("IPv6-unclosed: the IPv6 address has no closing bracket");
        };
        return ipv6_failure(address);
    }
    if opaque {
        return input
            .contains(is_forbidden_host_code_point)
            .then_some("host-invalid-code-point: the host has a forbidden code point");
    }
    let decoded = percent_decode(input);
    let domain = String::from_utf8_lossy(&decoded);
    if domain.contains(is_forbidden_domain_code_point) {
        // ASCII input is only lowercased by the domain parser; non-ASCII
        // input goes through UTS #46 ToASCII without the STD3 rules, which
        // keeps these ASCII code points or fails.
        return Some("domain-invalid-code-point: the host has a forbidden code point");
    }
    if !domain.is_ascii() {
        // Whether UTS #46 accepts the rest is not modelled.
        return None;
    }
    let ascii_domain = domain.to_ascii_lowercase();
    if ends_in_a_number(&ascii_domain) {
        return ipv4_failure(&ascii_domain);
    }
    None
}

/// "percent-decode" (<https://url.spec.whatwg.org/#percent-decode>).
fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let hex = |offset: usize| {
            bytes
                .get(at + offset)
                .and_then(|&digit| char::from(digit).to_digit(16))
        };
        if let (b'%', Some(high), Some(low)) = (byte, hex(1), hex(2)) {
            // Two hex digits are at most 0xFF.
            out.push(u8::try_from(high * 16 + low).unwrap_or(u8::MAX));
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    out
}

/// The "ends in a number checker".
/// <https://url.spec.whatwg.org/#ends-in-a-number-checker>
fn ends_in_a_number(input: &str) -> bool {
    let mut parts: Vec<&str> = input.split('.').collect();
    if parts.last().is_some_and(|last| last.is_empty()) {
        parts.pop();
    }
    let Some(last) = parts.last() else {
        return false;
    };
    (!last.is_empty() && last.bytes().all(|b| b.is_ascii_digit())) || ipv4_number(last).is_some()
}

/// The "IPv4 number parser", saturating at `u64::MAX` (any value that large
/// fails the range checks anyway).
/// <https://url.spec.whatwg.org/#ipv4-number-parser>
fn ipv4_number(input: &str) -> Option<u64> {
    if input.is_empty() {
        return None;
    }
    let (digits, radix) =
        if input.len() >= 2 && (input.starts_with("0x") || input.starts_with("0X")) {
            (&input[2..], 16)
        } else if input.len() >= 2 && input.starts_with('0') {
            (&input[1..], 8)
        } else {
            (input, 10)
        };
    digits.chars().try_fold(0_u64, |value, c| {
        c.to_digit(radix).map(|digit| {
            value
                .saturating_mul(u64::from(radix))
                .saturating_add(u64::from(digit))
        })
    })
}

/// The "IPv4 parser" (<https://url.spec.whatwg.org/#concept-ipv4-parser>).
fn ipv4_failure(input: &str) -> Option<&'static str> {
    let mut parts: Vec<&str> = input.split('.').collect();
    if parts.len() > 1 && parts.last().is_some_and(|last| last.is_empty()) {
        parts.pop();
    }
    if parts.len() > 4 {
        return Some("IPv4-too-many-parts: the IPv4 address has more than four parts");
    }
    let Some(numbers) = parts
        .iter()
        .map(|part| ipv4_number(part))
        .collect::<Option<Vec<_>>>()
    else {
        return Some("IPv4-non-numeric-part: the host ends in a number but is not an IPv4 address");
    };
    let (last, init) = numbers.split_last()?;
    if init.iter().any(|&number| number > 255) {
        return Some("IPv4-out-of-range-part: an IPv4 part is above 255");
    }
    // 256^(5 - size) for a size of 1 to 4.
    let limit = 1_u64 << (8 * (5 - numbers.len()));
    (*last >= limit).then_some("IPv4-out-of-range-part: the IPv4 address is out of range")
}

/// The "IPv6 parser" (<https://url.spec.whatwg.org/#concept-ipv6-parser>).
fn ipv6_failure(input: &str) -> Option<&'static str> {
    const INVALID: &str = "IPv6-invalid-code-point: the IPv6 address is malformed";
    let chars: Vec<char> = input.chars().collect();
    let at = |pointer: usize| chars.get(pointer).copied();
    let leading_compression = at(0) == Some(':');
    if leading_compression && at(1) != Some(':') {
        return Some("IPv6-invalid-compression: the IPv6 address starts with one colon");
    }
    // A leading "::" is consumed up front and starts the compression.
    let (mut pointer, mut piece_index, mut compress) = if leading_compression {
        (2, 1, Some(1))
    } else {
        (0, 0, None)
    };
    while let Some(c) = at(pointer) {
        if piece_index == 8 {
            return Some("IPv6-too-many-pieces: the IPv6 address has more than eight pieces");
        }
        if c == ':' {
            if compress.is_some() {
                return Some("IPv6-multiple-compression: the IPv6 address has two '::'");
            }
            pointer += 1;
            piece_index += 1;
            compress = Some(piece_index);
            continue;
        }
        let mut length = 0;
        while length < 4 && at(pointer).is_some_and(|c| c.is_ascii_hexdigit()) {
            pointer += 1;
            length += 1;
        }
        match at(pointer) {
            Some('.') => {
                if length == 0 || piece_index > 6 {
                    return Some(INVALID);
                }
                pointer -= length;
                if let Some(failure) = ipv4_in_ipv6_failure(&chars[pointer..]) {
                    return Some(failure);
                }
                // The four IPv4 numbers fill two pieces.
                piece_index += 2;
                break;
            }
            Some(':') => {
                pointer += 1;
                if at(pointer).is_none() {
                    return Some(INVALID);
                }
            }
            Some(_) => return Some(INVALID),
            None => {}
        }
        piece_index += 1;
    }
    (compress.is_none() && piece_index != 8)
        .then_some("IPv6-too-few-pieces: the IPv6 address has fewer than eight pieces")
}

/// The dotted IPv4 tail of the IPv6 parser, from its first digit to the end
/// of the address; the caller has checked that it fits in the address.
fn ipv4_in_ipv6_failure(chars: &[char]) -> Option<&'static str> {
    const INVALID: &str = "IPv4-in-IPv6-invalid-code-point: the embedded IPv4 address is malformed";
    let mut pointer = 0;
    let mut numbers_seen = 0;
    while let Some(&c) = chars.get(pointer) {
        if numbers_seen > 0 {
            if c == '.' && numbers_seen < 4 {
                pointer += 1;
            } else {
                return Some(INVALID);
            }
        }
        if !chars.get(pointer).is_some_and(char::is_ascii_digit) {
            return Some(INVALID);
        }
        let mut piece: Option<u32> = None;
        while let Some(digit) = chars.get(pointer).and_then(|c| c.to_digit(10)) {
            piece = match piece {
                None => Some(digit),
                Some(0) => return Some(INVALID),
                Some(value) => Some(value * 10 + digit),
            };
            if piece.is_some_and(|value| value > 255) {
                return Some("IPv4-in-IPv6-out-of-range-part: an embedded IPv4 part is above 255");
            }
            pointer += 1;
        }
        numbers_seen += 1;
    }
    (numbers_seen != 4).then_some(
        "IPv4-in-IPv6-too-few-parts: the embedded IPv4 address has fewer than four parts",
    )
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
    // Only a parse *failure*, whose result the next claim defines, is
    // reported; a value that parses with validation errors (spaces,
    // backslashes, unescaped code points) is accepted.
    // Claim: "When an attribute fails to parse according to the specified CSS
    // Value Definition Syntax, ABNF or EBNF grammar, or if parsing according to
    // the URL Standard or by the prose describing how to parse the attribute
    // indicates failure, the attribute is assumed to have been specified as
    // the given initial value."
    // https://svgwg.org/svg2-draft/types.html#attribute-url
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
                "http://[1:2:1.2.3.4]/",
                "foo://[x]/",
            ],
        );
    }

    // A scheme-relative value reaches an authority (or, against a "file"
    // base, the file host state) whatever the document's URL is, unless that
    // URL has an opaque path, which fails anyway.
    // Claim (no scheme state): "If base is null, or base has an opaque path
    // and c is not U+0023 (#), missing-scheme-non-relative-URL validation
    // error, return failure."
    // https://url.spec.whatwg.org/#no-scheme-state
    // Claim (relative slash state): "If url is special and c is U+002F (/) or
    // U+005C (\), then: [...] Set state to special authority ignore slashes
    // state." Claim: "Otherwise, if c is U+002F (/), then set state to
    // authority state."
    // https://url.spec.whatwg.org/#relative-slash-state
    // Claim (file host state): "Let host be the result of host parsing buffer
    // with url is not special." Claim: "If host is failure, then return
    // failure."
    // https://url.spec.whatwg.org/#file-host-state
    // Claim (authority state): "If atSignSeen is true and buffer is the empty
    // string, host-missing validation error, return failure."
    // https://url.spec.whatwg.org/#authority-state
    // Claim (host parser): "If input does not end with U+005D (]),
    // IPv6-unclosed validation error, return failure."
    // https://url.spec.whatwg.org/#concept-host-parser
    #[test]
    fn scheme_relative_authority_fails_whatever_the_base() {
        accepts(
            check_url,
            &[
                "//cdn.example.com/x.svg",
                "//host:8080/p",
                "///x",
                "///:80/x",
                "//C:/x",
                "//C|/x",
                "//[::1]/x",
                "\\\\:80/x",
            ],
        );
        rejects(
            check_url,
            &[
                "//:80/p",
                "//[::1/x",
                "//user@/x",
                "//host:99999/",
                "//[g::]/",
                "//exa mple.com/",
            ],
        );
    }

    // Claim: "When an attribute fails to parse according to the specified CSS
    // Value Definition Syntax, ABNF or EBNF grammar, or if parsing according to
    // the URL Standard or by the prose describing how to parse the attribute
    // indicates failure, the attribute is assumed to have been specified as
    // the given initial value."
    // https://svgwg.org/svg2-draft/types.html#attribute-url
    // A parse failure is reported for what it does (the reference is dropped),
    // not as making the document non-conforming.
    #[test]
    fn href_parse_failure_falls_back_to_the_initial_value() {
        let reason = check_url("http://:80/");
        assert!(
            reason
                .as_deref()
                .is_some_and(|reason| reason.contains("initial value")),
            "{reason:?} says the attribute falls back to its initial value"
        );
        assert_eq!(check_url("http://example.com/"), None);
    }

    // Claim (URL reference): "An URL reference is an Internationalized
    // Resource Identifier, as defined in Internationalized Resource
    // Identifiers [rfc3987]."
    // https://svgwg.org/svg2-draft/linking.html#TermURLReference
    // Claim: "ihost          = IP-literal / IPv4address / ireg-name"
    // Claim: "ireg-name      = *( iunreserved / pct-encoded / sub-delims )"
    // https://www.rfc-editor.org/rfc/rfc3987#section-2.2
    // Claim: "port           = *DIGIT"
    // Claim: "IPvFuture      = "v" 1*HEXDIG "." 1*( unreserved / sub-delims / ":" )"
    // https://www.rfc-editor.org/rfc/rfc3987#section-2.2
    // An IRI is checked against the RFC 3987 grammar, not the URL parser:
    // values the URL parser fails on can still be well-formed IRIs.
    #[test]
    fn iri_is_not_held_to_the_url_parser() {
        accepts(
            check_iri,
            &[
                "http://例え.テスト/パス?q=値#片",
                "画像.png",
                "#ünï",
                "",
                "http://",
                "http://[v1.x]/",
                "http://[V1F.a:b!$]/",
                "http://example.com:99999/",
                "http://999.999.999.999/",
                "http://1.2.3.4.5/",
                "http://08/",
                "http://a%25b/",
                "http://a@b@example.com/",
                "//host:/x",
            ],
        );
    }

    // Claim: "The value of the href attribute is a [Legacy extended IRIs]
    // (LEIRI)."
    // https://www.w3.org/TR/xlink11/#link-locators
    // The IRI-typed attributes are SVG 1.1's XLink ones, whose LEIRIs also
    // allow space, "<", ">", '"', "{", "}", "|", "\", "^", "`" and controls,
    // so only the structure of the authority is checked.
    #[test]
    fn iri_allows_leiri_characters() {
        accepts(
            check_iri,
            &["a b", "http://exa mple/", "http://a{b}/<c>", "http://a\\b/"],
        );
    }

    // Claim: "iauthority     = [ iuserinfo "@" ] ihost [ ":" port ]"
    // Claim: "irelative-part = "//" iauthority ipath-abempty"
    // https://www.rfc-editor.org/rfc/rfc3987#section-2.2
    // Claim: "IP-literal     = "[" ( IPv6address / IPvFuture  ) "]""
    // Claim: "port           = *DIGIT"
    // https://www.rfc-editor.org/rfc/rfc3987#section-2.2
    #[test]
    fn iri_authority_has_a_well_formed_host_and_port() {
        accepts(
            check_iri,
            &[
                "http://[::1]/",
                "http://[2001:db8::1]:8080/p",
                "//[::ffff:1.2.3.4]",
                "foo://host:1234",
            ],
        );
        rejects(
            check_iri,
            &[
                "http://[::1/",
                "//[::1/x",
                "http://[g::]/",
                "http://[1::2::3]/",
                "http://[v.x]/",
                "http://[vg.x]/",
                "http://[v1.]/",
                "http://[v1x]/",
                "http://[::1]x/",
                "http://[::1]:8o/",
                "http://a]b/",
                "http://a[b/",
                "http://host:8o/",
                "//host:x/",
            ],
        );
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
