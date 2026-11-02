//! Numbers, integers and number-or-percentage values.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Number`: a `<number>`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_number(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `Integer`: an `<integer>`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_integer(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `NumberOrPercentage`: a `<number>` or `<percentage>`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_number_or_percentage(value: &str) -> Option<String> {
    let _ = value;
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::invalid_reason;
    use svg_data::AttributeValues;

    type Check = fn(&str) -> Option<String>;

    fn assert_check(check: Check, what: &str, valid: &[&str], invalid: &[&str]) {
        for value in valid {
            assert_eq!(check(value), None, "{what} {value:?} must be valid");
        }
        for value in invalid {
            assert!(check(value).is_some(), "{what} {value:?} must be invalid");
        }
    }

    /// Run `valid` and `invalid` through the dispatcher with the catalog value
    /// space of `attr` on `element`, asserting it is `kind`.
    fn assert_attr(
        attr: &str,
        element: &str,
        kind: fn(&AttributeValues) -> bool,
        valid: &[&str],
        invalid: &[&str],
    ) {
        let Some(def) = svg_data::attribute(attr) else {
            panic!("{attr} is not in the catalog");
        };
        let values = def.values_for_element(Some(element));
        assert!(kind(values), "{attr} on <{element}> is {values:?}");
        for value in valid {
            assert_eq!(
                invalid_reason(values, value),
                None,
                "{attr}={value:?} on <{element}> must be valid"
            );
        }
        for value in invalid {
            assert!(
                invalid_reason(values, value).is_some(),
                "{attr}={value:?} on <{element}> must be invalid"
            );
        }
    }

    const fn is_number(values: &AttributeValues) -> bool {
        matches!(values, AttributeValues::Number)
    }

    const fn is_integer(values: &AttributeValues) -> bool {
        matches!(values, AttributeValues::Integer)
    }

    const fn is_number_or_percentage(values: &AttributeValues) -> bool {
        matches!(values, AttributeValues::NumberOrPercentage)
    }

    // Claim: "When written literally, a number is either an integer, or zero
    // or more decimal digits followed by a dot (.) followed by one or more
    // decimal digits; optionally, it can be concluded by the letter “e” or “E”
    // followed by an integer indicating the base-ten exponent in scientific
    // notation. It corresponds to the <number-token> production in the CSS
    // Syntax Module [CSS-SYNTAX-3]."
    // https://drafts.csswg.org/css-values-4/#numbers
    #[test]
    fn number_literals() {
        assert_check(
            check_number,
            "<number>",
            &[
                "0", "1", "-2.5", "+.5", ".5", "1e3", "1E-2", "-0", " 3 ", "\t4\n", "5/**/", "007",
            ],
            &[
                "1px", "50%", "one", "1,2", "1 2", "e3", "--1", "1..2", "#1", "'1'", "1e", "+-1",
                "NaN", "Infinity",
            ],
        );
    }

    // Claim: "A math function represents a numeric value, one of: <length>,
    // <frequency>, <angle>, <time>, <flex>, <resolution>, <percentage>,
    // <number>, <integer> ...or the <length-percentage>/etc mixed types, and
    // can be used wherever such a value would be valid."
    // https://drafts.csswg.org/css-values-4/#math
    #[test]
    fn numbers_accept_math_functions() {
        assert_check(
            check_number,
            "<number>",
            &["calc(1 + 2)", "min(1, 2)", "sign(-3)", "calc(pi)"],
            &[],
        );
        assert_check(
            check_integer,
            "<integer>",
            &["calc(1 + 2)", "round(2.5)"],
            &[],
        );
        assert_check(
            check_number_or_percentage,
            "<alpha-value>",
            &["calc(50% / 2)", "clamp(0, 2, 1)"],
            &[],
        );
    }

    // Claims: the attribute definitions, each "<number>".
    // "k1 = "<number>"" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fecomposite-k1
    // (and k2, k3, k4 on feComposite; slope, intercept, amplitude, exponent,
    // offset on the transfer functions; divisor and bias on
    // feConvolveMatrix; surfaceScale, diffuseConstant, specularConstant and
    // specularExponent on the lighting primitives; scale on
    // feDisplacementMap; dx and dy on feOffset and feDropShadow; seed on
    // feTurbulence; azimuth and elevation on feDistantLight; z on
    // fePointLight and feSpotLight; pointsAtX, pointsAtY, pointsAtZ,
    // specularExponent and limitingConeAngle on feSpotLight), all
    // https://drafts.fxtf.org/filter-effects-1/
    // "pathLength <number>" https://svgwg.org/svg2-draft/paths.html#PathLengthAttribute
    // "version = "<number>"" https://www.w3.org/TR/SVG11/struct.html#SVGElementVersionAttribute
    // "Value: <number>" https://svgwg.org/svg2-draft/painting.html#StrokeMiterlimitProperty
    #[test]
    fn number_attributes() {
        let usages = [
            ("amplitude", "feFuncR"),
            ("amplitude", "feComponentTransfer"),
            ("exponent", "feFuncG"),
            ("intercept", "feFuncB"),
            ("slope", "feFuncA"),
            ("offset", "feComponentTransfer"),
            ("k1", "feComposite"),
            ("k2", "feComposite"),
            ("k3", "feComposite"),
            ("k4", "feComposite"),
            ("bias", "feConvolveMatrix"),
            ("divisor", "feConvolveMatrix"),
            ("diffuseConstant", "feDiffuseLighting"),
            ("surfaceScale", "feDiffuseLighting"),
            ("surfaceScale", "feSpecularLighting"),
            ("specularConstant", "feSpecularLighting"),
            ("specularExponent", "feSpecularLighting"),
            ("specularExponent", "feSpotLight"),
            ("scale", "feDisplacementMap"),
            ("dx", "feOffset"),
            ("dy", "feOffset"),
            ("dx", "feDropShadow"),
            ("dy", "feDropShadow"),
            ("seed", "feTurbulence"),
            ("azimuth", "feDistantLight"),
            ("elevation", "feDistantLight"),
            ("z", "fePointLight"),
            ("z", "feSpotLight"),
            ("pointsAtX", "feSpotLight"),
            ("pointsAtY", "feSpotLight"),
            ("pointsAtZ", "feSpotLight"),
            ("limitingConeAngle", "feSpotLight"),
            ("pathLength", "path"),
            ("pathLength", "circle"),
            ("version", "svg"),
            ("stroke-miterlimit", "path"),
        ];
        for (attr, element) in usages {
            assert_attr(
                attr,
                element,
                is_number,
                &["0", "1.5", "-3", "1e2", " 2 "],
                &["1px", "10%", "auto", "1 2", "1,2"],
            );
        }
        assert_attr(
            "version",
            "svg",
            is_number,
            &["1.1", "1.0", "1.2"],
            &["1.1 Tiny", "SVG 1.1"],
        );
    }

    // Claim: "all properties accept the CSS-wide keywords, which represent
    // value computations common to all CSS properties."
    // https://drafts.csswg.org/css-values-4/#common-keywords
    #[test]
    fn number_presentation_attribute_accepts_css_wide_keywords() {
        assert_attr(
            "stroke-miterlimit",
            "path",
            is_number,
            &["inherit", "initial", "var(--m)"],
            &[],
        );
        assert_attr(
            "opacity",
            "g",
            is_number_or_percentage,
            &["inherit", "unset"],
            &[],
        );
    }

    // Claim: "number: Any real numerical value."
    // https://www.w3.org/TR/wai-aria-1.2/#valuetype_number
    // Claim: "Authors MAY specify a zero-length string ("") for any supported
    // (but not required) state or property."
    // https://www.w3.org/TR/wai-aria-1.2/#state_property_processing
    #[test]
    fn aria_numbers() {
        for attr in ["aria-valuemax", "aria-valuemin", "aria-valuenow"] {
            assert_attr(
                attr,
                "g",
                is_number,
                &["0", "-1.5", "100", "1e3", "", "5."],
                &["fifty", "50%", "1 2"],
            );
        }
    }

    // Claim: "When written literally, an integer is one or more decimal digits
    // 0 through 9 and corresponds to a subset of the <number-token> production
    // in the CSS Syntax Module [CSS-SYNTAX-3]. The first digit of an integer
    // may be immediately preceded by - or + to indicate the integer’s sign."
    // https://drafts.csswg.org/css-values-4/#integers
    #[test]
    fn integer_literals() {
        assert_check(
            check_integer,
            "<integer>",
            &["0", "7", "-3", "+12", " 4 ", "007"],
            &["1.5", "5px", "50%", "a", "1 2", "- 1", "1,", "0x10", "#1"],
        );
    }

    // Claim: "A string is a valid integer if it consists of one or more ASCII
    // digits, optionally prefixed with a U+002D HYPHEN-MINUS character (-)."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#valid-integer
    // Claim: "tabindex valid integer [HTML]"
    // https://svgwg.org/svg2-draft/struct.html#tabindexattribute
    #[test]
    fn tabindex_is_a_valid_integer() {
        assert_attr(
            "tabindex",
            "g",
            is_integer,
            &["0", "-1", "5", "32767"],
            &["1.5", "first", "1 2"],
        );
        assert_attr("tabindex", "core-attributes", is_integer, &["2"], &["two"]);
    }

    // Claim: "integer: A numerical value without a fractional component."
    // https://www.w3.org/TR/wai-aria-1.2/#valuetype_integer
    // Claim: "Authors MAY specify a zero-length string ("") for any supported
    // (but not required) state or property."
    // https://www.w3.org/TR/wai-aria-1.2/#state_property_processing
    #[test]
    fn aria_integers() {
        for attr in [
            "aria-colcount",
            "aria-colindex",
            "aria-colspan",
            "aria-level",
            "aria-posinset",
            "aria-rowcount",
            "aria-rowindex",
            "aria-rowspan",
            "aria-setsize",
        ] {
            assert_attr(
                attr,
                "g",
                is_integer,
                &["1", "-1", "12", "", "3.0"],
                &["1.5", "two", "1 2", "3px"],
            );
        }
    }

    // Claim: "<opacity-value> = <number> | <percentage>"
    // https://drafts.csswg.org/css-color-4/#typedef-opacity-opacity-value
    // Claim: "Value: <'opacity'>" for fill-opacity and stroke-opacity
    // https://drafts.csswg.org/fill-stroke-3/#propdef-fill-opacity
    // https://drafts.csswg.org/fill-stroke-3/#propdef-stroke-opacity
    // Claim: "Value: <'opacity'>" for stop-opacity
    // https://svgwg.org/svg2-draft/pservers.html#StopOpacityProperty
    // Claim: "Value: <'opacity'>" for flood-opacity
    // https://drafts.fxtf.org/filter-effects-1/#propdef-flood-opacity
    #[test]
    fn opacity_values() {
        for attr in [
            "opacity",
            "fill-opacity",
            "stroke-opacity",
            "stop-opacity",
            "flood-opacity",
        ] {
            assert_attr(
                attr,
                "rect",
                is_number_or_percentage,
                &["0", "0.5", "1", "50%", "150%", "-1", "2", " .25 ", "1e-1"],
                &["", "half", "0.5px", "50 %", "0.5 0.5", "50%%", "none"],
            );
        }
    }

    // Claim: "offset" "Value <number> | <percentage>"
    // https://svgwg.org/svg2-draft/pservers.html#StopElementOffsetAttribute
    #[test]
    fn stop_offset() {
        assert_attr(
            "offset",
            "stop",
            is_number_or_percentage,
            &["0", "0.5", "1", "50%", "100%", " 0.3 "],
            &["", "50px", "half", "0 1", "10%,"],
        );
        assert_check(
            check_number_or_percentage,
            "<number> | <percentage>",
            &["0.25", "25%"],
            &["25deg"],
        );
    }
}
