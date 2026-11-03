//! Keyword-enumerated value spaces.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

use svg_data::{AttributeDef, AttributeValues};

use super::syntax::{
    cascade::is_css_wide_keyword_value,
    microsyntax::{
        eq_ascii_case_insensitive, find_ascii_case_insensitive, is_xml_whitespace,
        split_on_ascii_whitespace, strip_ascii_whitespace,
    },
    notation::contains_arbitrary_substitution,
    parser::{ComponentValue, ComponentValueList, parse_component_values},
    tokenizer::Token,
    units::{UnitCategory, unit_category},
};

/// `Enum`: one of a fixed keyword set.
///
/// The keyword list alone does not say how the value is parsed, so the check
/// finds the catalog attributes that use this list and applies each one's
/// syntax; the value is invalid only when it is invalid for all of them:
///
/// - a CSS property (presentation attribute): the property's own grammar,
///   which can be wider than the catalog list, parsed as CSS with CSS-wide
///   keywords, comments, escapes and ASCII case-insensitive keywords;
/// - another presentation attribute this module does not model: accepted;
/// - an SVG attribute defined with the CSS Value Definition Syntax: exactly
///   one keyword, parsed as CSS (case-insensitive, whitespace and comments
///   allowed), but no CSS-wide keywords;
/// - `attributeType` (SVG 1.1, "values are case-sensitive") and `xml:space`
///   ("without white space"): the exact strings;
/// - an ARIA token or token list: case-insensitive tokens, or the empty
///   string;
/// - an HTML enumerated attribute: a case-insensitive keyword with no
///   surrounding whitespace, or the empty string where it has a state.
///
/// A list no catalog attribute uses is accepted.
pub fn check_enum(value: &str, values: &[&str]) -> Option<String> {
    // Stop at the first syntax that accepts the value; otherwise keep the
    // first reason.
    syntaxes(values)
        .into_iter()
        .try_fold(None, |first: Option<String>, syntax| {
            let reason = syntax.check(value, values)?;
            Some(first.or(Some(reason)))
        })
        .flatten()
}

/// How a keyword-valued attribute is parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Syntax {
    /// A CSS property value with the given grammar.
    Property(&'static Rule),
    /// A presentation attribute whose grammar is not modelled here.
    Unmodelled,
    /// An SVG attribute in CSS Value Definition Syntax.
    Svg,
    /// The keyword exactly, with surrounding XML whitespace allowed.
    CaseSensitive,
    /// The keyword exactly.
    Exact,
    /// A WAI-ARIA token, or a token list.
    Aria {
        /// Whether the value is a token list.
        list: bool,
    },
    /// An HTML enumerated attribute.
    Html {
        /// Whether the empty string is conforming (an empty value default).
        empty: bool,
    },
}

impl Syntax {
    /// The syntax of `def`, for its element-scoped value space when `scoped`.
    fn of(def: &AttributeDef, scoped: bool) -> Self {
        let name = def.name;
        if name == "xml:space" {
            return Self::Exact;
        }
        if let Some(state) = name.strip_prefix("aria-") {
            return Self::Aria {
                list: ARIA_TOKEN_LISTS.contains(&state),
            };
        }
        if !scoped {
            if let Some(rule) = property_rule(name) {
                return Self::Property(rule);
            }
            if def.presentation_attribute.is_some() {
                return Self::Unmodelled;
            }
        }
        match name {
            "crossorigin" => Self::Html { empty: true },
            "decoding" | "fetchpriority" => Self::Html { empty: false },
            "attributeType" => Self::CaseSensitive,
            _ => Self::Svg,
        }
    }

    fn check(self, value: &str, keywords: &[&str]) -> Option<String> {
        match self {
            Self::Property(rule) => rule.check(value),
            Self::Unmodelled => None,
            Self::Svg => check_svg_keyword(value, keywords),
            Self::CaseSensitive => {
                let trimmed = value.trim_matches(is_xml_whitespace);
                (!keywords.contains(&trimmed)).then(|| expected_exactly(keywords))
            }
            Self::Exact => (!keywords.contains(&value)).then(|| expected_exactly(keywords)),
            Self::Aria { list } => check_aria(value, keywords, list),
            Self::Html { empty } => check_html(value, keywords, empty),
        }
    }
}

/// The ARIA states and properties whose value is a token list
/// (<https://www.w3.org/TR/wai-aria-1.2/#aria-relevant>,
/// <https://www.w3.org/TR/wai-aria-1.2/#aria-dropeffect>).
const ARIA_TOKEN_LISTS: &[&str] = &["relevant", "dropeffect"];

/// The syntaxes of the catalog attributes whose value space is `keywords`.
fn syntaxes(keywords: &[&str]) -> Vec<Syntax> {
    let is_list = |values: &AttributeValues| matches!(values, AttributeValues::Enum(list) if list[..] == keywords[..]);
    let mut found = Vec::new();
    let mut add = |syntax: Syntax| {
        if !found.contains(&syntax) {
            found.push(syntax);
        }
    };
    for def in svg_data::attributes() {
        let unscoped = std::iter::once(&def.values)
            .chain(def.value_overrides.iter().map(|(_, values)| values))
            .any(is_list);
        if unscoped {
            add(Syntax::of(def, false));
        }
        if def
            .element_values
            .iter()
            .any(|scoped| is_list(&scoped.values))
        {
            add(Syntax::of(def, true));
        }
    }
    found
}

/// A CSS property value grammar, as far as this module checks it.
#[derive(Debug, PartialEq, Eq)]
enum Rule {
    /// Exactly one of the keywords.
    One(&'static [&'static str]),
    /// One to `max` of the keywords, in any combination.
    Several {
        /// The keywords the grammar uses.
        keywords: &'static [&'static str],
        /// The most components a value can have.
        max: usize,
    },
    /// `font-weight`.
    FontWeight,
    /// `font-stretch`.
    FontStretch,
    /// `font-style`.
    FontStyle,
    /// `glyph-orientation-vertical`.
    GlyphOrientationVertical,
}

