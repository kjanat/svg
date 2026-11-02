//! Transform lists and preserveAspectRatio.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Transform`: a transform list using these functions.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_transform(value: &str, functions: &[&str]) -> Option<String> {
    let _ = (value, functions);
    None
}

/// `PreserveAspectRatio`: alignment plus optional meet/slice.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_preserve_aspect_ratio(
    value: &str,
    alignments: &[&str],
    meet_or_slice: &[&str],
) -> Option<String> {
    let _ = (value, alignments, meet_or_slice);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The functions the catalog lists for a transform attribute.
    fn catalog_functions(attr: &str) -> &'static [&'static str] {
        let def = svg_data::attribute(attr).expect("catalog attribute");
        match def.values {
            svg_data::AttributeValues::Transform(functions) => functions,
            ref other => panic!("{attr} is not a transform list: {other:?}"),
        }
    }

    fn transform(value: &str) -> Option<String> {
        check_transform(value, catalog_functions("transform"))
    }

    #[track_caller]
    fn assert_valid_transforms(values: &[&str]) {
        for value in values {
            assert_eq!(transform(value), None, "{value:?} must be valid");
        }
    }

    #[track_caller]
    fn assert_invalid_transforms(values: &[&str]) {
        for value in values {
            assert!(transform(value).is_some(), "{value:?} must be invalid");
        }
    }

    // Claim: "transform-list: wsp* transforms? wsp*" with "transforms:
    // transform | transform comma-wsp? transforms" and "comma-wsp: (wsp+
    // comma? wsp*) | (comma wsp*)"
    // https://drafts.csswg.org/css-transforms-1/#svg-syntax
    #[test]
    fn transform_list_separators() {
        assert_valid_transforms(&[
            "",
            "   ",
            "\n\t ",
            "translate(10,20) rotate(45)",
            "translate(1,2)rotate(3)",
            "translate(1,2),scale(2)",
            "translate(1,2) , scale(2)",
            "translate(1,2)  rotate(-30) scale(2)",
            " \n translate(1) \t",
        ]);
        assert_invalid_transforms(&[
            ",translate(1)",
            "translate(1),",
            "translate(1),,scale(2)",
            "translate(1) , , scale(2)",
            ",",
            "translate(1) foo",
            "10",
            "translate(1);",
            "translate(1))",
            "{translate(1)}",
        ]);
    }

    // Claim: "translate: "translate" wsp* "(" wsp* number ( comma-wsp? number
    // )? wsp* ")""
    // https://drafts.csswg.org/css-transforms-1/#svg-syntax
    #[test]
    fn svg_translate() {
        assert_valid_transforms(&[
            "translate(10)",
            "translate(10,20)",
            "translate(10 20)",
            "translate(-10-20)",
            "translate(.5.5)",
            "translate (10 20)",
            "translate( 10 , 20 )",
            "translate(1e2, -1E-2)",
        ]);
        assert_invalid_transforms(&[
            "translate()",
            "translate(1 2 3)",
            "translate(1,,2)",
            "translate(,1)",
            "translate(1,)",
            "translate (10px 20px)",
            "translate(a)",
        ]);
    }

    // Claim: "scale: "scale" wsp* "(" wsp* number ( comma-wsp? number )? wsp*
    // ")""
    // https://drafts.csswg.org/css-transforms-1/#svg-syntax
    #[test]
    fn svg_scale() {
        assert_valid_transforms(&["scale(2)", "scale(2,-1)", "scale(.5 .5)", "scale (3)"]);
        assert_invalid_transforms(&["scale()", "scale(1 2 3)", "scale(1,,2)"]);
    }

    // Claim: "rotate: "rotate" wsp* "(" wsp* number ( comma-wsp? number
    // comma-wsp? number )? wsp* ")""
    // https://drafts.csswg.org/css-transforms-1/#svg-syntax
    #[test]
    fn svg_rotate() {
        assert_valid_transforms(&[
            "rotate(45)",
            "rotate(90,100,100)",
            "rotate(-45 10 10)",
            "rotate(90, 100,100)",
            "rotate(30 50-50)",
        ]);
        assert_invalid_transforms(&[
            "rotate()",
            "rotate(45 10)",
            "rotate(45, 10)",
            "rotate(1 2 3 4)",
            "rotate(45 10 10,)",
        ]);
    }

    // Claim: "skewX: "skewX" wsp* "(" wsp* number wsp* ")"" and "skewY:
    // "skewY" wsp* "(" wsp* number wsp* ")""
    // https://drafts.csswg.org/css-transforms-1/#svg-syntax
    #[test]
    fn svg_skew_x_and_skew_y() {
        assert_valid_transforms(&["skewX(30)", "skewY(-1e1)", "skewX( 30 )"]);
        assert_invalid_transforms(&["skewX()", "skewX(1 2)", "skewY(1,2)", "skewY(,1)"]);
    }

    // Claim: "matrix: "matrix" wsp* "(" wsp* number comma-wsp? number
    // comma-wsp? number comma-wsp? number comma-wsp? number comma-wsp? number
    // wsp* ")""
    // https://drafts.csswg.org/css-transforms-1/#svg-syntax
    #[test]
    fn svg_matrix() {
        assert_valid_transforms(&[
            "matrix(1 0 0 1 0 0)",
            "matrix(1,0,0,1,0,0)",
            "matrix(0.70710678,-0.70710678,0.70710678,0.70710678,200,200)",
            "matrix(25.61117 -6.17556 7.21294 29.91337 152.832 545.192)",
            "matrix(1, 0 0,1 0 , 0)",
        ]);
        assert_invalid_transforms(&[
            "matrix()",
            "matrix(1 0 0 1 0)",
            "matrix(1 0 0 1 0 0 0)",
            "matrix(1,0,0,1,0,0,)",
            "matrix(1 0 0 1 0 0px)",
        ]);
    }

    // Claim: "matrix() = matrix( <number>#{6} )"
    // https://drafts.csswg.org/css-transforms-1/#funcdef-transform-matrix
    // Claim: "A hash mark (#) indicates that the preceding type, word, or
    // group occurs one or more times, separated by comma tokens (which may
    // optionally be surrounded by white space and/or comments)."
    // https://drafts.csswg.org/css-values-4/#mult-comma
    #[test]
    fn css_matrix() {
        assert_valid_transforms(&["matrix(1, 0, 0, 1, 10, 20)", "matrix(1,/**/0,0,1,0,0)"]);
        assert_invalid_transforms(&["matrix(1, 0, 0, 1, 10)", "matrix(1, 0, 0, 1, 10px, 20px)"]);
    }

    // Claim: "translate() = translate( <length-percentage> ,
    // <length-percentage>? )"
    // https://drafts.csswg.org/css-transforms-1/#funcdef-transform-translate
    #[test]
    fn css_translate() {
        assert_valid_transforms(&[
            "translate(10px)",
            "translate(10px, 20%)",
            "translate(1em,2vh)",
            "translate(10px, 20)",
            "translate(0, 5mm)",
        ]);
        assert_invalid_transforms(&[
            "translate(10px 20px)",
            "translate(10deg)",
            "translate(10px,)",
            "translate(10px, 20px, 30px)",
            "translate(10foo)",
        ]);
    }

    // Claim: "translateX() = translateX( <length-percentage> )" and
    // "translateY() = translateY( <length-percentage> )"
    // https://drafts.csswg.org/css-transforms-1/#funcdef-transform-translatex
    #[test]
    fn css_translate_x_and_y() {
        assert_valid_transforms(&["translateX(10px)", "translateY(5%)", "translateX(-3)"]);
        assert_invalid_transforms(&[
            "translateX(1px, 2px)",
            "translateX()",
            "translateY(1s)",
            "translateY(1 2)",
        ]);
    }

    // Claim: "scale() = scale( [ <number> | <percentage> ]#{1,2} )"
    // https://drafts.csswg.org/css-transforms-2/#funcdef-scale
    #[test]
    fn css_scale() {
        assert_valid_transforms(&["scale(50%)", "scale(1, 50%)", "scale(2, 3)"]);
        assert_invalid_transforms(&["scale(1px)", "scale(1, 2, 3)", "scale(1deg, 2)"]);
    }

    // Claim: "scaleX() = scaleX( [ <number> | <percentage> ] )" and "scaleY()
    // = scaleY( [ <number> | <percentage> ] )"
    // https://drafts.csswg.org/css-transforms-2/#funcdef-scalex
    #[test]
    fn css_scale_x_and_y() {
        assert_valid_transforms(&["scaleX(2)", "scaleY(50%)", "scaleX(-1)"]);
        assert_invalid_transforms(&["scaleX(1, 2)", "scaleY(1px)", "scaleX()"]);
    }

    // Claim: "rotate() = rotate( [ <angle> | <zero> ] )"
    // https://drafts.csswg.org/css-transforms-1/#funcdef-transform-rotate
    #[test]
    fn css_rotate() {
        assert_valid_transforms(&[
            "rotate(45deg)",
            "rotate(0.25turn)",
            "rotate(1rad)",
            "rotate(100grad)",
            "rotate(0)",
        ]);
        assert_invalid_transforms(&[
            "rotate(45px)",
            "rotate(45deg, 10, 10)",
            "rotate(45deg 10 10)",
            "rotate(50%)",
        ]);
    }

    // Claim: "skew() = skew( [ <angle> | <zero> ] , [ <angle> | <zero> ]? )"
    // https://drafts.csswg.org/css-transforms-1/#funcdef-transform-skew
    #[test]
    fn css_skew() {
        assert_valid_transforms(&[
            "skew(10deg)",
            "skew(10deg, 20deg)",
            "skew(10, 20)",
            "skew(0)",
        ]);
        assert_invalid_transforms(&["skew(10deg 20deg)", "skew(1, 2, 3)", "skew(1px)", "skew()"]);
    }

    // Claim: "skewX() = skewX( [ <angle> | <zero> ] )" and "skewY() = skewY(
    // [ <angle> | <zero> ] )"
    // https://drafts.csswg.org/css-transforms-1/#funcdef-transform-skewx
    #[test]
    fn css_skew_x_and_y() {
        assert_valid_transforms(&["skewX(10deg)", "skewY(-0.1turn)"]);
        assert_invalid_transforms(&["skewX(10px)", "skewX(1deg, 2deg)", "skewY(5%)"]);
    }

    // Claim: "Replace all instances of <length> in grammar with [<length> |
    // <number>]." and "Replace all instances of <length-percentage> in
    // grammar with [<length-percentage> | <number>]." and "Replace all
    // instances of <angle> in grammar with [<angle> | <number>]."
    // https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value
    #[test]
    fn presentation_attribute_unitless_numbers() {
        assert_valid_transforms(&[
            "translate(10, 20px)",
            "translateX(10)",
            "rotate(45)",
            "skew(10, 20deg)",
            "skewY(7)",
            "translate3d(1, 2, 3)",
            "translateZ(5)",
            "rotate3d(1, 0, 0, 45)",
            "rotateX(30)",
        ]);
        assert_invalid_transforms(&["translate(10s)", "rotate(10px)", "translateZ(5%)"]);
    }

    // Claim: "matrix3d() = matrix3d( <number>#{16} )"
    // https://drafts.csswg.org/css-transforms-2/#funcdef-matrix3d
    #[test]
    fn css_matrix3d() {
        assert_valid_transforms(&["matrix3d(1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1)"]);
        assert_invalid_transforms(&[
            "matrix3d(1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0)",
            "matrix3d(1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1)",
            "matrix3d(1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1px)",
        ]);
    }

    // Claim: "translate3d() = translate3d( <length-percentage> ,
    // <length-percentage> , <length> )" and "translateZ() = translateZ(
    // <length> )"
    // https://drafts.csswg.org/css-transforms-2/#funcdef-translate3d
    #[test]
    fn css_translate3d_and_z() {
        assert_valid_transforms(&["translate3d(10px, 20%, 5px)", "translateZ(2em)"]);
        assert_invalid_transforms(&[
            "translate3d(10px, 20px)",
            "translate3d(1px, 2px, 3%)",
            "translateZ(1px, 2px)",
        ]);
    }

    // Claim: "scale3d() = scale3d( [ <number> | <percentage> ]#{3} )" and
    // "scaleZ() = scaleZ( [ <number> | <percentage> ] )"
    // https://drafts.csswg.org/css-transforms-2/#funcdef-scale3d
    #[test]
    fn css_scale3d_and_z() {
        assert_valid_transforms(&["scale3d(1, 50%, 2)", "scaleZ(3)", "scaleZ(10%)"]);
        assert_invalid_transforms(&["scale3d(1, 2)", "scale3d(1, 2, 3px)", "scaleZ(1px)"]);
    }

    // Claim: "rotate3d() = rotate3d( <number> , <number> , <number> , [
    // <angle> | <zero> ] )" and "rotateX() = rotateX( [ <angle> | <zero> ] )"
    // https://drafts.csswg.org/css-transforms-2/#funcdef-rotate3d
    #[test]
    fn css_rotate3d_and_axes() {
        assert_valid_transforms(&[
            "rotate3d(1, 1, 0, 45deg)",
            "rotateX(10deg)",
            "rotateY(0)",
            "rotateZ(1turn)",
        ]);
        assert_invalid_transforms(&[
            "rotate3d(1, 1, 45deg)",
            "rotate3d(1px, 1, 0, 45deg)",
            "rotateX(10deg, 20deg)",
            "rotateZ(5px)",
        ]);
    }

    // Claim: "perspective() = perspective( [ <length [0,∞]> | none ] )"
    // https://drafts.csswg.org/css-transforms-2/#funcdef-perspective
    // Claim: "If the value is outside the allowed range, then unless
    // otherwise specified, the declaration is invalid and must be ignored."
    // https://drafts.csswg.org/css-values-4/#numeric-ranges
    #[test]
    fn css_perspective() {
        assert_valid_transforms(&[
            "perspective(100px)",
            "perspective(none)",
            "perspective(0)",
            "perspective(NONE)",
        ]);
        assert_invalid_transforms(&[
            "perspective(-10px)",
            "perspective(10deg)",
            "perspective(auto)",
            "perspective(1px, 2px)",
        ]);
    }

    // Claim: "<transform-list> = <transform-function>+" and "The set of
    // allowed transform functions is given below."
    // https://drafts.csswg.org/css-transforms-1/#transform-functions
    #[test]
    fn unknown_transform_functions() {
        assert_invalid_transforms(&[
            "foo(1)",
            "translate4d(1px)",
            "-webkit-translate(1px)",
            "rotate(45) bogus(1)",
            "calc(1px)",
        ]);
        // Helper, not a spec claim: a function the catalog does not list for
        // the attribute.
        assert!(check_transform("translateX(1px)", &["translate"]).is_some());
        assert_eq!(check_transform("translate(1px)", &["translate"]), None);
    }

    // Claim: "Value: none | <transform-list>"
    // https://drafts.csswg.org/css-transforms-1/#propdef-transform
    // Claim: "These are attributes whose name matches (or is similar to) a
    // given CSS property and whose value is parsed as a value of that
    // property."
    // https://svgwg.org/svg2-draft/styling.html#TermPresentationAttribute
    #[test]
    fn transform_none() {
        assert_valid_transforms(&["none", " none ", "NONE", "None"]);
        assert_invalid_transforms(&[
            "none translate(1)",
            "translate(1) none",
            "none none",
            "none, rotate(1)",
        ]);
    }

    // Claim: "In addition to the property-specific values listed in their
    // definitions, all properties defined in this specification also accept
    // the CSS-wide keywords as their property value."
    // https://drafts.csswg.org/css-values-4/#common-keywords
    // Claim: "Like keywords, function names are ASCII case-insensitive."
    // https://drafts.csswg.org/css-values-4/#functional-notations
    // Claim: "Note that this allows CSS comments and escapes to be used in
    // such attributes."
    // https://svgwg.org/svg2-draft/types.html#attribute-css-value
    // Claim: "<eof-token> <)-token> Discard a token from input. Return
    // function."
    // https://drafts.csswg.org/css-syntax-3/#consume-function
    // Accept-only: values the check does not model must be accepted.
    #[test]
    fn transform_unmodelled_constructs_are_accepted() {
        assert_valid_transforms(&[
            "inherit",
            "initial",
            "unset",
            "revert",
            "revert-layer",
            "var(--t)",
            "translate(var(--x))",
            "translate(var(--x)) rotate(10)",
            "attr(data-t)",
            "translate(env(safe-area-inset-left), 2px)",
            "translate(calc(10px + 5%), 2px)",
            "rotate(calc(45deg * 2))",
            "matrix(calc(1), 0, 0, 1, 0, 0)",
            "translate(min(1px, 2%))",
            "TRANSLATE(10px)",
            "Rotate(45deg)",
            "SCALEX(2)",
            "translate(/**/10/**/)",
            "transl\\61te(10)",
            "translate(10px",
        ]);
    }

    // Claim: "Value: <transform-list>" for patternTransform and
    // gradientTransform, and "The patternTransform attribute is a
    // presentation attribute for the transform property."
    // https://svgwg.org/svg2-draft/pservers.html#PatternElementPatternTransformAttribute
    // https://svgwg.org/svg2-draft/pservers.html#LinearGradientElementGradientTransformAttribute
    // Real catalog values, dispatched through the catalog's value space.
    #[test]
    fn catalog_transform_attributes() {
        let cases: &[(&str, &[&str], &[&str])] = &[
            (
                "transform",
                &[
                    "translate(250,100)",
                    "rotate(90,100,100)",
                    "translate(50,50) rotate(-45) scale(2)",
                    "matrix(1,0,0,1,0,0)",
                    "scale(-1,-1)",
                ],
                &[
                    "translate(250,100) scale",
                    "rotate(90,100)",
                    "matrix(1 0 0 1)",
                ],
            ),
            (
                "gradientTransform",
                &[
                    "matrix(1.25,0,0,1.25,-112.5,-40)",
                    "matrix(104.362 0 0 104.085 186.533 511.617)",
                    "rotate(90)",
                ],
                &["matrix(104.362 0 0 104.085 186.533)", "rotate(90deg 1 1)"],
            ),
            (
                "patternTransform",
                &[
                    "rotate(45)",
                    "translate(20,10)",
                    "matrix(0.70710678,-0.70710678,0.70710678,0.70710678,200,200)",
                ],
                &["rotate(45,)", "translate(20,10) skewX()"],
            ),
        ];
        for (attr, valid, invalid) in cases {
            let values = &svg_data::attribute(attr).expect("catalog attribute").values;
            for value in *valid {
                assert_eq!(
                    crate::values::invalid_reason(values, value),
                    None,
                    "{attr}={value:?} must be valid"
                );
            }
            for value in *invalid {
                assert!(
                    crate::values::invalid_reason(values, value).is_some(),
                    "{attr}={value:?} must be invalid"
                );
            }
        }
    }

    /// The keyword sets of the catalog's preserveAspectRatio grammar
    /// `<align> <meetOrSlice>?`.
    const ALIGNMENTS: &[&str] = &[
        "none", "xMinYMin", "xMidYMin", "xMaxYMin", "xMinYMid", "xMidYMid", "xMaxYMid", "xMinYMax",
        "xMidYMax", "xMaxYMax",
    ];
    const MEET_OR_SLICE: &[&str] = &["meet", "slice"];

    fn par(value: &str) -> Option<String> {
        check_preserve_aspect_ratio(value, ALIGNMENTS, MEET_OR_SLICE)
    }

    #[track_caller]
    fn assert_valid_pars(values: &[&str]) {
        for value in values {
            assert_eq!(par(value), None, "{value:?} must be valid");
        }
    }

    #[track_caller]
    fn assert_invalid_pars(values: &[&str]) {
        for value in values {
            assert!(par(value).is_some(), "{value:?} must be invalid");
        }
    }

    // Claim: "preserveAspectRatio <align> <meetOrSlice>?" with "<align> = none
    // | xMinYMin | xMidYMin | xMaxYMin | xMinYMid | xMidYMid | xMaxYMid |
    // xMinYMax | xMidYMax | xMaxYMax" and "<meetOrSlice> = meet | slice"
    // https://svgwg.org/svg2-draft/coords.html#PreserveAspectRatioAttribute
    #[test]
    fn preserve_aspect_ratio_grammar() {
        let mut valid: Vec<String> = Vec::new();
        for align in ALIGNMENTS {
            valid.push((*align).to_owned());
            for mode in MEET_OR_SLICE {
                valid.push(format!("{align} {mode}"));
            }
        }
        let valid: Vec<&str> = valid.iter().map(String::as_str).collect();
        assert_valid_pars(&valid);
        // Real values from the SVG 2 example files.
        assert_valid_pars(&["xMidYMid meet", "xMinYMin slice", "xMaxYMax meet", "none"]);
        assert_invalid_pars(&[
            "",
            "   ",
            "meet",
            "slice xMidYMid",
            "xMidYMid meet slice",
            "xMidYMid meet extra",
            "xMidYMid foo",
            "center",
            "xMidYMid 1",
            "xMid YMid",
            "xMidYMid meet;",
        ]);
    }

    // Claim: "The <meetOrSlice> parameter is optional and, if provided, is
    // separated from the <align> value by one or more spaces"
    // https://svgwg.org/svg2-draft/coords.html#DataTypeAlign
    // Claim: "When any other attribute defined using the CSS Value Definition
    // Syntax is parsed, this is done by parsing the attribute's value
    // according to the grammar given in attribute definition table."
    // https://svgwg.org/svg2-draft/types.html#attribute-css-value
    #[test]
    fn preserve_aspect_ratio_separators() {
        assert_valid_pars(&[
            "xMidYMid  meet",
            "  xMidYMid\tmeet  ",
            "\nnone\n",
            "xMinYMin/**/slice",
        ]);
        assert_invalid_pars(&["xMidYMid,meet", "xMidYMid , meet", ",xMidYMid", "xMidYMid,"]);
    }

    // Claim: "Keywords are identifiers and are interpreted ASCII
    // case-insensitively (i.e., [a-z] and [A-Z] are equivalent)."
    // https://drafts.csswg.org/css-values-4/#keywords
    #[test]
    fn preserve_aspect_ratio_keywords_ignore_ascii_case() {
        assert_valid_pars(&["XMIDYMID MEET", "xmidymid slice", "NONE", "xMinYMin Slice"]);
        assert_invalid_pars(&["XMIDYMID MEAT", "x\u{131}dymid"]);
    }

    // Claim: "Removed the defer keyword from preserveAspectRatio."
    // https://svgwg.org/svg2-draft/changes.html#coords
    // Claim: "Attribute preserveAspectRatio="[defer] <align>
    // [<meetOrSlice>]""
    // https://www.w3.org/TR/SVG11/coords.html#PreserveAspectRatioAttribute
    #[test]
    fn preserve_aspect_ratio_defer() {
        assert_invalid_pars(&["defer xMidYMid", "defer xMidYMid meet", "defer"]);
        // A value space that still lists `defer` (SVG 1.1) accepts it as a
        // prefix only.
        let svg11: Vec<&str> = std::iter::once("defer")
            .chain(ALIGNMENTS.iter().copied())
            .collect();
        assert_eq!(
            check_preserve_aspect_ratio("defer xMidYMid meet", &svg11, MEET_OR_SLICE),
            None
        );
        assert_eq!(
            check_preserve_aspect_ratio("defer none", &svg11, MEET_OR_SLICE),
            None
        );
        assert!(check_preserve_aspect_ratio("defer", &svg11, MEET_OR_SLICE).is_some());
        assert!(check_preserve_aspect_ratio("xMidYMid defer", &svg11, MEET_OR_SLICE).is_some());
        assert!(check_preserve_aspect_ratio("defer defer none", &svg11, MEET_OR_SLICE).is_some());
    }

    // Claim: "Like keywords, function names are ASCII case-insensitive."
    // https://drafts.csswg.org/css-values-4/#functional-notations
    // Accept-only: arbitrary substitution functions are left to CSS.
    #[test]
    fn preserve_aspect_ratio_unmodelled_constructs_are_accepted() {
        assert_valid_pars(&["var(--par)", "xMidYMid var(--mode)", "VAR(--x)"]);
    }
}
