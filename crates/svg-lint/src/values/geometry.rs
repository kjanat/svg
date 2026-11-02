//! Transform lists and preserveAspectRatio.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.
//!
//! The `transform`, `patternTransform` and `gradientTransform` attributes
//! have two definitions that disagree: the legacy SVG grammar of CSS
//! Transforms 1 §6.2 (<https://drafts.csswg.org/css-transforms-1/#svg-syntax>:
//! unitless numbers only, commas or whitespace between arguments and between
//! functions, `rotate(a cx cy)`), and SVG 2's rule that a presentation
//! attribute is parsed as a value of its property
//! (<https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value>:
//! the CSS `transform` grammar `none | <transform-list>`, comma-separated
//! arguments, units, with a bare `<number>` admitted for lengths and angles).
//! A value is accepted when either definition accepts it.
//!
//! `preserveAspectRatio` follows SVG 2's `<align> <meetOrSlice>?`
//! (<https://svgwg.org/svg2-draft/coords.html#PreserveAspectRatioAttribute>),
//! parsed as CSS (<https://svgwg.org/svg2-draft/types.html#attribute-css-value>),
//! so keywords match ASCII case-insensitively and comments are allowed.

use super::syntax::cascade::is_css_wide_keyword_value;
use super::syntax::notation::{contains_arbitrary_substitution, is_whole_value};
use super::syntax::parser::{BlockKind, ComponentValue, parse_component_values};
use super::syntax::tokenizer::Token;
use super::syntax::types::{Match, NumericType, UnitlessMode, match_numeric};

/// The argument types of the CSS transform functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arg {
    /// `<number>`.
    Number,
    /// `[ <number> | <percentage> ]`.
    NumberOrPercentage,
    /// `<length-percentage>`.
    LengthPercentage,
    /// `<length>`.
    Length,
    /// `[ <angle> | <zero> ]`.
    Angle,
    /// `[ <length [0,∞]> | none ]`.
    Perspective,
}

/// A CSS transform function: CSS Transforms 1 §7.1 and CSS Transforms 2
/// §12 (<https://drafts.csswg.org/css-transforms-2/#three-d-transform-functions>).
struct CssFunction {
    /// The name as the specs write it.
    name: &'static str,
    /// The argument types, in order.
    args: &'static [Arg],
    /// How many trailing arguments may be omitted.
    optional: usize,
    /// The arguments both definitions accept, for the diagnostic.
    usage: &'static str,
    /// The argument counts of the legacy SVG form, for the six functions of
    /// CSS Transforms 1 §6.2.
    svg_counts: &'static [usize],
}

const fn css(
    name: &'static str,
    args: &'static [Arg],
    optional: usize,
    usage: &'static str,
) -> CssFunction {
    CssFunction {
        name,
        args,
        optional,
        usage,
        svg_counts: &[],
    }
}

const fn svg_and_css(
    name: &'static str,
    args: &'static [Arg],
    optional: usize,
    usage: &'static str,
    svg_counts: &'static [usize],
) -> CssFunction {
    CssFunction {
        name,
        args,
        optional,
        usage,
        svg_counts,
    }
}

const ANGLE: &[Arg] = &[Arg::Angle];
const NUMBER_OR_PERCENTAGE: &[Arg] = &[Arg::NumberOrPercentage];
const LENGTH_PERCENTAGE: &[Arg] = &[Arg::LengthPercentage];