/// The property grammars, by presentation attribute name. Each list is the
/// union of the SVG 2 and CSS definitions and the SVG 1.1 values still
/// accepted for presentation attributes.
static PROPERTIES: &[(&str, Rule)] = &[
    (
        "border-top-style",
        Rule::One(&[
            "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset",
            "outset",
        ]),
    ),
    ("clip-rule", Rule::One(&["nonzero", "evenodd"])),
    ("fill-rule", Rule::One(&["nonzero", "evenodd"])),
    (
        "color-interpolation",
        Rule::One(&["auto", "sRGB", "linearRGB"]),
    ),
    (
        "color-interpolation-filters",
        Rule::One(&["auto", "sRGB", "linearRGB"]),
    ),
    ("direction", Rule::One(&["ltr", "rtl"])),
    (
        "display",
        Rule::Several {
            keywords: DISPLAY,
            max: 3,
        },
    ),
    ("font-kerning", Rule::One(&["auto", "normal", "none"])),
    ("font-stretch", Rule::FontStretch),
    ("font-style", Rule::FontStyle),
    ("font-weight", Rule::FontWeight),
    ("glyph-orientation-vertical", Rule::GlyphOrientationVertical),
    ("hyphens", Rule::One(&["none", "manual", "auto"])),
    (
        "image-rendering",
        Rule::One(&[
            "auto",
            "smooth",
            "high-quality",
            "pixelated",
            "crisp-edges",
            "optimizeSpeed",
            "optimizeQuality",
        ]),
    ),
    ("isolation", Rule::One(&["auto", "isolate"])),
    (
        "line-break",
        Rule::One(&["auto", "loose", "normal", "strict", "anywhere"]),
    ),
    ("mask-type", Rule::One(&["luminance", "alpha"])),
    (
        "overflow",
        Rule::Several {
            keywords: &["visible", "hidden", "clip", "scroll", "auto", "overlay"],
            max: 2,
        },
    ),
    (
        "overflow-wrap",
        Rule::One(&["normal", "break-word", "anywhere"]),
    ),
    (
        "word-wrap",
        Rule::One(&["normal", "break-word", "anywhere"]),
    ),
    (
        "pointer-events",
        Rule::One(&[
            "auto",
            "bounding-box",
            "visiblePainted",
            "visibleFill",
            "visibleStroke",
            "visible",
            "painted",
            "fill",
            "stroke",
            "all",
            "none",
        ]),
    ),
    (
        "shape-rendering",
        Rule::One(&["auto", "optimizeSpeed", "crispEdges", "geometricPrecision"]),
    ),
    ("stroke-linecap", Rule::One(&["butt", "round", "square"])),
    (
        "stroke-linejoin",
        Rule::Several {
            keywords: &[
                "miter",
                "miter-clip",
                "round",
                "bevel",
                "arcs",
                "crop",
                "fallback",
            ],
            max: 2,
        },
    ),
    (
        "text-align-last",
        Rule::One(&[
            "auto",
            "start",
            "end",
            "left",
            "right",
            "center",
            "justify",
            "match-parent",
        ]),
    ),
    ("text-anchor", Rule::One(&["start", "middle", "end"])),
    (
        "text-decoration-style",
        Rule::One(&["solid", "double", "dotted", "dashed", "wavy"]),
    ),
    (
        "text-orientation",
        Rule::One(&["mixed", "upright", "sideways", "sideways-right"]),
    ),
    (
        "text-rendering",
        Rule::One(&[
            "auto",
            "optimizeSpeed",
            "optimizeLegibility",
            "geometricPrecision",
        ]),
    ),
    (
        "transform-box",
        Rule::One(&[
            "content-box",
            "border-box",
            "fill-box",
            "stroke-box",
            "view-box",
        ]),
    ),
    (
        "unicode-bidi",
        Rule::One(&[
            "normal",
            "embed",
            "isolate",
            "bidi-override",
            "isolate-override",
            "plaintext",
        ]),
    ),
    (
        "vector-effect",
        Rule::One(&[
            "none",
            "non-scaling-stroke",
            "non-scaling-size",
            "non-rotation",
            "fixed-position",
        ]),
    ),
    (
        "visibility",
        Rule::One(&["visible", "hidden", "force-hidden", "collapse"]),
    ),
    (
        "word-break",
        Rule::One(&[
            "normal",
            "break-all",
            "keep-all",
            "manual",
            "auto-phrase",
            "break-word",
        ]),
    ),
    (
        "writing-mode",
        Rule::One(&[
            "horizontal-tb",
            "vertical-rl",
            "vertical-lr",
            "sideways-rl",
            "sideways-lr",
            "lr",
            "lr-tb",
            "rl",
            "rl-tb",
            "tb",
            "tb-rl",
        ]),
    ),
];

/// Every keyword of the `display` grammar (CSS Display 4 with the CSS Grid 3
/// and `MathML` Core extensions) and the CSS 2 values SVG 1.1 lists.
const DISPLAY: &[&str] = &[
    "block",
    "inline",
    "run-in",
    "flow",
    "flow-root",
    "table",
    "flex",
    "grid",
    "ruby",
    "list-item",
    "table-row-group",
    "table-header-group",
    "table-footer-group",
    "table-row",
    "table-cell",
    "table-column-group",
    "table-column",
    "table-caption",
    "ruby-base",
    "ruby-text",
    "ruby-base-container",
    "ruby-text-container",
    "contents",
    "none",
    "inline-block",
    "inline-table",
    "inline-flex",
    "inline-grid",
    "grid-lanes",
    "inline-grid-lanes",
    "math",
    "compact",
    "marker",
];

const FONT_WEIGHT_KEYWORDS: &[&str] = &["normal", "bold", "bolder", "lighter"];

const FONT_STRETCH_KEYWORDS: &[&str] = &[
    "normal",
    "ultra-condensed",
    "extra-condensed",
    "condensed",
    "semi-condensed",
    "semi-expanded",
    "expanded",
    "extra-expanded",
    "ultra-expanded",
    "narrower",
    "wider",
];

const FONT_STYLE_KEYWORDS: &[&str] = &["normal", "italic", "oblique", "left", "right"];

fn property_rule(name: &str) -> Option<&'static Rule> {
    PROPERTIES
        .iter()
        .find_map(|(property, rule)| (*property == name).then_some(rule))
}

impl Rule {
    /// Check `value` as a presentation attribute with this grammar.
    fn check(&self, value: &str) -> Option<String> {
        if is_css_wide_keyword_value(value) {
            return None;
        }
        let list = parse_component_values(value);
        if is_undecidable(&list) {
            return None;
        }
        let parts: Vec<&ComponentValue<'_>> = list.significant().collect();
        let valid = match (self, parts.as_slice()) {
            (Self::One(keywords), [only]) => is_keyword(only, keywords),
            (Self::Several { keywords, max }, parts) => {
                (1..=*max).contains(&parts.len())
                    && parts.iter().all(|part| is_keyword(part, keywords))
            }
            (Self::FontWeight, [only]) => {
                is_keyword(only, FONT_WEIGHT_KEYWORDS)
                    || number(only).is_some_and(|weight| (1.0..=1000.0).contains(&weight))
            }
            (Self::FontStretch, [only]) => {
                is_keyword(only, FONT_STRETCH_KEYWORDS)
                    || percentage(only).is_some_and(|width| width >= 0.0)
            }
            (Self::FontStyle, [only]) => is_keyword(only, FONT_STYLE_KEYWORDS),
            (Self::FontStyle, [oblique, angle]) => {
                is_keyword(oblique, &["oblique"]) && is_angle(angle)
            }
            (Self::GlyphOrientationVertical, [only]) => {
                is_keyword(only, &["auto"]) || is_angle(only)
            }
            _ => false,
        };
        (!valid).then(|| self.expected())
    }

    fn expected(&self) -> String {
        match self {
            Self::One(keywords) => expected_one_of(keywords),
            Self::Several { keywords, max } => {
                format!("expected one to {max} of {}", keywords.join(", "))
            }
            Self::FontWeight => {
                "expected normal, bold, bolder, lighter or a number from 1 to 1000".to_owned()
            }
            Self::FontStretch => format!(
                "expected a non-negative percentage or one of {}",
                FONT_STRETCH_KEYWORDS.join(", ")
            ),
            Self::FontStyle => {
                "expected normal, italic, left, right, or oblique with an optional angle".to_owned()
            }
            Self::GlyphOrientationVertical => "expected auto or an angle".to_owned(),
        }
    }
}

/// Whether the syntax layer cannot decide `list`: a function (a math
/// function or one this module does not model), an arbitrary substitution
/// anywhere, or a nesting limit.
fn is_undecidable(list: &ComponentValueList<'_>) -> bool {
    list.depth_limited
        || list
            .values
            .iter()
            .any(|value| matches!(value, ComponentValue::Function(_)))
        || contains_arbitrary_substitution(&list.values)
}

