//! Colors and paints.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

/// `Color`: a CSS `<color>`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_color(value: &str) -> Option<String> {
    let _ = value;
    None
}

/// `Paint`: an SVG `<paint>`.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_paint(value: &str) -> Option<String> {
    let _ = value;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    type Check = fn(&str) -> Option<String>;

    #[track_caller]
    fn assert_valid(check: Check, values: &[&str]) {
        for value in values {
            assert_eq!(check(value), None, "{value:?} should be valid");
        }
    }

    #[track_caller]
    fn assert_invalid(check: Check, values: &[&str]) {
        for value in values {
            assert!(check(value).is_some(), "{value:?} should be invalid");
        }
    }

    // Claim: "<color> = <color-base> | currentColor | <system-color>" and
    // "<color-base> = <hex-color> | <color-function> | <named-color> | transparent"
    // https://drafts.csswg.org/css-color-4/#color-syntax
    #[test]
    fn color_is_one_of_the_color_forms() {
        assert_valid(
            check_color,
            &[
                "currentcolor",
                "currentColor",
                "CURRENTCOLOR",
                "transparent",
                "Transparent",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "", "   ", "foo", "12", "10px", "50%", "red blue", "red,", "\"red\"", "none",
                "auto", "(red)", "[red]", "red)",
            ],
        );
    }

    // Claim: "A <named-color> is written as an <ident>, accepted anywhere a
    // <color> is. As usual for CSS-defined <ident>s, all of these keywords are
    // ASCII case-insensitive."
    // https://drafts.csswg.org/css-color-4/#named-colors
    #[test]
    fn named_colors() {
        assert_valid(
            check_color,
            &[
                "red",
                "RebeccaPurple",
                "lightgoldenrodyellow",
                "grey",
                "GRAY",
                "  red  ",
                "\tred\n",
                "red/**/",
                "/* c */ red",
            ],
        );
        assert_invalid(
            check_color,
            &["lightgoldenrod", "reds", "re d", "rebecca-purple", "grey50"],
        );
    }

    // Claim: "The <system-color> keywords are defined as follows: AccentColor
    // AccentColorText ActiveText ButtonBorder ButtonFace ButtonText Canvas
    // CanvasText Field FieldText GrayText Highlight HighlightText LinkText Mark
    // MarkText SelectedItem SelectedItemText VisitedText" and "Note: As with
    // all other keywords, these names are ASCII case-insensitive."
    // https://drafts.csswg.org/css-color-4/#css-system-colors
    #[test]
    fn system_colors() {
        assert_valid(
            check_color,
            &[
                "Canvas",
                "canvastext",
                "AccentColorText",
                "VisitedText",
                "MARK",
                "SelectedItemText",
            ],
        );
        assert_invalid(check_color, &["CanvasColor", "AccentColour"]);
    }

    // Claim: "User agents must support these keywords, and to mitigate
    // fingerprinting must map them to the (undeprecated) system colors as
    // listed below." with "<deprecated-color> = ActiveBorder | ActiveCaption
    // | AppWorkspace | Background | ButtonHighlight | ButtonShadow |
    // CaptionText | InactiveBorder | InactiveCaption | InactiveCaptionText |
    // InfoBackground | InfoText | Menu | MenuText | Scrollbar |
    // ThreeDDarkShadow | ThreeDFace | ThreeDHighlight | ThreeDLightShadow |
    // ThreeDShadow | Window | WindowFrame | WindowText"
    // https://drafts.csswg.org/css-color-4/#deprecated-system-colors
    #[test]
    fn deprecated_system_colors() {
        assert_valid(
            check_color,
            &[
                "ActiveBorder",
                "threeddarkshadow",
                "WindowText",
                "Background",
            ],
        );
        assert_invalid(check_color, &["ThreeDColor", "WindowBorder"]);
    }

    // Claim: "The syntax of a <hex-color> is a <hash-token> token whose value
    // consists of 3, 4, 6, or 8 hexadecimal digits."
    // https://drafts.csswg.org/css-color-4/#hex-notation
    #[test]
    fn hex_colors() {
        assert_valid(
            check_color,
            &["#fff", "#FFFF", "#a1b2c3", "#A1B2C3D4", "#000", " #A8F "],
        );
        assert_invalid(
            check_color,
            &[
                "#",
                "#f",
                "#ff",
                "#fffff",
                "#fffffff",
                "#fffffffff",
                "#ggg",
                "#12345g",
                "# fff",
                "#fff #000",
                "#-ff",
            ],
        );
    }

    // Claim: "rgb() = [ <legacy-rgb-syntax> | <modern-rgb-syntax> ]
    // rgba() = [ <legacy-rgba-syntax> | <modern-rgba-syntax> ]
    // <legacy-rgb-syntax> = rgb( <percentage>#{3} , <alpha-value>? ) |
    //                   rgb( <number>#{3} , <alpha-value>? )
    // <legacy-rgba-syntax> = rgba( <percentage>#{3} , <alpha-value>? ) |
    //                   rgba( <number>#{3} , <alpha-value>? )"
    // https://drafts.csswg.org/css-color-4/#rgb-functions
    #[test]
    fn rgb_legacy_syntax() {
        assert_valid(
            check_color,
            &[
                "rgb(255, 0, 0)",
                "rgb(255,0,0)",
                "rgba(255, 0, 0, 0.5)",
                "rgb(100%, 0%, 0%)",
                "rgba(100%,0%,0%,50%)",
                "rgb(255, 0, 0, 50%)",
                "rgba(255, 0, 0)",
                "RGB(1,2,3)",
                "rgb( 1 , 2 , 3 )",
                "rgb(-10, 300, 1e2)",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "rgb(255, 0%, 0)",
                "rgb(255, 0)",
                "rgb(1, 2, 3, 4, 5)",
                "rgb(none, 0, 0)",
                "rgb(1deg, 2, 3)",
                "rgb(255, 0 0)",
                "rgb(1 2 3, 0.5)",
                "rgba(1, 2, 3, auto)",
            ],
        );
    }

    // Claim: "<modern-rgb-syntax> = rgb(
    //   [ <number> | <percentage> | none]{3}
    //   [ / [<alpha-value> | none] ]?  )
    // <modern-rgba-syntax> = rgba(
    //   [ <number> | <percentage> | none]{3}
    //   [ / [<alpha-value> | none] ]?  )"
    // https://drafts.csswg.org/css-color-4/#rgb-functions
    #[test]
    fn rgb_modern_syntax() {
        assert_valid(
            check_color,
            &[
                "rgb(255 0 0)",
                "rgb(255 0 0 / 0.5)",
                "rgb(100% 0 50% / 50%)",
                "rgba(none none none / none)",
                "rgb(255 0 0/.5)",
                "rgba(0 0 0)",
                "rgb(1 2 3",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "rgb()",
                "rgb(255 0)",
                "rgb(255 0 0 0)",
                "rgb(255 0 0 /)",
                "rgb(255 0 0 / 1 / 1)",
                "rgb(255 0 / 0 0)",
                "rgb(/ 0.5)",
                "rgb(1px 0 0)",
                "rgb(r g b)",
                "rgb(255 0 0 / 0.5 0.5)",
            ],
        );
    }

    // Claim: "Within a top-level list in a property or other CSS value, or a
    // function’s argument list, a comma specified in the grammar must be
    // omitted if: all items preceding the comma have been omitted; all items
    // following the comma have been omitted; multiple commas would be
    // adjacent (ignoring white space/comments), due to the items between the
    // commas being omitted."
    // https://drafts.csswg.org/css-values-4/#comb-comma
    #[test]
    fn omitted_alpha_omits_its_comma() {
        assert_valid(check_color, &["rgb(1, 2, 3)", "hsl(1, 2%, 3%)"]);
        assert_invalid(
            check_color,
            &[
                "rgb(255, 0, 0,)",
                "rgba(255, 0, 0, )",
                "rgb(255,,0,0)",
                "rgb(,255,0,0)",
                "hsl(120, 100%, 50%,)",
            ],
        );
    }

    // Claim: "<alpha-value> = <number> | <percentage>"
    // https://drafts.csswg.org/css-color-4/#typedef-color-alpha-value
    #[test]
    fn alpha_value() {
        assert_valid(
            check_color,
            &[
                "rgb(0 0 0 / 0.5)",
                "rgb(0 0 0 / 50%)",
                "rgb(0 0 0 / 2)",
                "rgba(0, 0, 0, -1)",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "rgb(0 0 0 / 1deg)",
                "rgba(0, 0, 0, 1px)",
                "rgb(0 0 0 / red)",
            ],
        );
    }

    // Claim: "<hue> = <number> | <angle>"
    // https://drafts.csswg.org/css-color-4/#typedef-hue
    #[test]
    fn hue() {
        assert_valid(
            check_color,
            &[
                "hsl(120deg 50% 50%)",
                "hsl(120 50% 50%)",
                "hsl(0.5turn 50% 50%)",
                "hsl(2rad, 50%, 50%)",
                "hsl(100grad 50% 50%)",
                "lch(50% 30 1RAD)",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "hsl(120px 50% 50%)",
                "hsl(50% 50% 50%)",
                "hsl(1s, 50%, 50%)",
                "oklch(0.5 0.1 10%)",
            ],
        );
    }

    // Claim: "hsl() = [ <legacy-hsl-syntax> | <modern-hsl-syntax> ]
    // hsla() = [ <legacy-hsla-syntax> | <modern-hsla-syntax> ]
    // <modern-hsl-syntax> = hsl(
    //     [<hue> | none]
    //     [<percentage> | <number> | none]
    //     [<percentage> | <number> | none]
    //     [ / [<alpha-value> | none] ]? )
    // <legacy-hsl-syntax> = hsl( <hue>, <percentage>, <percentage>, <alpha-value>? )
    // <legacy-hsla-syntax> = hsla( <hue>, <percentage>, <percentage>, <alpha-value>? )"
    // https://drafts.csswg.org/css-color-4/#the-hsl-notation
    #[test]
    fn hsl_syntax() {
        assert_valid(
            check_color,
            &[
                "hsl(120, 100%, 50%)",
                "hsla(120, 100%, 50%, 0.3)",
                "hsl(120, 100%, 50%, 30%)",
                "hsl(120 100% 50%)",
                "hsl(120 100 50)",
                "hsl(none none none / none)",
                "HSLA(120deg 100% 50% / 30%)",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "hsl(120, 100, 50)",
                "hsl(120 100%)",
                "hsl(none, 100%, 50%)",
                "hsl(120, none, 50%)",
                "hsl(120 100% 50% 1)",
                "hsl(120, 100%, 50%, 1, 1)",
            ],
        );
    }

    // Claim: "hwb() = hwb(
    //   [<hue> | none]
    //   [<percentage> | <number> | none]
    //   [<percentage> | <number> | none]
    //   [ / [<alpha-value> | none] ]? )"
    // https://drafts.csswg.org/css-color-4/#the-hwb-notation
    #[test]
    fn hwb_syntax() {
        assert_valid(
            check_color,
            &[
                "hwb(120 0% 0%)",
                "hwb(120deg 10 20 / 0.5)",
                "hwb(none none none)",
                "HWB(0 0% 0% / 50%)",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "hwb(120, 0%, 0%)",
                "hwb(120 0%)",
                "hwb(120% 0% 0%)",
                "hwb(120 0deg 0%)",
            ],
        );
    }

    // Claim: "lab() = lab( [<percentage> | <number> | none]
    //       [ <percentage> | <number> | none]
    //       [ <percentage> | <number> | none]
    //       [ / [<alpha-value> | none] ]? )" and
    // "lch() = lch( [<percentage> | <number> | none]
    //       [ <percentage> | <number> | none]
    //       [ <hue> | none]
    //       [ / [<alpha-value> | none] ]? )"
    // https://drafts.csswg.org/css-color-4/#specifying-lab-lch
    #[test]
    fn lab_and_lch_syntax() {
        assert_valid(
            check_color,
            &[
                "lab(50% 40 59.5)",
                "lab(50 -20 30 / 0.5)",
                "lab(none none none)",
                "lch(52.2% 72.2 50)",
                "lch(50% 30 120deg / 50%)",
                "LCH(50% 30% none)",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "lab(50%, 40, 59.5)",
                "lab(50% 40)",
                "lab(50deg 40 59)",
                "lab(50% 40 59 / 1 2)",
                "lch(50% 30deg 120)",
                "lch(50% 30 120%)",
            ],
        );
    }

    // Claim: "oklab() = oklab( [ <percentage> | <number> | none]
    //     [ <percentage> | <number> | none]
    //     [ <percentage> | <number> | none]
    //     [ / [<alpha-value> | none] ]? )" and
    // "oklch() = oklch( [ <percentage> | <number> | none]
    //       [ <percentage> | <number> | none]
    //       [ <hue> | none]
    //       [ / [<alpha-value> | none] ]? )"
    // https://drafts.csswg.org/css-color-4/#specifying-oklab-oklch
    #[test]
    fn oklab_and_oklch_syntax() {
        assert_valid(
            check_color,
            &[
                "oklab(0.5 0.1 -0.1)",
                "oklab(50% 10% none / 0.5)",
                "oklch(0.7 0.15 180)",
                "oklch(70% 0.15 180deg / 50%)",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "oklab(0.5, 0.1, 0.1)",
                "oklab(0.5 0.1)",
                "oklch(0.7 0.15 180px)",
                "oklch(0.7 0.15)",
            ],
        );
    }

    // Claim: "color() = color( <colorspace-params> [ / [ <alpha-value> | none ] ]? )
    // <colorspace-params> = [ <predefined-rgb-params> | <xyz-params>]
    // <predefined-rgb-params> = <predefined-rgb> [ <number> | <percentage> | none ]{3}
    // <predefined-rgb> = srgb | srgb-linear | display-p3 | display-p3-linear | a98-rgb | prophoto-rgb | rec2020
    // <xyz-params> = <xyz-space> [ <number> | <percentage> | none ]{3}
    // <xyz-space> = xyz | xyz-d50 | xyz-d65"
    // https://drafts.csswg.org/css-color-4/#color-function
    #[test]
    fn color_function_syntax() {
        assert_valid(
            check_color,
            &[
                "color(srgb 1 0 0)",
                "color(display-p3 1 0.5 0 / 0.5)",
                "color(xyz-d65 0.2 0.3 0.4)",
                "color(xyz 0.2 0.3 0.4 / none)",
                "color(rec2020 50% 50% none)",
                "color(display-p3-linear 0 0 0)",
                "color(a98-rgb 0 0 0)",
                "color(prophoto-rgb 0 0 0)",
                "color(srgb-linear 0 0 0)",
                "color(xyz-d50 0 0 0)",
                "COLOR(SRGB 1 0 0)",
            ],
        );
        assert_invalid(
            check_color,
            &[
                "color()",
                "color(srgb 1 0)",
                "color(srgb 1 0 0 0)",
                "color(1 0 0)",
                "color(srgb, 1, 0, 0)",
                "color(xyz 1 0 0 /)",
                "color(srgb 1deg 0 0)",
            ],
        );
    }

    // Claim: "color() = color( [from <color>]? <colorspace-params> [ / [
    // <alpha-value> | none ] ]? )" and "<custom-params> = <dashed-ident> [
    // <number> | <percentage> | none ]+"
    // https://drafts.csswg.org/css-color-5/#color-function
    // Claim: "The new relative color syntax extends modern color syntax to
    // allow existing colors to be modified using the color functions: if an
    // origin color is specified, then each color component (and the alpha
    // component, if specified) can either be directly specified, or taken
    // from the origin color (and possibly modified with math functions)."
    // https://drafts.csswg.org/css-color-5/#relative-colors
    // Claim: "<color> = <color-base> | currentColor | <system-color> |
    //       <contrast-color()> | <device-cmyk()>  | <light-dark-color>"
    // https://drafts.csswg.org/css-color-5/#typedef-color
    // (accept-only: these extensions are accepted without being modelled)
    #[test]
    fn color_5_extensions_are_accepted() {
        assert_valid(
            check_color,
            &[
                "color(--custom 1 2 3 4)",
                "color(from red srgb r g b)",
                "color(rec2100-pq 0 0 0)",
                "rgb(from red r g b)",
                "rgb(from #fff r g b / 0.5)",
                "hsl(from red h s l / 0.5)",
                "hwb(from red h w b)",
                "lab(from red l a b)",
                "oklch(from red l c h)",
                "color-mix(in srgb, red, blue)",
                "light-dark(red, blue)",
                "contrast-color(red)",
                "device-cmyk(0 0 0 1)",
                "alpha(from red / 0.5)",
            ],
        );
    }

    // Claim: "A math function represents a numeric value, one of: <length>,
    // <frequency>, <angle>, <time>, <flex>, <resolution>, <percentage>,
    // <number>, <integer> ...or the <length-percentage>/etc mixed types, and
    // can be used wherever such a value would be valid."
    // https://drafts.csswg.org/css-values-4/#math
    #[test]
    fn math_functions_stand_for_one_component() {
        assert_valid(
            check_color,
            &[
                "rgb(calc(255) 0 0)",
                "hsl(calc(120deg + 10deg) 50% 50%)",
                "rgb(clamp(0, 300, 255), 0, 0)",
                "rgb(calc(100%), 0%, 0%)",
                "rgb(0 0 0 / calc(0.5))",
                "lch(50% 30 sin(1))",
            ],
        );
        assert_invalid(check_color, &["rgb(calc(255) 0)", "rgb(calc(1) 0 0 0)"]);
    }

    // Claim: "If a property value contains one or more arbitrary substitution
    // functions, and all of those functions are themselves syntactically
    // valid according to their argument grammars, the entire value’s grammar
    // must be assumed to be valid at parse time."
    // https://drafts.csswg.org/css-values-5/#resolve-property
    // (accept-only)
    #[test]
    fn arbitrary_substitution_is_accepted() {
        assert_valid(
            check_color,
            &[
                "var(--c)",
                "rgb(var(--r) 0 0)",
                "rgb(var(--rgb))",
                "var(--a) var(--b)",
                "env(--x)",
                "attr(data-c type(<color>))",
                "if(media(print): red; else: blue)",
            ],
        );
        assert_valid(check_paint, &["var(--paint)", "url(#a) var(--fallback)"]);
    }

    // Claim: "All CSS properties also accept the CSS-wide keyword values as
    // the sole component of their property value."
    // https://drafts.csswg.org/css-values-4/#component-types
    #[test]
    fn css_wide_keywords_alone() {
        let keywords = [
            "inherit",
            "initial",
            "unset",
            "revert",
            "revert-layer",
            "INHERIT",
            " inherit ",
        ];
        assert_valid(check_color, &keywords);
        assert_valid(check_paint, &keywords);
        assert_invalid(check_color, &["inherit red", "red inherit"]);
        assert_invalid(check_paint, &["url(#a) inherit", "inherit none"]);
    }

    // Claim: "Since presentation attributes are parsed as CSS values, not
    // declarations, an !important declaration within a presentation attribute
    // will cause it to have an invalid value."
    // https://svgwg.org/svg2-draft/styling.html#PresentationAttributes
    #[test]
    fn important_is_invalid_in_presentation_attributes() {
        assert_invalid(check_color, &["red !important", "red;", "red; "]);
        assert_invalid(check_paint, &["none !important", "#fff!important"]);
    }

    // Claim: "icccolor ::= "icc-color(" name (comma-wsp number)+ ")""
    // https://www.w3.org/TR/SVG11/types.html#DataTypeICCColor
    // Claim: "<paint>: none | currentColor | <color> [<icccolor>] |
    // <funciri> [ none | currentColor | <color> [<icccolor>] ] | inherit"
    // https://www.w3.org/TR/SVG11/painting.html#SpecifyingPaint
    // (an SVG 1.1 document may carry an ICC color after a color, never after
    // none)
    #[test]
    fn svg11_icc_color_after_a_color() {
        assert_valid(
            check_color,
            &["#CD853F icc-color(acmecmyk, 0.11, 0.48, 0.83, 0.00)"],
        );
        assert_valid(
            check_paint,
            &[
                "#CD853F icc-color(acmecmyk, 0.11, 0.48, 0.83, 0.00)",
                "url(#a) red icc-color(p, 1)",
            ],
        );
        assert_invalid(
            check_paint,
            &["none icc-color(p, 1)", "url(#a) none icc-color(p, 1)"],
        );
    }

    // Claim: "<paint> = none | <color> | <url> [none | <color>]? |
    // context-fill | context-stroke"
    // https://svgwg.org/svg2-draft/painting.html#DataTypePaint
    #[test]
    fn paint_grammar() {
        assert_valid(
            check_paint,
            &[
                "none",
                "NONE",
                "red",
                "#fff",
                "currentColor",
                "context-fill",
                "Context-Stroke",
                "rgb(0 0 0 / 0.5)",
                "url(#grad)",
                "url('#grad')",
                "url(\"#g\") red",
                "url(#a) none",
                "url(#a) currentColor",
                "url(#a)red",
                "URL(#a)",
                "  url(#a)  ",
                "color-mix(in srgb, red, blue)",
            ],
        );
        assert_invalid(
            check_paint,
            &[
                "",
                "  ",
                "foo",
                "none red",
                "red none",
                "red url(#a)",
                "url(#a) url(#b)",
                "url(#a) context-fill",
                "url(#a) red blue",
                "context-fill red",
                "context-fill context-stroke",
                "#ff",
                "url(#a) foo",
                "url(#a),red",
                "url(#a) #ff",
                "freeze",
            ],
        );
    }

    // Claim: "<url> = <url()> | <src()>" with "<url()> = url( <string>
    // <url-modifier>* ) | <url-token>" and "<src()> = src( <string>
    // <url-modifier>* )"
    // https://drafts.csswg.org/css-values-4/#url-value
    #[test]
    fn paint_server_url_forms() {
        assert_valid(
            check_paint,
            &[
                "url(#a)",
                "url( #a )",
                "url(\"#a\")",
                "src(\"#a\")",
                "src(\"#a\") blue",
                "url(paint.svg#p) white",
            ],
        );
        assert_invalid(
            check_paint,
            &["url(#a) src(\"#b\")", "src(\"#a\") src(\"#b\")"],
        );
    }

    // Real values for every attribute dispatched to `Color`: `color`,
    // `flood-color`, `lighting-color`, `stop-color` and
    // `text-decoration-color`, taken from SVG 2 examples where they exist.
    // Claim: "<color> = <color-base> | currentColor | <system-color>"
    // https://drafts.csswg.org/css-color-4/#color-syntax
    #[test]
    fn color_attribute_values() {
        // stop-color
        assert_valid(
            check_color,
            &["purple", "lightblue", "#A8F", "#FDC", "currentColor"],
        );
        // flood-color, lighting-color
        assert_valid(check_color, &["#ffffff", "green", "#bbbbbb", "white"]);
        // color, text-decoration-color
        assert_valid(check_color, &["red", "rgb(255 0 0)", "hsl(0 100% 50%)"]);
        assert_invalid(check_color, &["#bbbbb", "url(#a)", "none"]);
    }

    // Real values for every attribute dispatched to `Paint`: `fill` and
    // `stroke`.
    // Claim: "<paint> = none | <color> | <url> [none | <color>]? |
    // context-fill | context-stroke"
    // https://svgwg.org/svg2-draft/painting.html#DataTypePaint
    #[test]
    fn paint_attribute_values() {
        // fill
        assert_valid(
            check_paint,
            &[
                "none",
                "white",
                "url(#MyHatch) powderblue",
                "url(#MyGradient)",
            ],
        );
        // stroke
        assert_valid(check_paint, &["blue", "deeppink", "#ccc", "context-stroke"]);
        assert_invalid(check_paint, &["bleu", "#cc", "url(#a) red green"]);
    }
}
