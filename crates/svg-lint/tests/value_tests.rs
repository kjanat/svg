//! End-to-end tests for `InvalidAttributeValue`: whole documents through
//! [`svg_lint::lint`], covering the wiring from the parse tree to the value
//! checks rather than the checks themselves (each has its own unit tests).

use svg_lint::{DiagnosticCode, SvgDiagnostic, lint};

/// The value diagnostics for `body` placed inside an SVG root.
fn value_diagnostics(body: &str) -> Vec<SvgDiagnostic> {
    let source = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"#);
    lint(source.as_bytes())
        .into_iter()
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::InvalidAttributeValue)
        .collect()
}

/// The source text each value diagnostic points at, so a test can assert
/// that the range covers the attribute value.
fn flagged_values(body: &str) -> Vec<String> {
    let source = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"#);
    value_diagnostics(body)
        .iter()
        .map(|diagnostic| source[diagnostic.byte_range.clone()].to_owned())
        .collect()
}

#[test]
fn valid_document_has_no_value_diagnostics() {
    let body = r#"<defs><linearGradient id="g" gradientTransform="rotate(45 .5 .5)"><stop offset="50%" stop-color="rgb(0 128 255 / .5)"/></linearGradient></defs><g transform="translate(10,20) scale(2)" opacity=".8" fill="url(#g) red" stroke-linejoin="ROUND" class="a b"><path d="M0 0h10v10z" stroke-width="calc(1px + 2px)" pathLength="100"/><circle cx="5" cy="5em" r="var(--r)" fill="inherit"/><rect x="1" y="2" width="auto" height="10%" rx="2" style="fill: red; --x: { y }"/><animateMotion path="M0 0 L10 10" keyTimes="0;1" values="0,0; 10,10"/></g><text x="1 2 3" y="4" lang="en-GB" xml:space="preserve">t</text>"#;
    assert_eq!(value_diagnostics(body), []);
}

#[test]
fn invalid_values_are_reported_on_the_value() {
    let body = r##"<path d="L0 0" stroke-linecap="banana" fill="#12"/><circle r="-1" cx="nope"/><g transform="rotate()" opacity="half"/><animateMotion path="c 1 1"/>"##;
    assert_eq!(
        flagged_values(body),
        [
            r#""L0 0""#,
            r#""banana""#,
            "\"#12\"",
            r#""-1""#,
            r#""nope""#,
            r#""rotate()""#,
            r#""half""#,
            r#""c 1 1""#,
        ]
    );
}

#[test]
fn message_names_the_attribute_and_element() {
    let diagnostics = value_diagnostics(r#"<rect stroke-linejoin="pointy"/>"#);
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one diagnostic, got {diagnostics:?}");
    };
    assert!(
        diagnostic
            .message
            .starts_with("Invalid value for stroke-linejoin on <rect>: "),
        "{}",
        diagnostic.message
    );
}

#[test]
fn references_are_decoded_before_checking() {
    // `&#x2D;1` decodes to `-1`, which `r` forbids; `&#109;` decodes to `m`.
    assert_eq!(flagged_values(r#"<circle r="&#x2D;1"/>"#), [r#""&#x2D;1""#]);
    assert_eq!(value_diagnostics(r#"<path d="&#77;0 0"/>"#), []);
}

#[test]
fn attributes_outside_their_elements_are_not_value_checked() {
    // `path` applies to <animateMotion> and <textPath>; on a <stop> it is the
    // mesh-gradient attribute SVG 2 removed, which has no value space here.
    assert_eq!(value_diagnostics(r#"<stop path="c 1,1 2,2 3,3"/>"#), []);
}

#[test]
fn value_diagnostics_can_be_suppressed() {
    let body = r#"<!-- svg-lint-disable-next-line InvalidAttributeValue -->
<rect stroke-linejoin="pointy"/>"#;
    assert_eq!(value_diagnostics(body), []);
}

// Claim: "For a white space character (#x20, #xD, #xA, #x9), append a space
// character (#x20) to the normalized value."
// https://www.w3.org/TR/xml/#AVNormalize
#[test]
fn literal_line_breaks_are_normalized_to_spaces() {
    // A literal line break inside a CSS string is a bad string, but XML
    // turns it into a space before the value reaches CSS.
    let body = "<text text-overflow=\"'more\ntext'\" stroke-linecap=\"\r\nround\t\">t</text>";
    assert_eq!(value_diagnostics(body), []);
    // A character reference is not normalized: `&#10;` stays a line break.
    assert_eq!(
        flagged_values(r#"<text text-overflow="'more&#10;text'">t</text>"#),
        [r#""'more&#10;text'""#]
    );
}

// Claim: "For an entity reference, recursively apply step 3 of this algorithm
// to the replacement text of the entity."
// https://www.w3.org/TR/xml/#AVNormalize
#[test]
fn values_with_declared_entities_are_not_checked() {
    let source = r#"<!DOCTYPE svg [<!ENTITY overflow "ellipsis">]>
<svg xmlns="http://www.w3.org/2000/svg"><text text-overflow="&overflow;" stroke-linecap="&cap;">t</text></svg>"#;
    let diagnostics: Vec<_> = lint(source.as_bytes())
        .into_iter()
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::InvalidAttributeValue)
        .collect();
    assert_eq!(diagnostics, []);
}