/// Whether `value` is an identifier matching one of `keywords` ASCII
/// case-insensitively.
fn is_keyword(value: &ComponentValue<'_>, keywords: &[&str]) -> bool {
    value
        .ident()
        .is_some_and(|ident| find_ascii_case_insensitive(ident, keywords).is_some())
}

const fn number(value: &ComponentValue<'_>) -> Option<f64> {
    match value {
        ComponentValue::Token(Token::Number(number)) => Some(number.value),
        _ => None,
    }
}

const fn percentage(value: &ComponentValue<'_>) -> Option<f64> {
    match value {
        ComponentValue::Token(Token::Percentage(number)) => Some(number.value),
        _ => None,
    }
}

/// An `<angle>`, or the `<number>` a presentation attribute accepts for it.
fn is_angle(value: &ComponentValue<'_>) -> bool {
    match value {
        ComponentValue::Token(Token::Number(_)) => true,
        ComponentValue::Token(Token::Dimension { unit, .. }) => {
            unit_category(unit) == Some(UnitCategory::Angle)
        }
        _ => false,
    }
}

/// An SVG attribute defined in CSS Value Definition Syntax as a choice of
/// keywords: one keyword, parsed as CSS.
fn check_svg_keyword(value: &str, keywords: &[&str]) -> Option<String> {
    let list = parse_component_values(value);
    if is_undecidable(&list) {
        return None;
    }
    let mut parts = list.significant();
    match (parts.next(), parts.next()) {
        (Some(only), None) if is_keyword(only, keywords) => None,
        _ => Some(expected_one_of(keywords)),
    }
}

/// A WAI-ARIA token (or token list): case-insensitive, surrounding
/// whitespace and the empty string accepted.
fn check_aria(value: &str, keywords: &[&str], list: bool) -> Option<String> {
    let trimmed = strip_ascii_whitespace(value);
    if trimmed.is_empty() {
        return None;
    }
    let valid = if list {
        split_on_ascii_whitespace(trimmed).all(|token| {
            keywords
                .iter()
                .flat_map(|keyword| split_on_ascii_whitespace(keyword))
                .any(|known| eq_ascii_case_insensitive(known, token))
        })
    } else {
        find_ascii_case_insensitive(trimmed, keywords).is_some()
    };
    (!valid).then(|| {
        if list {
            format!("expected a list of {}", keywords.join(", "))
        } else {
            expected_one_of(keywords)
        }
    })
}

/// An HTML enumerated attribute: "An ASCII case-insensitive match for one of
/// the conforming keywords for that attribute, with no leading or trailing
/// whitespace", or the empty string when it has an empty value default.
fn check_html(value: &str, keywords: &[&str], empty: bool) -> Option<String> {
    if (empty && value.is_empty()) || find_ascii_case_insensitive(value, keywords).is_some() {
        return None;
    }
    let trimmed = strip_ascii_whitespace(value);
    if trimmed != value && find_ascii_case_insensitive(trimmed, keywords).is_some() {
        return Some("leading or trailing whitespace is not allowed".to_owned());
    }
    Some(expected_one_of(keywords))
}

fn expected_one_of(keywords: &[&str]) -> String {
    format!("expected one of {}", keywords.join(", "))
}

