//! Constraints that belong to one attribute rather than to its value space.
//!
//! The kind checks see only the value space, which several attributes share:
//! `Length` serves `r` and `cx` alike, and `Number` serves `pathLength` as
//! well as `aria-valuenow`. The spec adds rules for single attributes on top
//! of those grammars, such as forbidding negative radii. This module applies
//! those rules once the kind check has accepted the value.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

use svg_data::AttributeValues;

use super::syntax::{
    microsyntax::strip_ascii_whitespace,
    parser::{ComponentValue, parse_component_values},
    tokenizer::Token,
};

/// Attributes whose spec makes a negative value invalid, for the value
/// spaces where that rule applies.
///
/// - `r` and `fr`: "A negative value for 'r' is invalid"
///   (<https://svgwg.org/svg2-draft/geometry.html#RProperty>) and, on
///   `<radialGradient>`, "A negative value is an error (see Error
///   processing)."
///   (<https://svgwg.org/svg2-draft/pservers.html#RadialGradientElementRAttribute>).
/// - `pathLength`: "A negative value is an error (see Error handling)."
///   (<https://svgwg.org/svg2-draft/paths.html#PathLengthAttribute>).
/// - `stroke-miterlimit`: "A negative value for 'stroke-miterlimit' is
///   invalid and must be ignored."
///   (<https://svgwg.org/svg2-draft/painting.html#StrokeMiterlimitProperty>).
const NON_NEGATIVE: &[(&str, ValueSpace)] = &[
    ("r", ValueSpace::Length),
    ("fr", ValueSpace::Length),
    ("pathLength", ValueSpace::Number),
    ("stroke-miterlimit", ValueSpace::Number),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueSpace {
    Length,
    Number,
}

impl ValueSpace {
    const fn of(values: &AttributeValues) -> Option<Self> {
        match values {
            AttributeValues::Length => Some(Self::Length),
            AttributeValues::Number => Some(Self::Number),
            _ => None,
        }
    }
}

/// Why `value` breaks a rule specific to `attr`, given that the check for
/// `values` already accepted it.
pub fn check(attr: &str, values: &AttributeValues, value: &str) -> Option<String> {
    empty_number(attr, values, value).or_else(|| negative(attr, values, value))
}

/// An empty `<number>` or `<integer>`.
///
/// The `Number` and `Integer` checks accept the empty string because an ARIA
/// attribute may be empty
/// (<https://www.w3.org/TR/wai-aria-1.2/#state_property_processing>). For
/// every other bearer the grammar needs a number: a presentation attribute is
/// parsed with the property's grammar, which says to "Return the result of parsing
/// value with grammar"
/// (<https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value>);
/// the filter, `pathLength` and `version` attributes are a bare `<number>`;
/// and `tabindex` is an HTML valid integer, "one or more ASCII digits"
/// (<https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#valid-integer>).
fn empty_number(attr: &str, values: &AttributeValues, value: &str) -> Option<String> {
    let noun = match values {
        AttributeValues::Number => "a number",
        AttributeValues::Integer => "an integer",
        _ => return None,
    };
    (!attr.starts_with("aria-") && strip_ascii_whitespace(value).is_empty())
        .then(|| format!("expected {noun}, found an empty value"))
}

/// A literal negative number, percentage or dimension where `attr` forbids
/// one. Math functions and anything else that is not a single literal are
/// left alone: their sign is only known at computed-value time.
fn negative(attr: &str, values: &AttributeValues, value: &str) -> Option<String> {
    let space = ValueSpace::of(values)?;
    if !NON_NEGATIVE.contains(&(attr, space)) {
        return None;
    }
    let list = parse_component_values(value);
    let mut significant = list.significant();
    let number = match (significant.next(), significant.next()) {
        (
            Some(ComponentValue::Token(
                Token::Number(number) | Token::Percentage(number) | Token::Dimension { number, .. },
            )),
            None,
        ) => number.value,
        _ => return None,
    };
    (number < 0.0).then(|| format!("a negative value for {attr} is invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Claim: "A negative value for 'r' is invalid"
    // https://svgwg.org/svg2-draft/geometry.html#RProperty
    #[test]
    fn only_the_named_attributes_reject_negatives() {
        assert!(check("r", &AttributeValues::Length, "-1").is_some());
        assert!(check("fr", &AttributeValues::Length, "-1%").is_some());
        assert!(check("pathLength", &AttributeValues::Number, "-0.5").is_some());
        assert_eq!(check("cx", &AttributeValues::Length, "-1"), None);
        assert_eq!(check("k1", &AttributeValues::Number, "-1"), None);
        assert_eq!(check("r", &AttributeValues::Length, "-0"), None);
    }

    // Math functions resolve their sign at computed-value time, so a negative
    // operand is not a negative value.
    #[test]
    fn non_literal_values_are_left_alone() {
        for value in ["calc(-1px)", "var(--r)", "inherit", "-1 2"] {
            assert_eq!(check("r", &AttributeValues::Length, value), None, "{value}");
        }
    }

    // Claim: "A string is a valid integer if it consists of one or more ASCII
    // digits, optionally prefixed with a U+002D HYPHEN-MINUS character (-)."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#valid-integer
    #[test]
    fn empty_numbers_are_invalid_outside_aria() {
        assert!(check("tabindex", &AttributeValues::Integer, "").is_some());
        assert!(check("k1", &AttributeValues::Number, " ").is_some());
        assert_eq!(check("aria-level", &AttributeValues::Integer, ""), None);
        assert_eq!(check("aria-valuenow", &AttributeValues::Number, ""), None);
        assert_eq!(check("k1", &AttributeValues::Number, "1"), None);
    }
}