/// Every transform function either definition allows.
const TRANSFORM_FUNCTIONS: &[CssFunction] = &[
    svg_and_css("matrix", &[Arg::Number; 6], 0, "six numbers", &[6]),
    svg_and_css(
        "translate",
        &[Arg::LengthPercentage; 2],
        1,
        "one or two numbers, lengths or percentages",
        &[1, 2],
    ),
    css(
        "translateX",
        LENGTH_PERCENTAGE,
        0,
        "one number, length or percentage",
    ),
    css(
        "translateY",
        LENGTH_PERCENTAGE,
        0,
        "one number, length or percentage",
    ),
    svg_and_css(
        "scale",
        &[Arg::NumberOrPercentage; 2],
        1,
        "one or two numbers or percentages",
        &[1, 2],
    ),
    css(
        "scaleX",
        NUMBER_OR_PERCENTAGE,
        0,
        "one number or percentage",
    ),
    css(
        "scaleY",
        NUMBER_OR_PERCENTAGE,
        0,
        "one number or percentage",
    ),
    svg_and_css(
        "rotate",
        ANGLE,
        0,
        "one angle or number, or three numbers (angle, cx, cy)",
        &[1, 3],
    ),
    css(
        "skew",
        &[Arg::Angle; 2],
        1,
        "one or two comma-separated angles or numbers",
    ),
    svg_and_css("skewX", ANGLE, 0, "one angle or number", &[1]),
    svg_and_css("skewY", ANGLE, 0, "one angle or number", &[1]),
    css(
        "matrix3d",
        &[Arg::Number; 16],
        0,
        "sixteen comma-separated numbers",
    ),
    css(
        "translate3d",
        &[Arg::LengthPercentage, Arg::LengthPercentage, Arg::Length],
        0,
        "two lengths or percentages and a length, separated by commas",
    ),
    css("translateZ", &[Arg::Length], 0, "one number or length"),
    css(
        "scale3d",
        &[Arg::NumberOrPercentage; 3],
        0,
        "three comma-separated numbers or percentages",
    ),
    css(
        "scaleZ",
        NUMBER_OR_PERCENTAGE,
        0,
        "one number or percentage",
    ),
    css(
        "rotate3d",
        &[Arg::Number, Arg::Number, Arg::Number, Arg::Angle],
        0,
        "three numbers and an angle, separated by commas",
    ),
    css("rotateX", ANGLE, 0, "one angle or number"),
    css("rotateY", ANGLE, 0, "one angle or number"),
    css("rotateZ", ANGLE, 0, "one angle or number"),
    css(
        "perspective",
        &[Arg::Perspective],
        0,
        "one non-negative length, or none",
    ),
];

/// `Transform`: a transform list using these functions.
///
/// `functions` is the catalog's list of the functions the attribute allows;
/// an empty list allows every transform function.
pub fn check_transform(value: &str, functions: &[&str]) -> Option<String> {
    let list = parse_component_values(value);
    if list.depth_limited
        || contains_arbitrary_substitution(&list.values)
        || is_whole_value(&list.values)
        || is_css_wide_keyword_value(value)
    {
        return None;
    }
    let items: Vec<&ComponentValue<'_>> = list.significant().collect();
    if let [only] = items.as_slice()
        && only
            .ident()
            .is_some_and(|ident| ident.eq_ignore_ascii_case("none"))
    {
        return None;
    }
    let mut seen_function = false;
    let mut after_comma = false;
    let mut items = items.into_iter().peekable();
    while let Some(item) = items.next() {
        let reason = match item {
            ComponentValue::Function(function) => {
                check_function(&function.name, &function.value, true, functions)
            }
            // The legacy form allows whitespace before the `(`:
            // `"translate" wsp* "("`.
            ComponentValue::Token(Token::Ident(name)) => match items.next_if(|next| {
                matches!(next, ComponentValue::Block(block) if block.kind == BlockKind::Paren)
            }) {
                Some(ComponentValue::Block(block)) => {
                    check_function(name, &block.value, false, functions)
                }
                _ if name.eq_ignore_ascii_case("none") => {
                    Some("none cannot be combined with transform functions".to_owned())
                }
                _ => Some(format!(
                    "expected a transform function, found {}",
                    describe(item)
                )),
            },
            ComponentValue::Token(Token::Comma) => {
                if !seen_function || after_comma {
                    return Some("a comma must sit between two transform functions".to_owned());
                }
                after_comma = true;
                continue;
            }
            other => Some(format!(
                "expected a transform function, found {}",
                describe(other)
            )),
        };
        if reason.is_some() {
            return reason;
        }
        seen_function = true;
        after_comma = false;
    }
    after_comma.then(|| "a transform list cannot end with a comma".to_owned())
}

/// Check one transform function. `function_token` is false for the legacy
/// `name (` form, which only the SVG grammar allows.
fn check_function(
    name: &str,
    args: &[ComponentValue<'_>],
    function_token: bool,
    functions: &[&str],
) -> Option<String> {
    let Some(known) = TRANSFORM_FUNCTIONS
        .iter()
        .find(|known| known.name.eq_ignore_ascii_case(name))
    else {
        return Some(format!("unknown transform function {name}()"));
    };
    if !functions.is_empty()
        && !functions
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(known.name))
    {
        return Some(format!("{}() is not allowed here", known.name));
    }
    let svg = if known.svg_counts.is_empty() {
        Match::No
    } else {
        match_svg_args(args, known.svg_counts)
    };
    let css = if function_token {
        match_css_args(args, known)
    } else {
        Match::No
    };
    svg.or(css)
        .is_no()
        .then(|| format!("{}() takes {}", known.name, known.usage))
}

