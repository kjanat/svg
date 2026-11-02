//! Lengths.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

use super::syntax::types::{NumericType, UnitlessMode, match_value};

/// `Length`: a `<length-percentage>` (and the keywords SVG allows).
///
/// The catalog uses this value space for `cx`, `cy`, `r`, `x`, `y`, `x1`,
/// `y1`, `x2`, `y2`, `fx`, `fy` and `fr`. As geometry properties
/// (<https://svgwg.org/svg2-draft/geometry.html>) their value is
/// `<length-percentage>`, which a presentation attribute widens to
/// `[<length-percentage> | <number>]`
/// (<https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value>).
/// The gradient and filter attributes of the same names are `<length>`,
/// `<length-percentage>` or `<number>`, all within that union, so one check
/// serves every bearer. None of these grammars has a keyword, so `auto` is
/// invalid (only `rx` and `ry` take it).
///
/// CSS-wide keywords, math functions and arbitrary substitution functions
/// are accepted: the syntax layer reports them as undecidable.
pub fn check_length(value: &str) -> Option<String> {
    match_value(
        value,
        NumericType::LengthPercentage,
        UnitlessMode::PresentationAttribute,
    )
    .is_no()
    .then(|| "expected a length, percentage or number".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::invalid_reason;
    use svg_data::AttributeValues;

    fn assert_length(valid: &[&str], invalid: &[&str]) {
        for value in valid {
            assert_eq!(check_length(value), None, "{value:?} must be valid");
        }
        for value in invalid {
            assert!(check_length(value).is_some(), "{value:?} must be invalid");
        }
    }

    /// Run `valid` and `invalid` through the dispatcher with the catalog value
    /// space of `attr` on `element`, asserting it is `Length`.
    fn assert_attr(attr: &str, element: &str, valid: &[&str], invalid: &[&str]) {
        let Some(def) = svg_data::attribute(attr) else {
            panic!("{attr} is not in the catalog");
        };
        let values = def.values_for_element(Some(element));
        assert!(
            matches!(values, AttributeValues::Length),
            "{attr} on <{element}> is {values:?}"
        );
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

    // Claim: "Value: <length-percentage>" for cx, cy, r, x and y
    // https://svgwg.org/svg2-draft/geometry.html#CxProperty
    // https://svgwg.org/svg2-draft/geometry.html#CyProperty
    // https://svgwg.org/svg2-draft/geometry.html#RProperty
    // https://svgwg.org/svg2-draft/geometry.html#XProperty
    // https://svgwg.org/svg2-draft/geometry.html#YProperty
    // Claim: "Replace all instances of <length-percentage> in grammar with
    // [<length-percentage> | <number>]."
    // https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value
    #[test]
    fn geometry_properties_are_length_percentages_or_numbers() {
        assert_length(
            &[
                "0", "10", "-5.5", "1e2", "10px", "1.5em", "2rem", "50%", "3vw", "1cqi", "4Q",
                "10PX", " 5 ", "5/**/",
            ],
            &[
                "",
                " ",
                "auto",
                "none",
                "ten",
                "10deg",
                "10s",
                "10 px",
                "10px 20px",
                "10px,",
                "50%%",
                "#10",
                "'10'",
            ],
        );
        for (attr, element) in [
            ("cx", "circle"),
            ("cy", "ellipse"),
            ("r", "circle"),
            ("x", "rect"),
            ("y", "image"),
        ] {
            assert_attr(
                attr,
                element,
                &["10", "10px", "25%", "calc(50% - 2px)"],
                &["auto", "10 20", "left"],
            );
        }
    }

    // Claim: "Value: <length-percentage> | auto" (rx only; r has no auto)
    // https://svgwg.org/svg2-draft/geometry.html#RxProperty
    // Claim: "Value: <length-percentage>"
    // https://svgwg.org/svg2-draft/geometry.html#RProperty
    #[test]
    fn r_has_no_auto_keyword() {
        assert_attr("r", "circle", &["5"], &["auto", "AUTO"]);
    }

    // Claim: "Note that this allows CSS comments and escapes to be used in
    // such attributes. For example, a value of '10\px/**/' would successfully
    // parse as '10px' in the ‘x’ presentation attribute of the ‘rect’
    // element."
    // https://svgwg.org/svg2-draft/types.html#attribute-css-value
    #[test]
    fn comments_and_escapes_are_css() {
        assert_attr("x", "rect", &["10\\px/**/", "/* x */ 10px"], &["10/**/px"]);
    }

    // Claim: "all properties accept the CSS-wide keywords, which represent
    // value computations common to all CSS properties."
    // https://drafts.csswg.org/css-values-4/#common-keywords
    // Claim: "A math function represents a numeric value, one of: <length>,
    // <frequency>, <angle>, <time>, <flex>, <resolution>, <percentage>,
    // <number>, <integer> ...or the <length-percentage>/etc mixed types, and
    // can be used wherever such a value would be valid."
    // https://drafts.csswg.org/css-values-4/#math
    // Claim: "If a property value contains one or more arbitrary substitution
    // functions, and all of those functions are themselves syntactically valid
    // according to their argument grammars, the entire value’s grammar must be
    // assumed to be valid at parse time."
    // https://drafts.csswg.org/css-values-5/#arbitrary-substitution
    #[test]
    fn css_wide_keywords_math_and_substitution() {
        assert_length(
            &[
                "inherit",
                "initial",
                "unset",
                "calc(10px + 5%)",
                "min(10px, 2em)",
                "var(--x)",
                "env(safe-area-inset-left)",
                "attr(data-x px)",
            ],
            &[],
        );
    }

    // Claims: the gradient and filter attribute definitions.
    // "cx = "<length>"" https://svgwg.org/svg2-draft/pservers.html#RadialGradientElementCXAttribute
    // "r = "<length>"" https://svgwg.org/svg2-draft/pservers.html#RadialGradientElementRAttribute
    // "Value <length>" for x1, y1, x2, y2
    // https://svgwg.org/svg2-draft/pservers.html#LinearGradientElementX1Attribute
    // "Value <length>" for cy, fx, fy and fr
    // https://svgwg.org/svg2-draft/pservers.html#RadialGradientElementFRAttribute
    // "x = "<length-percentage>"" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-filter-x
    // "x = "<length-percentage>"" (filter primitive subregion)
    // https://drafts.fxtf.org/filter-effects-1/#element-attrdef-filter-primitive-x
    // "x = "<number>"" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fepointlight-x
    #[test]
    fn gradient_and_filter_coordinates() {
        for (attr, element) in [
            ("cx", "radialGradient"),
            ("cy", "radialGradient"),
            ("r", "radialGradient"),
            ("fx", "radialGradient"),
            ("fy", "radialGradient"),
            ("fr", "radialGradient"),
            ("x1", "linearGradient"),
            ("y1", "linearGradient"),
            ("x2", "linearGradient"),
            ("y2", "linearGradient"),
            ("x", "filter"),
            ("y", "filter"),
            ("x", "feFlood"),
            ("y", "feOffset"),
            ("x", "fePointLight"),
            ("y", "feSpotLight"),
            ("x", "mask"),
            ("y", "pattern"),
        ] {
            assert_attr(
                attr,
                element,
                &["0", "0.5", "50%", "10px", "-2"],
                &["center", "50% 50%", "", "1,"],
            );
        }
    }
}