fn expected_exactly(keywords: &[&str]) -> String {
    format!(
        "expected exactly one of {} (case-sensitive)",
        keywords.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use svg_data::{AttributeValues, SpecSnapshotId};

    /// The catalog keyword list of `attr` on `element` (its element-scoped
    /// value space when there is one) or, with `profile`, in that snapshot.
    fn keywords(
        attr: &str,
        element: Option<&str>,
        profile: Option<SpecSnapshotId>,
    ) -> &'static [&'static str] {
        let Some(def) = svg_data::attribute(attr) else {
            panic!("{attr} is not in the catalog");
        };
        let values = match (element, profile) {
            (Some(element), _) => def.values_for_element(Some(element)),
            (None, Some(profile)) => def.values_for_profile(profile),
            (None, None) => &def.values,
        };
        let AttributeValues::Enum(keywords) = values else {
            panic!("{attr} on {element:?} in {profile:?} is not an Enum: {values:?}");
        };
        keywords
    }

    fn assert_usage(
        attr: &str,
        element: Option<&str>,
        profile: Option<SpecSnapshotId>,
        valid: &[&str],
        invalid: &[&str],
    ) {
        let list = keywords(attr, element, profile);
        for value in valid {
            assert_eq!(
                check_enum(value, list),
                None,
                "{attr}={value:?} on {element:?} ({profile:?}) must be valid"
            );
        }
        for value in invalid {
            assert!(
                check_enum(value, list).is_some(),
                "{attr}={value:?} on {element:?} ({profile:?}) must be invalid"
            );
        }
    }

    fn assert_attr(attr: &str, valid: &[&str], invalid: &[&str]) {
        assert_usage(attr, None, None, valid, invalid);
    }

    fn assert_on(attr: &str, element: &str, valid: &[&str], invalid: &[&str]) {
        assert_usage(attr, Some(element), None, valid, invalid);
    }

    const SVG11: Option<SpecSnapshotId> = Some(SpecSnapshotId::Svg11Rec20110816);

    // Claim: "Keywords are identifiers and are interpreted ASCII
    // case-insensitively (i.e., [a-z] and [A-Z] are equivalent)."
    // https://drafts.csswg.org/css-values-4/#keywords
    #[test]
    fn property_keywords_match_ascii_case_insensitively() {
        assert_attr("fill-rule", &["EVENODD", "NonZero"], &["even-odd"]);
        assert_attr("clip-rule", &["evenOdd"], &["nonzero2"]);
        assert_attr("color-interpolation", &["SRGB", "linearrgb"], &["rgb"]);
        assert_attr("shape-rendering", &["crispedges"], &["crisp"]);
        assert_attr("pointer-events", &["VISIBLEPAINTED"], &["painted-visible"]);
        assert_attr("text-anchor", &["Middle"], &["center"]);
    }

    // Claim: "Let grammar be the grammar given in the attribute definition
    // table's Value column." / "Return the result of parsing value with
    // grammar."
    // https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value
    // Claim: "Parse a list of component values from input, and let result be
    // the return value. Attempt to match result against grammar."
    // https://drafts.csswg.org/css-syntax-3/#css-parse-something-according-to-a-css-grammar
    #[test]
    fn presentation_attributes_parse_as_css_values() {
        assert_attr(
            "fill-rule",
            &[
                " evenodd ",
                "\tnonzero\n",
                "evenodd/**/",
                "/* rule */nonzero",
                "\\65venodd",
            ],
            &[
                "",
                "   ",
                "evenodd nonzero",
                "evenodd,",
                "'evenodd'",
                "[evenodd]",
                "#evenodd",
                "evenodd;",
            ],
        );
        assert_attr("stroke-linecap", &["round"], &["round round", "5"]);
    }

    // Claim: "Since presentation attributes are parsed as CSS values, not
    // declarations, an !important declaration within a presentation attribute
    // will cause it to have an invalid value."
    // https://svgwg.org/svg2-draft/styling.html#PresentationAttributes
    #[test]
    fn important_in_presentation_attribute_is_invalid() {
        assert_attr(
            "fill-rule",
            &[],
            &["evenodd !important", "evenodd!important"],
        );
        assert_attr("visibility", &[], &["hidden !important"]);
        assert_attr("display", &[], &["none !important"]);
    }

    // Claim: "all properties accept the CSS-wide keywords, which represent
    // value computations common to all CSS properties."
    // https://drafts.csswg.org/css-values-4/#common-keywords
    #[test]
    fn presentation_attributes_accept_css_wide_keywords() {
        let wide = [
            "inherit",
            "INHERIT",
            "initial",
            "unset",
            "revert",
            "revert-layer",
            " inherit ",
        ];
        assert_attr("fill-rule", &wide, &["inherited"]);
        assert_attr("display", &wide, &[]);
        assert_attr("stroke-linejoin", &wide, &[]);
        assert_attr("font-weight", &wide, &[]);
        assert_usage("visibility", None, SVG11, &wide, &["shown"]);
    }

    // Claim: "If a property value contains one or more arbitrary substitution
    // functions, and all of those functions are themselves syntactically valid
    // according to their argument grammars, the entire value’s grammar must be
    // assumed to be valid at parse time."
    // https://drafts.csswg.org/css-values-5/#arbitrary-substitution
    // Claim: "A math function represents a numeric value, one of: <length>,
    // <frequency>, <angle>, <time>, <flex>, <resolution>, <percentage>,
    // <number>, <integer> ...or the <length-percentage>/etc mixed types, and
    // can be used wherever such a value would be valid."
    // https://drafts.csswg.org/css-values-4/#math
    #[test]
    fn presentation_attributes_accept_substitution_and_math_functions() {
        assert_attr(
            "fill-rule",
            &[
                "var(--rule)",
                "var(--rule, evenodd)",
                "attr(data-rule type(<custom-ident>))",
                "env(rule)",
            ],
            &[],
        );
        assert_attr(
            "display",
            &[
                "var(--d) !important",
                "if(media(print): none; else: inline)",
            ],
            &[],
        );
        assert_attr(
            "font-weight",
            &["calc(400 + 100)", "clamp(100, 50 * 10, 900)"],
            &[],
        );
        assert_attr("font-style", &["oblique calc(10deg * 2)"], &[]);
    }

    // Claims: the property value grammars, quoted from each definition.
    // "nonzero | evenodd" https://drafts.csswg.org/css-masking-1/#propdef-clip-rule
    // "nonzero | evenodd" https://svgwg.org/svg2-draft/painting.html#FillRuleProperty
    // "auto | sRGB | linearRGB" https://svgwg.org/svg2-draft/painting.html#ColorInterpolationProperty
    // "auto | sRGB | linearRGB" https://drafts.csswg.org/filter-effects-1/#propdef-color-interpolation-filters
    // "ltr | rtl" https://drafts.csswg.org/css-writing-modes-4/#propdef-direction
    // "<line-style> = none | hidden | dotted | dashed | solid | double | groove | ridge | inset | outset"
    //   https://drafts.csswg.org/css-borders-4/#typedef-line-style
    // "auto | normal | none" https://drafts.csswg.org/css-fonts-4/#propdef-font-kerning
    // "none | manual | auto" https://drafts.csswg.org/css-text-4/#propdef-hyphens
    // "<isolation-mode> = auto | isolate" https://drafts.csswg.org/compositing-2/#isolated-propid
    // "auto | loose | normal | strict | anywhere" https://drafts.csswg.org/css-text-4/#propdef-line-break
    // "luminance | alpha" https://drafts.csswg.org/css-masking-1/#propdef-mask-type
    // "normal | break-word | anywhere" https://drafts.csswg.org/css-text-4/#propdef-overflow-wrap
    // "auto | bounding-box | visiblePainted | visibleFill | visibleStroke | visible | painted | fill | stroke | all | none"
    //   https://svgwg.org/svg2-draft/interact.html#PointerEventsProperty
    // "auto | optimizeSpeed | crispEdges | geometricPrecision" https://svgwg.org/svg2-draft/painting.html#ShapeRenderingProperty
    // "butt | round | square" https://svgwg.org/svg2-draft/painting.html#StrokeLinecapProperty
    // "auto | start | end | left | right | center | justify | match-parent"
    //   https://drafts.csswg.org/css-text-4/#propdef-text-align-last
    // "start | middle | end" https://svgwg.org/svg2-draft/text.html#TextAnchorProperty
    // "solid | double | dotted | dashed | wavy" https://drafts.csswg.org/css-text-decor-4/#propdef-text-decoration-style
    // "mixed | upright | sideways" https://drafts.csswg.org/css-writing-modes-4/#propdef-text-orientation
    // "auto | optimizeSpeed | optimizeLegibility | geometricPrecision"
    //   https://svgwg.org/svg2-draft/painting.html#TextRenderingProperty
    // "content-box | border-box | fill-box | stroke-box | view-box" https://drafts.csswg.org/css-transforms-1/#propdef-transform-box
    // "normal | embed | isolate | bidi-override | isolate-override | plaintext"
    //   https://drafts.csswg.org/css-writing-modes-4/#propdef-unicode-bidi
    // "none | non-scaling-stroke | non-scaling-size | non-rotation | fixed-position"
    //   https://svgwg.org/svg2-draft/coords.html#VectorEffectProperty
    // "visible | hidden | force-hidden | collapse" https://drafts.csswg.org/css-display-4/#propdef-visibility
    // "normal | break-all | keep-all | manual | auto-phrase | break-word" https://drafts.csswg.org/css-text-4/#propdef-word-break
    #[test]
    fn single_keyword_property_grammars() {
        assert_attr("clip-rule", &["nonzero", "evenodd"], &["odd"]);
        assert_attr("fill-rule", &["nonzero", "evenodd"], &["winding"]);
        assert_attr(
            "color-interpolation",
            &["auto", "sRGB", "linearRGB"],
            &["displayP3"],
        );
        assert_attr(
            "color-interpolation-filters",
            &["auto", "sRGB", "linearRGB"],
            &["rgb"],
        );
        assert_attr("direction", &["ltr", "rtl"], &["right", "ltr rtl"]);
        assert_attr("border-top-style", &["none", "groove", "outset"], &["wavy"]);
        assert_attr("font-kerning", &["auto", "normal", "none"], &["off"]);
        assert_attr("hyphens", &["none", "manual", "auto"], &["yes"]);
        assert_attr("isolation", &["auto", "isolate"], &["isolated"]);
        assert_attr(
            "line-break",
            &["auto", "loose", "strict", "anywhere"],
            &["always"],
        );
        assert_attr("mask-type", &["luminance", "alpha"], &["luminosity"]);
        assert_attr(
            "overflow-wrap",
            &["normal", "break-word", "anywhere"],
            &["break-all"],
        );
        assert_attr(
            "word-wrap",
            &["normal", "break-word", "anywhere"],
            &["keep-all"],
        );
        assert_attr(
            "pointer-events",
            &[
                "auto",
                "bounding-box",
                "visibleStroke",
                "painted",
                "all",
                "none",
            ],
            &["visibleAll"],
        );
        assert_attr(
            "shape-rendering",
            &["auto", "geometricPrecision"],
            &["optimizeQuality"],
        );
        assert_attr("stroke-linecap", &["butt", "round", "square"], &["miter"]);
        assert_attr(
            "text-align-last",
            &["start", "justify", "match-parent"],
            &["middle"],
        );
        assert_attr("text-anchor", &["start", "middle", "end"], &["center"]);
        assert_attr("text-decoration-style", &["solid", "wavy"], &["zigzag"]);
        assert_attr(
            "text-orientation",
            &["mixed", "upright", "sideways"],
            &["diagonal"],
        );
        assert_attr(
            "text-rendering",
            &["optimizeSpeed", "optimizeLegibility"],
            &["crispEdges"],
        );
        assert_attr(
            "transform-box",
            &["fill-box", "view-box", "border-box"],
            &["padding-box"],
        );
        assert_attr(
            "unicode-bidi",
            &["isolate-override", "plaintext"],
            &["override"],
        );
        assert_attr(
            "vector-effect",
            &[
                "none",
                "non-scaling-stroke",
                "non-scaling-size",
                "non-rotation",
                "fixed-position",
            ],
            &["scale"],
        );
        assert_attr(
            "visibility",
            &["visible", "collapse", "force-hidden"],
            &["invisible"],
        );
        assert_attr(
            "word-break",
            &["auto-phrase", "break-word", "keep-all"],
            &["break-anywhere"],
        );
    }

    // Claim: "UAs may accept sideways-right as a value that computes to
    // sideways if needed for backward compatibility reasons."
    // https://drafts.csswg.org/css-writing-modes-4/#valdef-text-orientation-sideways
    #[test]
    fn text_orientation_sideways_right_is_accepted() {
        assert_attr("text-orientation", &["sideways-right"], &[]);
    }

    // Claim: "This property previously accepted the values optimizeSpeed and
    // optimizeQuality. These are now deprecated; a user agent must accept them
    // as valid values"
    // https://drafts.csswg.org/css-images-3/#the-image-rendering
    // Claim: "auto | smooth | high-quality | pixelated | crisp-edges"
    // https://drafts.csswg.org/css-images-3/#propdef-image-rendering
    #[test]
    fn image_rendering_grammar() {
        assert_attr(
            "image-rendering",
            &[
                "auto",
                "smooth",
                "high-quality",
                "pixelated",
                "crisp-edges",
                "optimizeSpeed",
                "optimizeQuality",
            ],
            &["nearest", "pixelated smooth"],
        );
    }

    // Claim: "SVG1.1 [SVG11] defines some additional values: lr, lr-tb, rl,
    // rl-tb, tb, and tb-rl."
    // https://drafts.csswg.org/css-writing-modes-4/#svg-writing-mode
    // Claim: "horizontal-tb | vertical-rl | vertical-lr | sideways-rl | sideways-lr"
    // https://drafts.csswg.org/css-writing-modes-4/#propdef-writing-mode
    #[test]
    fn writing_mode_accepts_svg11_values() {
        let all = [
            "horizontal-tb",
            "vertical-rl",
            "sideways-lr",
            "lr",
            "lr-tb",
            "rl",
            "rl-tb",
            "tb",
            "tb-rl",
        ];
        assert_attr("writing-mode", &all, &["bottom-top", "lr tb"]);
        assert_usage("writing-mode", None, SVG11, &all, &["bottom-top"]);
    }

    // Claim: "[ <display-outside> || <display-inside> ] | <display-listitem> |
    // <display-internal> | <display-box> | <display-legacy>"
    // https://drafts.csswg.org/css-display-4/#propdef-display
    // Claim: "Name: display New values: grid-lanes | inline-grid-lanes"
    // https://drafts.csswg.org/css-grid-3/#valdef-display-grid-lanes
    // Claim: "Name: display New values: <display-outside> || [ <display-inside> | math ]"
    // https://w3c.github.io/mathml-core/#new-display-math-value
    // Claim: "<display-listitem> = <display-outside>? && [ flow | flow-root ]? && list-item"
    // https://drafts.csswg.org/css-display-4/#typedef-display-listitem
    #[test]
    fn display_grammar() {
        let valid = [
            "none",
            "contents",
            "inline",
            "block flow",
            "inline flex",
            "flow-root",
            "list-item",
            "inline flow list-item",
            "inline-block",
            "table-cell",
            "ruby-text-container",
            "grid-lanes",
            "math",
            "block math",
        ];
        assert_attr(
            "display",
            &valid,
            &[
                "hidden",
                "flexbox",
                "",
                "block flow list-item inline",
                "inline-flexbox",
            ],
        );
        assert_usage(
            "display",
            None,
            SVG11,
            &["compact", "marker", "run-in", "inline"],
            &["hidden"],
        );
    }

    // Claim: "<'overflow-block'>{1,2}"
    // https://drafts.csswg.org/css-overflow-3/#propdef-overflow
    // Claim: "visible | hidden | clip | scroll | auto"
    // https://drafts.csswg.org/css-overflow-3/#propdef-overflow-block
    // Claim: "User agents must also support the overlay keyword as a legacy
    // value alias of auto."
    // https://drafts.csswg.org/css-overflow-3/#valdef-overflow-overlay
    #[test]
    fn overflow_grammar() {
        assert_attr(
            "overflow",
            &[
                "visible",
                "hidden",
                "clip",
                "scroll",
                "auto",
                "hidden visible",
                "overlay",
            ],
            &["none", "hidden visible auto", "scrollbar"],
        );
    }

    // Claim: "miter | round | bevel"
    // https://svgwg.org/svg2-draft/painting.html#StrokeLinejoinProperty
    // Claim: "[ crop | arcs | miter ] || [ bevel | round | fallback ]"
    // https://drafts.csswg.org/fill-stroke-3/#propdef-stroke-linejoin
    // Claim: "Remove miter-clip and arcs values from stroke-linejoin"
    // https://svgwg.org/svg2-draft/changes.html#painting (valid in the 2018 CR)
    #[test]
    fn stroke_linejoin_grammar() {
        assert_attr(
            "stroke-linejoin",
            &[
                "miter",
                "round",
                "bevel",
                "arcs",
                "miter-clip",
                "crop",
                "fallback",
                "miter bevel",
            ],
            &["sharp", "miter bevel round"],
        );
    }

    // Claim: "<font-weight-absolute> = [ normal | bold | <number [1,1000]> ]"
    // https://drafts.csswg.org/css-fonts-4/#font-weight-absolute-values
    // Claim: "Only values greater than or equal to 1, and less than or equal to
    // 1000, are valid, and all other values are invalid."
    // https://drafts.csswg.org/css-fonts-4/#font-weight-numeric-values
    // Claim: "<font-weight-absolute> | bolder | lighter"
    // https://drafts.csswg.org/css-fonts-4/#propdef-font-weight
    #[test]
    fn font_weight_grammar() {
        assert_attr(
            "font-weight",
            &[
                "normal", "bold", "bolder", "lighter", "400", "550", "1", "1000", "123.5", "BOLD",
            ],
            &["0", "1001", "-100", "heavy", "400px", "bold 400", "50%"],
        );
    }

    // Claim: "For historical reasons, a font-stretch property exists which is
    // a legacy name alias and functions in the identical way to the
    // font-width."
    // https://drafts.csswg.org/css-fonts-4/#propdef-font-stretch
    // Claim: "normal | <percentage [0,∞]> | ultra-condensed | extra-condensed |
    // condensed | semi-condensed | semi-expanded | expanded | extra-expanded |
    // ultra-expanded"
    // https://drafts.csswg.org/css-fonts-4/#propdef-font-width
    #[test]
    fn font_stretch_grammar() {
        assert_attr(
            "font-stretch",
            &["normal", "condensed", "ultra-expanded", "50%", "0%", "150%"],
            &["-10%", "wide", "50", "condensed expanded"],
        );
        assert_usage(
            "font-stretch",
            None,
            SVG11,
            &["wider", "narrower"],
            &["wide"],
        );
    }

    // Claim: "normal | italic | left | right | oblique <angle [-90deg,90deg]>?"
    // https://drafts.csswg.org/css-fonts-4/#propdef-font-style
    // Claim: "Replace all instances of <angle> in grammar with [<angle> | <number>]."
    // https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value
    #[test]
    fn font_style_grammar() {
        assert_attr(
            "font-style",
            &[
                "normal",
                "italic",
                "oblique",
                "oblique 10deg",
                "oblique -0.1turn",
                "oblique 14",
                "left",
                "right",
            ],
            &[
                "slanted",
                "italic 10deg",
                "oblique wide",
                "oblique 10deg 5deg",
                "10deg",
            ],
        );
    }

    // Claim: "normal | italic | left | right | oblique <angle [-90deg,90deg]>?"
    // https://drafts.csswg.org/css-fonts-4/#propdef-font-style
    // Claim: "Fractional and negative values are accepted; however, values less
    // than -90deg or values greater than 90deg are invalid."
    // https://drafts.csswg.org/css-fonts-4/#valdef-font-style-oblique-angle--90deg-90deg
    // Claim: "If the value is outside the allowed range, then unless otherwise
    // specified, the declaration is invalid and must be ignored."
    // https://drafts.csswg.org/css-values-4/#numeric-ranges
    // Claim: "numeric functions returning out-of-range values never cause a
    // declaration to become invalid. Instead, the value of a numeric function
    // is clamped to the range allowed in the context it is used at computed
    // value time if possible, and at used value time otherwise."
    // https://drafts.csswg.org/css-values-4/#functional-numeric
    // A bare <number> (the presentation attribute's stand-in for <angle>) has
    // no range in the substituted grammar, so it is accepted at any value.
    #[test]
    fn font_style_oblique_angle_range() {
        assert_attr(
            "font-style",
            &[
                "oblique 90deg",
                "oblique -90deg",
                "oblique 90DEG",
                "oblique 100grad",
                "oblique -100grad",
                "oblique 0.25turn",
                "oblique 1.5rad",
                "oblique calc(91deg)",
                "oblique 91",
            ],
            &[
                "oblique 91deg",
                "oblique -91deg",
                "oblique 90.5deg",
                "oblique 101grad",
                "oblique 1.6rad",
                "oblique 0.26turn",
                "oblique -1turn",
            ],
        );
    }

    // Claim: "auto | 0deg | 90deg | 0 | 90"
    // https://drafts.csswg.org/css-writing-modes-4/#propdef-glyph-orientation-vertical
    // Claim: "the obsolete SVG glyph-orientation-vertical property, which was
    // defined to accept an auto keyword as well as <angle> and <integer>
    // values representing multiples of 90°."
    // https://drafts.csswg.org/css-writing-modes-4/#propdef-glyph-orientation-vertical
    #[test]
    fn glyph_orientation_vertical_grammar() {
        assert_attr(
            "glyph-orientation-vertical",
            &["auto", "0", "90", "0deg", "90deg", "90DEG", "180", "270deg"],
            &["upright", "auto 90", "90px", "90%"],
        );
    }

    // Claim: "auto | 0deg | 90deg | 0 | 90"
    // https://drafts.csswg.org/css-writing-modes-4/#propdef-glyph-orientation-vertical
    // Claim: "UAs must ignore and treat as invalid any other values for the
    // glyph-orientation-vertical property"
    // https://drafts.csswg.org/css-writing-modes-4/#propdef-glyph-orientation-vertical
    // Claim: "the obsolete SVG glyph-orientation-vertical property, which was
    // defined to accept an auto keyword as well as <angle> and <integer>
    // values representing multiples of 90°."
    // https://drafts.csswg.org/css-writing-modes-4/#propdef-glyph-orientation-vertical
    // Claim: "Note: The 180deg and 270deg values, the radian and gradian
    // values, and the glyph-orientation-horizontal property are not mapped"
    // https://drafts.csswg.org/css-writing-modes-4/#propdef-glyph-orientation-vertical
    #[test]
    fn glyph_orientation_vertical_is_a_multiple_of_90_degrees() {
        let valid = [
            "0",
            "90",
            "180",
            "270",
            "-90deg",
            "360deg",
            "90.0",
            "100grad",
            "0.25turn",
            "3.14159rad",
            "1.5708rad",
            "0rad",
        ];
        let invalid = [
            "45deg", "1rad", "45", "100deg", "89deg", "50grad", "0.1turn",
        ];
        assert_attr("glyph-orientation-vertical", &valid, &invalid);
        assert_usage("glyph-orientation-vertical", None, SVG11, &valid, &invalid);
    }

    // Claim: "Using the CSS Value Definition Syntax [css-values]. This is the
    // notation used to define the syntax for most attributes in this
    // specification and is the default."
    // https://svgwg.org/svg2-draft/types.html#syntax
    // Claim: "When any other attribute defined using the CSS Value Definition
    // Syntax is parsed, this is done by parsing the attribute's value according
    // to the grammar given in attribute definition table."
    // https://svgwg.org/svg2-draft/types.html#attribute-css-value
    // Claim: "pad | reflect | repeat"
    // https://svgwg.org/svg2-draft/pservers.html#LinearGradientElementSpreadMethodAttribute
    #[test]
    fn svg_attributes_parse_as_css_grammar() {
        assert_attr(
            "spreadMethod",
            &[
                "pad", "reflect", "repeat", " pad ", "PAD", "Reflect", "pad/**/", "var(--m)",
            ],
            &[
                "",
                "  ",
                "inherit",
                "initial",
                "mirror",
                "pad reflect",
                "pad,",
                "'pad'",
                "pad !important",
            ],
        );
    }

    // Claims: the keyword grammars of the remaining SVG attributes, quoted from
    // each definition.
    // "userSpaceOnUse | objectBoundingBox" https://svgwg.org/svg2-draft/pservers.html#LinearGradientElementGradientUnitsAttribute
    // "userSpaceOnUse | objectBoundingBox" https://svgwg.org/svg2-draft/pservers.html#PatternElementPatternUnitsAttribute
    // "userSpaceOnUse | objectBoundingBox" https://svgwg.org/svg2-draft/pservers.html#PatternElementPatternContentUnitsAttribute
    // "strokeWidth | userSpaceOnUse" https://svgwg.org/svg2-draft/painting.html#MarkerUnitsAttribute
    // "spacing | spacingAndGlyphs" https://svgwg.org/svg2-draft/text.html#TextElementLengthAdjustAttribute
    // "align | stretch" https://svgwg.org/svg2-draft/text.html#TextPathElementMethodAttribute
    // "auto | exact" https://svgwg.org/svg2-draft/text.html#TextPathElementSpacingAttribute
    // "left | right" https://svgwg.org/svg2-draft/text.html#TextPathElementSideAttribute
    // "disable | magnify" https://www.w3.org/TR/SVG11/struct.html#SVGElementZoomAndPanAttribute
    // "false | true" https://www.w3.org/TR/SVG11/struct.html#ExternalResourcesRequiredAttribute
    // "forwardonly | all" https://svgwg.org/specs/animations/#SVGElementPlaybackorderAttribute
    // "loadend | loadbegin" https://svgwg.org/specs/animations/#SVGElementTimelinebeginAttribute
    // "always | whenNotActive | never" https://svgwg.org/specs/animations/#RestartAttribute
    // "freeze | remove" https://svgwg.org/specs/animations/#FillAttribute
    // "discrete | linear | paced | spline" https://svgwg.org/specs/animations/#CalcModeAttribute
    // "replace | sum" https://svgwg.org/specs/animations/#AdditiveAttribute
    // "none | sum" https://svgwg.org/specs/animations/#AccumulateAttribute
    // "default" https://svgwg.org/specs/animations/#OriginAttribute
    // "rotate | scale | skewX | skewY | translate" https://svgwg.org/specs/animations/#AnimateTransformElementTypeAttribute
    #[test]
    fn svg_attribute_keyword_grammars() {
        assert_attr(
            "gradientUnits",
            &["userSpaceOnUse", "objectBoundingBox"],
            &["userSpace"],
        );
        assert_attr("patternUnits", &["userSpaceOnUse"], &["user-space-on-use"]);
        assert_attr("patternContentUnits", &["objectBoundingBox"], &["bbox"]);
        assert_attr(
            "markerUnits",
            &["strokeWidth", "userSpaceOnUse"],
            &["stroke-width"],
        );
        assert_on(
            "markerUnits",
            "marker",
            &["strokeWidth"],
            &["objectBoundingBox"],
        );
        assert_attr(
            "lengthAdjust",
            &["spacing", "spacingAndGlyphs"],
            &["glyphs"],
        );
        assert_on("lengthAdjust", "text", &["spacingAndGlyphs"], &["stretch"]);
        assert_attr("method", &["align", "stretch"], &["warp"]);
        assert_attr("spacing", &["auto", "exact"], &["loose"]);
        assert_attr("side", &["left", "right"], &["both"]);
        assert_attr("zoomAndPan", &["disable", "magnify"], &["zoom"]);
        assert_attr("externalResourcesRequired", &["false", "true"], &["yes"]);
        assert_attr("playbackorder", &["forwardonly", "all"], &["backward"]);
        assert_attr("timelinebegin", &["loadend", "loadbegin"], &["onload"]);
        assert_attr(
            "restart",
            &["always", "whenNotActive", "never"],
            &["sometimes"],
        );
        assert_on("restart", "animate", &["never"], &["once"]);
        assert_on("fill", "animate", &["freeze", "remove"], &["red", "none"]);
        assert_on("fill", "animateMotion", &["freeze"], &["hold"]);
        assert_on("fill", "animateTransform", &["remove"], &["forwards"]);
        assert_on("fill", "set", &["freeze"], &["both"]);
        assert_attr(
            "calcMode",
            &["discrete", "linear", "paced", "spline"],
            &["ease"],
        );
        assert_attr("additive", &["replace", "sum"], &["add"]);
        assert_on("additive", "animate", &["sum"], &["accumulate"]);
        assert_attr("accumulate", &["none", "sum"], &["replace"]);
        assert_on("accumulate", "animate", &["sum"], &["all"]);
        assert_attr("origin", &["default"], &["center"]);
        assert_on("origin", "animate", &["default"], &["auto"]);
        assert_on("origin", "animateMotion", &["default"], &["path"]);
        assert_on(
            "type",
            "animateTransform",
            &["rotate", "skewX", "translate"],
            &["matrix"],
        );
    }

    // Claims: the keyword grammars of the Filter Effects, CSS Masking and
    // XLink attributes, quoted from each definition.
    // "userSpaceOnUse | objectBoundingBox" https://drafts.fxtf.org/css-masking-1/#element-attrdef-clippath-clippathunits
    // "userSpaceOnUse | objectBoundingBox" https://drafts.fxtf.org/css-masking-1/#element-attrdef-mask-maskunits
    // "userSpaceOnUse | objectBoundingBox" https://drafts.fxtf.org/css-masking-1/#element-attrdef-mask-maskcontentunits
    // "userSpaceOnUse | objectBoundingBox" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-filter-filterunits
    // "userSpaceOnUse | objectBoundingBox" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-filter-primitiveunits
    // "matrix | saturate | hueRotate | luminanceToAlpha" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fecolormatrix-type
    // "identity | table | discrete | linear | gamma" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fecomponenttransfer-type
    // "over | in | out | atop | xor | lighter | arithmetic" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fecomposite-operator
    // "erode | dilate" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-femorphology-operator
    // "duplicate | wrap | mirror | none" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-feconvolvematrix-edgemode
    // "duplicate | wrap | mirror | none" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fegaussianblur-edgemode
    // "false | true" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-feconvolvematrix-preservealpha
    // "R | G | B | A" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fedisplacementmap-xchannelselector
    // "R | G | B | A" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-fedisplacementmap-ychannelselector
    // "stitch | noStitch" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-feturbulence-stitchtiles
    // "fractalNoise | turbulence" https://drafts.fxtf.org/filter-effects-1/#element-attrdef-feturbulence-type
    // "If a value is supplied for a show attribute, it must be one of the
    // values "new", "replace", "embed", "other", and "none"."
    // https://www.w3.org/TR/xlink11/#show-att
    // "If a value is supplied for an actuate attribute, it must be be one of
    // the values "onLoad", "onRequest", "other", and "none"."
    // https://www.w3.org/TR/xlink11/#actuate-att
    #[test]
    fn fx_and_xlink_attribute_keyword_grammars() {
        assert_attr("clipPathUnits", &["userSpaceOnUse"], &["strokeWidth"]);
        assert_on(
            "clipPathUnits",
            "clipPath",
            &["objectBoundingBox"],
            &["viewBox"],
        );
        assert_attr("maskUnits", &["userSpaceOnUse"], &["user"]);
        assert_on("maskUnits", "mask", &["objectBoundingBox"], &["object"]);
        assert_attr("maskContentUnits", &["userSpaceOnUse"], &["content"]);
        assert_on("maskContentUnits", "mask", &["objectBoundingBox"], &["box"]);
        assert_attr("filterUnits", &["userSpaceOnUse"], &["px"]);
        assert_on(
            "filterUnits",
            "filter",
            &["objectBoundingBox"],
            &["bounding"],
        );
        assert_attr("primitiveUnits", &["objectBoundingBox"], &["primitive"]);
        assert_on("primitiveUnits", "filter", &["userSpaceOnUse"], &["filter"]);
        assert_on(
            "type",
            "feColorMatrix",
            &["matrix", "hueRotate", "luminanceToAlpha"],
            &["hue-rotate"],
        );
        assert_on(
            "type",
            "feComponentTransfer",
            &["identity", "table", "gamma"],
            &["linearRGB"],
        );
        assert_on(
            "type",
            "feTurbulence",
            &["fractalNoise", "turbulence"],
            &["perlin"],
        );
        assert_attr(
            "operator",
            &["over", "arithmetic", "lighter"],
            &["multiply"],
        );
        assert_on("operator", "feComposite", &["xor"], &["erode"]);
        assert_on("operator", "feMorphology", &["erode", "dilate"], &["over"]);
        assert_attr(
            "edgeMode",
            &["duplicate", "wrap", "mirror", "none"],
            &["clamp"],
        );
        assert_on("edgeMode", "feConvolveMatrix", &["wrap"], &["repeat"]);
        assert_on("edgeMode", "feGaussianBlur", &["none"], &["edge"]);
        assert_attr("preserveAlpha", &["false", "true"], &["1"]);
        assert_on("preserveAlpha", "feConvolveMatrix", &["true"], &["yes"]);
        assert_attr("xChannelSelector", &["R", "A"], &["X", "RG"]);
        assert_on("xChannelSelector", "feDisplacementMap", &["G"], &["alpha"]);
        assert_attr("yChannelSelector", &["B"], &["Y"]);
        assert_on("yChannelSelector", "feDisplacementMap", &["A"], &["red"]);
        assert_attr("stitchTiles", &["stitch", "noStitch"], &["tile"]);
        assert_on("stitchTiles", "feTurbulence", &["noStitch"], &["no-stitch"]);
        assert_attr(
            "xlink:show",
            &["new", "replace", "embed", "other", "none"],
            &["popup"],
        );
        assert_attr(
            "xlink:actuate",
            &["onLoad", "onRequest", "other", "none"],
            &["onClick"],
        );
    }

    // Claim: "Specifies the namespace in which the target attribute and its
    // associated values are defined. The attribute value is one of the
    // following (values are case-sensitive):" CSS, XML, auto
    // https://www.w3.org/TR/SVG11/animate.html#AttributeTypeAttribute
    #[test]
    fn attribute_type_is_case_sensitive() {
        assert_attr(
            "attributeType",
            &["CSS", "XML", "auto", " XML "],
            &["css", "Xml", "AUTO", "SMIL", "", "CSS XML"],
        );
    }

    // Claim: "Deprecated XML attribute to specify whether white space is
    // preserved in character data. The only possible values are the strings
    // 'default' and 'preserve', without white space."
    // https://svgwg.org/svg2-draft/struct.html#XMLSpaceAttribute
    #[test]
    fn xml_space_is_one_of_two_exact_strings() {
        assert_attr(
            "xml:space",
            &["default", "preserve"],
            &[" preserve", "preserve ", "Preserve", "DEFAULT", "", "keep"],
        );
        assert_on(
            "xml:space",
            "core-attributes",
            &["preserve"],
            &["preserve "],
        );
    }

    // Claim: "These attributes, if specified, must have a value that is the
    // WAI-ARIA value type in the "Value" field of the definition for the state
    // or property, mapped to the appropriate SVG value type"
    // https://svgwg.org/svg2-draft/struct.html#ARIAStateandPropertyAttributes
    // Claim: "token: One of a limited set of allowed values."
    // https://www.w3.org/TR/wai-aria-1.2/#propcharacteristic_value
    #[test]
    fn aria_tokens_are_one_of_the_allowed_values() {
        assert_attr(
            "aria-autocomplete",
            &["inline", "list", "both", "none"],
            &["all"],
        );
        assert_attr(
            "aria-checked",
            &["true", "false", "mixed", "undefined"],
            &["checked", "true false"],
        );
        assert_attr(
            "aria-current",
            &["page", "step", "location", "date", "time", "true", "false"],
            &["yes"],
        );
        assert_attr("aria-expanded", &["true", "false", "undefined"], &["open"]);
        assert_attr("aria-grabbed", &["true", "false", "undefined"], &["held"]);
        assert_attr(
            "aria-haspopup",
            &["false", "true", "menu", "listbox", "tree", "grid", "dialog"],
            &["popup"],
        );
        assert_attr("aria-hidden", &["true", "false", "undefined"], &["hidden"]);
        assert_attr(
            "aria-invalid",
            &["grammar", "false", "spelling", "true"],
            &["error"],
        );
        assert_attr(
            "aria-live",
            &["assertive", "off", "polite"],
            &["loud", "polite off"],
        );
        assert_attr(
            "aria-orientation",
            &["horizontal", "vertical", "undefined"],
            &["diagonal"],
        );
        assert_attr(
            "aria-pressed",
            &["true", "false", "mixed", "undefined"],
            &["down"],
        );
        assert_attr(
            "aria-selected",
            &["true", "false", "undefined"],
            &["selected"],
        );
        assert_attr(
            "aria-sort",
            &["ascending", "descending", "none", "other"],
            &["asc"],
        );
    }

    // Claim: "token list: A list of one or more tokens."
    // https://www.w3.org/TR/wai-aria-1.2/#propcharacteristic_value
    // Claim: "The list value types—ID reference list and token list—allow more
    // than one value of the given type to be provided."
    // https://www.w3.org/TR/wai-aria-1.2/#state_property_processing
    #[test]
    fn aria_token_lists() {
        assert_attr(
            "aria-relevant",
            &[
                "additions text",
                "text additions",
                "all",
                "removals",
                "additions removals text",
                "text",
            ],
            &["changes", "additions changes"],
        );
        assert_attr(
            "aria-dropeffect",
            &["copy", "copy move", "none", "popup execute link"],
            &["drag", "copy drag"],
        );
    }

    // Claim: "Authors MAY specify a zero-length string ("") for any supported
    // (but not required) state or property."
    // https://www.w3.org/TR/wai-aria-1.2/#state_property_processing
    // Claim: "Each value in the table is a keyword for the attribute, mapping
    // to a state of the same name."
    // https://www.w3.org/TR/wai-aria-1.2/#enumerated-attribute-values
    // Claim: "If the attribute's value is an ASCII case-insensitive match for
    // one of the keywords defined for the attribute, then return the state
    // represented by that keyword."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#enumerated-attribute
    #[test]
    fn aria_accepts_empty_and_case_variants() {
        assert_attr("aria-hidden", &["", "TRUE", "False", " true "], &[]);
        assert_attr("aria-live", &["", "Polite"], &[]);
        assert_attr("aria-relevant", &["", "Additions Text"], &[]);
    }

    // Claim: "For authoring conformance purposes, if an enumerated attribute is
    // specified, the attribute's value must be one of: An ASCII
    // case-insensitive match for one of the conforming keywords for that
    // attribute, with no leading or trailing whitespace."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#enumerated-attribute
    // Claim: "sync" / "async" / "auto" (image decoding hint keywords)
    // https://html.spec.whatwg.org/multipage/images.html#image-decoding-hint
    // Claim: "A fetch priority attribute is an enumerated attribute with the
    // following keywords and states:" high, low, auto
    // https://html.spec.whatwg.org/multipage/urls-and-fetching.html#fetch-priority-attribute
    #[test]
    fn html_enumerated_attributes() {
        assert_attr(
            "decoding",
            &["sync", "async", "auto", "ASYNC"],
            &["", " async", "async ", "lazy"],
        );
        assert_attr(
            "fetchpriority",
            &["high", "low", "auto", "High"],
            &["", "highest", " low"],
        );
    }

    // Claim: "The empty string and the attribute must have an empty value
    // default defined."
    // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#enumerated-attribute
    // Claim: "The attribute's missing value default is the No CORS state, and
    // its invalid value default and empty value default are both the Anonymous
    // state."
    // https://html.spec.whatwg.org/multipage/urls-and-fetching.html#cors-settings-attribute
    // Claim: "The crossorigin attribute is a CORS settings attribute."
    // https://drafts.fxtf.org/filter-effects-1/#element-attrdef-feimage-crossorigin
    #[test]
    fn crossorigin_is_a_cors_settings_attribute() {
        assert_attr(
            "crossorigin",
            &["anonymous", "use-credentials", "", "Anonymous"],
            &["credentials", " anonymous"],
        );
        assert_on(
            "crossorigin",
            "feImage",
            &["", "use-credentials"],
            &["include"],
        );
    }
}