/// The legacy arguments: `number ( comma-wsp? number )*` with one of
/// `counts` numbers, `wsp*` around them.
fn match_svg_args(args: &[ComponentValue<'_>], counts: &[usize]) -> Match {
    let mut numbers = 0;
    let mut after_comma = false;
    for arg in args.iter().filter(|arg| !arg.is_whitespace()) {
        match arg {
            ComponentValue::Token(Token::Number(_)) => {
                numbers += 1;
                after_comma = false;
            }
            ComponentValue::Token(Token::Comma) if numbers > 0 && !after_comma => {
                after_comma = true;
            }
            _ => return Match::No,
        }
    }
    Match::from(!after_comma && counts.contains(&numbers))
}

/// The CSS arguments: comma-separated, one component value each.
fn match_css_args(args: &[ComponentValue<'_>], function: &CssFunction) -> Match {
    let groups: Vec<&[ComponentValue<'_>]> = if args.iter().all(ComponentValue::is_whitespace) {
        Vec::new()
    } else {
        args.split(|arg| matches!(arg, ComponentValue::Token(Token::Comma)))
            .collect()
    };
    let max = function.args.len();
    if groups.len() < max - function.optional || groups.len() > max {
        return Match::No;
    }
    let mut result = Match::Yes;
    for (group, &ty) in groups.iter().zip(function.args) {
        let mut significant = group.iter().filter(|value| !value.is_whitespace());
        let (Some(value), None) = (significant.next(), significant.next()) else {
            return Match::No;
        };
        match match_arg(value, ty) {
            Match::No => return Match::No,
            Match::Unknown => result = Match::Unknown,
            Match::Yes => {}
        }
    }
    result
}

/// One CSS argument, with the SVG 2 presentation attribute rule that admits
/// a bare `<number>` for `<length>`, `<length-percentage>` and `<angle>`.
fn match_arg(value: &ComponentValue<'_>, ty: Arg) -> Match {
    let mode = UnitlessMode::PresentationAttribute;
    match ty {
        Arg::Number => match_numeric(value, NumericType::Number, mode),
        Arg::NumberOrPercentage => match_numeric(value, NumericType::Number, mode)
            .or(match_numeric(value, NumericType::Percentage, mode)),
        Arg::LengthPercentage => match_numeric(value, NumericType::LengthPercentage, mode),
        Arg::Length => match_numeric(value, NumericType::Length, mode),
        Arg::Angle => match_numeric(value, NumericType::Angle, mode),
        Arg::Perspective => {
            if value
                .ident()
                .is_some_and(|ident| ident.eq_ignore_ascii_case("none"))
            {
                return Match::Yes;
            }
            // `<length [0,∞]>`: a negative length literal is out of range. A
            // bare number stands for the substituted `<number>`, which has no
            // range.
            if let ComponentValue::Token(Token::Dimension { number, .. }) = value
                && number.value < 0.0
            {
                return Match::No;
            }
            match_numeric(value, NumericType::Length, mode)
        }
    }
}

/// The SVG 2 `<align>` keywords.
const SVG2_ALIGNMENTS: &[&str] = &[
    "none", "xMinYMin", "xMidYMin", "xMaxYMin", "xMinYMid", "xMidYMid", "xMaxYMid", "xMinYMax",
    "xMidYMax", "xMaxYMax",
];

/// The SVG 2 `<meetOrSlice>` keywords.
const SVG2_MEET_OR_SLICE: &[&str] = &["meet", "slice"];

/// `PreserveAspectRatio`: alignment plus optional meet/slice.
///
/// An empty keyword list stands for the SVG 2 keywords. `defer` (SVG 1.1)
/// is accepted before the alignment only when `alignments` lists it.
pub fn check_preserve_aspect_ratio(
    value: &str,
    alignments: &[&str],
    meet_or_slice: &[&str],
) -> Option<String> {
    let list = parse_component_values(value);
    if list.depth_limited || contains_arbitrary_substitution(&list.values) {
        return None;
    }
    let alignments = if alignments.is_empty() {
        SVG2_ALIGNMENTS
    } else {
        alignments
    };
    let meet_or_slice = if meet_or_slice.is_empty() {
        SVG2_MEET_OR_SLICE
    } else {
        meet_or_slice
    };
    let is_defer = |item: &ComponentValue<'_>| {
        item.ident()
            .is_some_and(|ident| ident.eq_ignore_ascii_case("defer"))
    };
    let mut items = list.significant();
    let mut first = items.next();
    if first.is_some_and(is_defer) {
        if !contains_keyword(alignments, "defer") {
            return Some("the defer keyword was removed in SVG 2".to_owned());
        }
        first = items.next();
    }
    let Some(align) = first else {
        return Some("expected an alignment such as xMidYMid".to_owned());
    };
    if is_defer(align)
        || !align
            .ident()
            .is_some_and(|ident| contains_keyword(alignments, ident))
    {
        return Some(format!(
            "expected an alignment ({}), found {}",
            alignments
                .iter()
                .filter(|keyword| !keyword.eq_ignore_ascii_case("defer"))
                .copied()
                .collect::<Vec<_>>()
                .join(" | "),
            describe(align)
        ));
    }
    if let Some(mode) = items.next()
        && !mode
            .ident()
            .is_some_and(|ident| contains_keyword(meet_or_slice, ident))
    {
        return Some(format!(
            "expected {} after the alignment, found {}",
            meet_or_slice.join(" or "),
            describe(mode)
        ));
    }
    items
        .next()
        .map(|extra| format!("unexpected {} at the end", describe(extra)))
}

/// Whether `list` holds `keyword`, ignoring ASCII case.
fn contains_keyword(list: &[&str], keyword: &str) -> bool {
    list.iter().any(|known| known.eq_ignore_ascii_case(keyword))
}

/// A short description of a component value for a diagnostic.
fn describe(value: &ComponentValue<'_>) -> String {
    match value {
        ComponentValue::Token(Token::Ident(ident)) => format!("`{ident}`"),
        ComponentValue::Function(function) => format!("`{}()`", function.name),
        ComponentValue::Token(Token::Comma) => "a comma".to_owned(),
        ComponentValue::Token(
            Token::Number(_) | Token::Percentage(_) | Token::Dimension { .. },
        ) => "a number".to_owned(),
        ComponentValue::Block(_) => "a bracketed block".to_owned(),
        ComponentValue::Token(_) => "an unexpected token".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The catalog's value space for an attribute.
    fn catalog_values(attr: &str) -> &'static svg_data::AttributeValues {
        let Some(def) = svg_data::attribute(attr) else {
            panic!("{attr} is not in the catalog");
        };
        &def.values
    }

    /// The functions the catalog lists for a transform attribute.
    fn catalog_functions(attr: &str) -> &'static [&'static str] {
        match catalog_values(attr) {
            svg_data::AttributeValues::Transform(functions) => functions,
            other => panic!("{attr} is not a transform list: {other:?}"),
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
            ",",
            "translate(1) foo",
            "10",
            "translate(1);",
            "translate(1))",
            "{translate(1)}",
        ]);
    }

    // Claim: "transforms: transform | transform comma-wsp+ transforms" and
    // "fractional-constant: digit-sequence? "." digit-sequence |
    // digit-sequence ".""
    // https://www.w3.org/TR/SVG11/coords.html#TransformAttribute
    // SVG 1.1 documents are linted against the SVG 1.1 grammar, which allows
    // several separators between functions and a number ending in a dot.
    #[test]
    fn svg11_transform_list() {
        assert_valid_transforms(&[
            "translate(1),,scale(2)",
            "translate(1) , , scale(2)",
            "translate(1)\n,\n,\nscale(2)",
            "translate(1.)",
            "translate(1. 2.)",
            "scale(1.e2)",
            "rotate(45. 10. 10.)",
            "matrix(1. 0 0 1. 0 0)",
            "translate(1.,2.),,rotate(3.E-1)",
        ]);
        assert_invalid_transforms(&[
            ",,translate(1)",
            "translate(1),,",
            "translate(1..)",
            "translate(.)",
            "translate(1.,)",
            "translate(1.) 2",
            "translate(1.e)",
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
            let values = catalog_values(attr);
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
