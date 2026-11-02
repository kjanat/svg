//! Path data and animation number/coordinate lists.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `PathData`: SVG path data.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_path_data(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `SemicolonNumberList`: semicolon-separated numbers.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_semicolon_number_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `CoordinatePair`: one coordinate pair.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_coordinate_pair(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `CoordinatePairList`: semicolon-separated coordinate pairs.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_coordinate_pair_list(value: &str) -> Option<String> {
    let _ = value;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assert `check` accepts every value in `values`.
    #[track_caller]
    fn assert_valid(check: fn(&str) -> Option<String>, values: &[&str]) {
        for value in values {
            assert_eq!(check(value), None, "{value:?} must be valid");
        }
    }

    /// Assert `check` rejects every value in `values`.
    #[track_caller]
    fn assert_invalid(check: fn(&str) -> Option<String>, values: &[&str]) {
        for value in values {
            assert!(check(value).is_some(), "{value:?} must be invalid");
        }
    }

    // ---------------------------------------------------------------------
    // Path data
    // ---------------------------------------------------------------------

    // "svg_path::= wsp* (moveto (wsp* drawto_command)*)? wsp*"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_svg_path_moveto_first_and_empty_allowed() {
        assert_valid(
            check_path_data,
            &[
                "",
                "   ",
                "\t\n\r\x0C ",
                "M0 0",
                "m 10 10",
                "  M 0 0 L 100 100  ",
                "M0 0 Z M5 5 Z",
                "M0,0zm5,5z",
            ],
        );
        assert_invalid(
            check_path_data,
            &[
                "L 10 10",
                "Z",
                "10 10",
                ",M0 0",
                "M0 0 junk",
                "M0 0 X 1 2",
                "M",
                "M 10",
            ],
        );
    }

    // "drawto_command::= moveto | closepath | lineto | horizontal_lineto
    //  | vertical_lineto | curveto | smooth_curveto | quadratic_bezier_curveto
    //  | smooth_quadratic_bezier_curveto | elliptical_arc"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_drawto_command_letters() {
        assert_valid(
            check_path_data,
            &[
                "M0 0L1 1l1 1H2h2V3v3C1 1 2 2 3 3c1 1 2 2 3 3S1 1 2 2s1 1 2 2Q1 1 2 2q1 1 2 2T1 \
                 1t1 1A1 1 0 0 1 5 5a1 1 0 0 1 5 5Zz",
            ],
        );
        assert_invalid(
            check_path_data,
            &[
                "M0 0 B 1 2",
                "M0 0 R 1 2 3 4",
                "M0 0 e 1",
                "M0 0 \u{141} 1 1",
            ],
        );
    }

    // "moveto::= ( "M" | "m" ) wsp* coordinate_pair_sequence"
    // "lineto::= ("L"|"l") wsp* coordinate_pair_sequence"
    // "coordinate_pair_sequence::= coordinate_pair | (coordinate_pair comma_wsp? coordinate_pair_sequence)"
    // "coordinate_pair::= coordinate comma_wsp? coordinate"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_coordinate_pair_sequence_and_implicit_commands() {
        assert_valid(
            check_path_data,
            &[
                "M 10 10 20 20 30 30",
                "M10,10,20,20",
                "M0 0 L 1 2 3 4 5 6",
                "M0 0 l1,2,3,4",
                "M0 0 T 1 2 3 4",
                "M0 0H 1 2 3",
                "M0 0 v1,2,3",
            ],
        );
        assert_invalid(
            check_path_data,
            &[
                "M 10 10 20",
                "M0 0 L 1",
                "M0 0 L",
                "M0 0 L 1 2 3",
                "M0 0 T 1 2 3",
                "M0 0 H",
                "M0 0 V,1",
                "M,0 0",
                "M0 0 L1 2,",
                "M0 0, L1 2",
            ],
        );
    }

    // "closepath::= ("Z" | "z")"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_closepath_takes_no_arguments() {
        assert_valid(
            check_path_data,
            &["M0 0 Z", "M0 0z L 1 1", "M0 0 ZZ", "M0 0 z m1 1"],
        );
        assert_invalid(check_path_data, &["M0 0 Z 1 2", "M0 0 z1", "M0 0 Z,"]);
    }

    // "curveto_coordinate_sequence::= coordinate_pair_triplet
    //  | (coordinate_pair_triplet comma_wsp? curveto_coordinate_sequence)"
    // "smooth_curveto_coordinate_sequence::= coordinate_pair_double
    //  | (coordinate_pair_double comma_wsp? smooth_curveto_coordinate_sequence)"
    // "quadratic_bezier_curveto_coordinate_sequence::= coordinate_pair_double
    //  | (coordinate_pair_double comma_wsp? quadratic_bezier_curveto_coordinate_sequence)"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_curve_argument_groups() {
        assert_valid(
            check_path_data,
            &[
                "M0 0 C 1 2 3 4 5 6",
                "M0 0 C 1 2 3 4 5 6 7 8 9 10 11 12",
                "M0 0c1,2,3,4,5,6,7,8,9,10,11,12",
                "M0 0 S 1 2 3 4 5 6 7 8",
                "M0 0 Q 1 2 3 4",
                "M0 0 q1 2 3 4 5 6 7 8",
            ],
        );
        assert_invalid(
            check_path_data,
            &[
                "M0 0 C 1 2 3 4 5",
                "M0 0 C 1 2 3 4 5 6 7",
                "M0 0 S 1 2 3",
                "M0 0 Q 1 2 3 4 5 6",
                "M0 0 C",
            ],
        );
    }

    // "elliptical_arc_argument::= number comma_wsp? number comma_wsp? number comma_wsp
    //  flag comma_wsp? flag comma_wsp? coordinate_pair"
    // "flag::= ("0" | "1")"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_arc_arguments_and_flags() {
        assert_valid(
            check_path_data,
            &[
                "M0 0 A 25 25 30 0 1 50 -25",
                "M0 0 a1,1,0,0,1,5,5",
                "M0 0 A1 1 0 01 5 5",
                "M0 0 A1 1 0 1110 10",
                "M0 0 a1 1 0 1 0 5 5 1 1 0 0 0 6 6",
                "M0 0 A1 1 0 0,1 5 5",
                "M0 0 A1 1 0 0 1-5-5",
            ],
        );
        assert_invalid(
            check_path_data,
            &[
                "M0 0 A 1 1 0 2 1 5 5",
                "M0 0 A 1 1 0 0 -1 5 5",
                "M0 0 A 1 1 0 0.5 1 5 5",
                "M0 0 A 1 1 0 0 1 5",
                "M0 0 A 1 1 0 0",
                "M0 0 A 1 1 0 0 1 5 5 1",
                "M0 0 A 1 1 0 true 1 5 5",
            ],
        );
    }

    // "Arbitrary numerical values are permitted for all elliptical arc parameters
    //  (other than the boolean flags)"
    // https://svgwg.org/svg2-draft/paths.html#ArcOutOfRangeParameters
    #[test]
    fn claim_arc_parameters_accept_any_number() {
        assert_valid(
            check_path_data,
            &[
                "M0 0 A -10 -20 -45 0 1 5 5",
                "M0 0 A +10 +20 +45 1 0 5 5",
                "M0 0 A 0 0 720 0 0 5 5",
            ],
        );
        assert_invalid(check_path_data, &["M0 0 A 10 20 45 -1 0 5 5"]);
    }

    // "number::= fractional-constant exponent?"
    // "fractional-constant::= (digit* "." digit+) | digit+"
    // "exponent::= ("e" | "E") sign? digit+"
    // "Authors and authoring tools must not use the disallowed number format."
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_number_production() {
        assert_valid(
            check_path_data,
            &[
                "M.5 .5",
                "M-.5-.5",
                "M1e5 1E-5",
                "M1e+5,+2",
                "M 0010 007",
                "M1.25e2 3",
            ],
        );
        assert_invalid(
            check_path_data,
            &[
                "M23. 5",
                "M10.,20",
                "M 1 2.",
                "M . 5",
                "M 1e 5",
                "M ++1 2",
                "M 1px 2",
                "M 50% 2",
                "M Infinity 2",
            ],
        );
    }

    // "The processing of the EBNF must consume as much of a given EBNF
    //  production as possible, stopping at the point when a character is
    //  encountered which no longer satisfies the production."
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_greedy_processing() {
        assert_valid(
            check_path_data,
            &["M 100-200", "M 0.6.5", "M1-2-3-4", "M.1.2.3.4", "M1e2-3"],
        );
        assert_invalid(check_path_data, &["M 0.6.5.4", "M1-2-3"]);
    }

    // "comma_wsp::= (wsp+ ","? wsp*) | ("," wsp*)"
    // "wsp::= (#x9 | #x20 | #xA | #xC | #xD)"
    // https://svgwg.org/svg2-draft/paths.html#PathDataBNF
    #[test]
    fn claim_comma_wsp_and_wsp() {
        assert_valid(
            check_path_data,
            &[
                "M 1 , 2",
                "M1 ,2",
                "M1, 2",
                "M\t1\n2\r3\x0C4",
                "M 1 2 , 3 4",
                "\x0CM0 0\x0C",
            ],
        );
        assert_invalid(
            check_path_data,
            &[
                "M1,,2",
                "M1 , , 2",
                "M1\u{a0}2",
                "M1\u{3000}2",
                "M 1 2 ,, 3 4",
            ],
        );
    }

    // "Unrecognized contents within a path data stream (i.e., contents that
    //  are not part of the path data grammar) is an error."
    // https://svgwg.org/svg2-draft/paths.html#PathDataErrorHandling
    #[test]
    fn claim_error_handling_does_not_make_errors_valid() {
        assert_valid(check_path_data, &["M 10,10 L 20,20"]);
        assert_invalid(
            check_path_data,
            &["M 10,10 L 20,20,30", "M 10 10 L 20 20 #", "M0 0 none"],
        );
    }

    // animateMotion: "path | svg-path [EBNF]"
    // https://w3c.github.io/svgwg/specs/animations/#AnimateMotionElementPathAttribute
    // textPath: "A path data string describing the path onto which the
    //  typographic characters will be rendered."
    // https://svgwg.org/svg2-draft/text.html#TextPathElementPathAttribute
    #[test]
    fn claim_path_attribute_on_animate_motion_and_text_path() {
        assert_valid(
            check_path_data,
            &[
                "M 0 0 L 100 100",
                "M-25,-12.5 L25,-12.5 L 0,-87.5 z",
                "M 100 200 C 200 100 300   0 400 100 C 500 200 600 300 700 200",
                "",
            ],
        );
        assert_invalid(check_path_data, &["none", "url(#p)", "path('M0 0')"]);
    }

    // ---------------------------------------------------------------------
    // keyTimes / keyPoints
    // ---------------------------------------------------------------------

    // keyTimes: "<number> [; <number>]* ;?"
    // https://w3c.github.io/svgwg/specs/animations/#KeyTimesAttribute
    #[test]
    fn claim_key_times_grammar() {
        assert_valid(
            check_semicolon_number_list,
            &[
                "0;1",
                "0; .25; 0.5; 1",
                "0",
                "0 ; 0.5 ; 1",
                "0;0.5;1;",
                "  0;1  ",
            ],
        );
        assert_invalid(
            check_semicolon_number_list,
            &[
                "", "   ", ";", "0;;1", ";0;1", "0;1;;", "0 1", "0,1", "0;0.5s;1", "0;50%;1",
                "0;auto;1", "0;1.;", "0;(1)", "inherit",
            ],
        );
    }

    // "Each time value in the keyTimes list is specified as a floating point
    //  value between 0 and 1 (inclusive)"
    // https://w3c.github.io/svgwg/specs/animations/#KeyTimesAttribute
    #[test]
    fn claim_key_times_range() {
        assert_valid(
            check_semicolon_number_list,
            &["0;1", "-0;1", "0;1e0", "0;5e-1;1", "0.0;1.0", "+0;+1"],
        );
        assert_invalid(
            check_semicolon_number_list,
            &["0;2", "-0.1;1", "0;1.0001", "0;1e1", "-1"],
        );
    }

    // "If the last semicolon separator is followed by either just white space
    //  or no more characters, ignore both the separator and the trailing white
    //  space."
    // https://w3c.github.io/svgwg/specs/animations/#KeyTimesAttribute
    #[test]
    fn claim_trailing_semicolon_ignored() {
        assert_valid(
            check_semicolon_number_list,
            &["0.0; 0.5;", "0;1; ", "0;1;\n\t"],
        );
        assert_invalid(check_semicolon_number_list, &["0;1;;", "0;1; ;"]);
    }

    // keyPoints: "<number> [; <number>]* ;?" and "keyPoints takes a
    //  semicolon-separated list of floating point values between 0 and 1"
    // https://w3c.github.io/svgwg/specs/animations/#KeyPointsAttribute
    #[test]
    fn claim_key_points_grammar_and_range() {
        assert_valid(
            check_semicolon_number_list,
            &["0;0.5;1", "1;0", "0.75;0.25;1;0"],
        );
        assert_invalid(check_semicolon_number_list, &["0;1.5", "0 0.5 1", "0;;1"]);
    }

    // "When any other attribute defined using the CSS Value Definition Syntax
    //  is parsed, this is done by parsing the attribute's value according to
    //  the grammar given in attribute definition table."
    // https://svgwg.org/svg2-draft/types.html#attribute-css-value
    #[test]
    fn claim_number_list_is_parsed_as_css() {
        assert_valid(
            check_semicolon_number_list,
            &[
                "0/* start */;1",
                "0;calc(1/2);1",
                "0;var(--t);1",
                "var(--times)",
                "0;\t1",
                "0;sin(1);1",
            ],
        );
        assert_invalid(check_semicolon_number_list, &["0;1px", "0;{1}"]);
    }

    // ---------------------------------------------------------------------
    // animateMotion from/to/by and values
    // ---------------------------------------------------------------------

    // "For animateMotion, the specified values for from, by, to and values
    //  consists of x, y coordinate pairs, with a single comma and/or white
    //  space separating the x coordinate from the y coordinate."
    // https://w3c.github.io/svgwg/specs/animations/#AnimateMotionElement
    // "coordinate-pair ::= ( coordinate comma-wsp coordinate )"
    // https://www.w3.org/TR/2001/REC-smil-animation-20010904/#animateMotionElement
    #[test]
    fn claim_coordinate_pair_separator() {
        assert_valid(
            check_coordinate_pair,
            &["33,15", "33 15", "33 , 15", "-1.5,+2", "0\t0", "1e2 3"],
        );
        assert_invalid(
            check_coordinate_pair,
            &[
                "", "33", "33,", ",33", "33,,15", "33 15 7", "33,15,7", "33;15", "1-2",
            ],
        );
    }

    // "Each coordinate represents a <length>."
    // https://w3c.github.io/svgwg/specs/animations/#AnimateMotionElement
    // "length ::= number ("em" | "ex" | "px" | "in" | "cm" | "mm" | "pt" | "pc" | "%")?"
    // https://www.w3.org/TR/SVG11/types.html#DataTypeLength
    #[test]
    fn claim_coordinates_are_lengths() {
        assert_valid(
            check_coordinate_pair,
            &[
                "10mm,20mm",
                "1px 2em",
                "50% 25%",
                "1PX 2Cm",
                "1vw,1vh",
                "calc(1px + 2px) 3px",
            ],
        );
        assert_invalid(
            check_coordinate_pair,
            &["10s,20", "1deg 2", "auto 2", "10 20foo", "#1 2", "\"1\" 2"],
        );
    }

    // "Leading and trailing white space, and white space before and after
    //  semicolon separators, will be ignored."
    // https://www.w3.org/TR/2001/REC-smil-animation-20010904/#AnimFuncValues
    #[test]
    fn claim_animation_values_trim_whitespace() {
        assert_valid(check_coordinate_pair, &["  33,15  ", "\n10 20\t"]);
        assert_valid(
            check_coordinate_pair_list,
            &[" 10,20 ; 30,20 ;30,40 ", "\n0 0;\n10 10\n"],
        );
        assert_invalid(check_coordinate_pair, &["  33  "]);
    }

    // "Let values be a list of strings formed by splitting attribute at each
    //  U+003B SEMICOLON character." ... "Remove the final value in values if
    //  it is the empty string."
    // https://w3c.github.io/svgwg/specs/animations/#ValuesAttribute
    // "If provided, the values attribute must consists of a list of x, y
    //  coordinate pairs."
    // https://w3c.github.io/svgwg/specs/animations/#AnimateMotionElement
    #[test]
    fn claim_coordinate_pair_list() {
        assert_valid(
            check_coordinate_pair_list,
            &[
                "10,20;30,20;30,40",
                "10mm,20mm;30mm,20mm;30mm,40mm",
                "0 0",
                "0,0;100,100;",
                "0,0; 100,100 ; ",
            ],
        );
        assert_invalid(
            check_coordinate_pair_list,
            &[
                "10,20;;30,40",
                ";10,20",
                "10,20;30",
                "10,20,30,40",
                "10,20;30,40;;",
                "10 20 30 40",
                "10,20;auto",
            ],
        );
    }
}
