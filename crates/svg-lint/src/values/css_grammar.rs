//! CSS value-definition-syntax grammars.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.
//!
//! [`check_css_grammar`] parses the catalog's grammar text as CSS Value
//! Definition Syntax (<https://drafts.csswg.org/css-values-4/#value-defs>),
//! parses the value as CSS component values ("parse something according to a
//! CSS grammar", <https://drafts.csswg.org/css-syntax-3/#parse-grammar>) and
//! matches the two with backtracking. The grammar graph in the catalog is a
//! flat token list without precedence, so the grammar text is the source.
//!
//! The value is accepted whole, without matching, when:
//!
//! - the grammar references a type, property value range or notation this
//!   module does not expand, or is not CSS value definition syntax at all
//!   (`[EBNF]` references, quoted SMIL literals);
//! - the value is a CSS-wide keyword, a `<whole-value>` notation or contains
//!   an arbitrary substitution function.
//!
//! A function in the value matches a functional notation of the same name in
//! the grammar, and a math function matches wherever a numeric type is
//! expected: its type is only known after evaluation.
//!
//! Presentation attributes are parsed with `<number>` admitted wherever the
//! catalog grammar text has `<length>`, `<length-percentage>` or `<angle>`:
//! "Replace all instances of <length> in grammar with [<length> | <number>]",
//! where grammar is "the grammar given in the attribute definition table's
//! Value column"
//! (<https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value>).
//! The replacement is textual, so under SVG 2 the types a named type or a
//! function's arguments expand to keep CSS parsing: `hue-rotate(90)` is
//! invalid. SVG 1.1 admits the number in every `<length>` and `<angle>` of a
//! presentation attribute (see [`UnitlessScope`]). The check cannot tell a
//! presentation attribute from another attribute, so it always admits the
//! number: that only accepts more.

use std::collections::BTreeSet;

use svg_data::{CssGrammarGraph, SpecSnapshotId};

use super::syntax::{
    cascade::{is_css_wide_keyword, is_css_wide_keyword_value},
    notation::{contains_arbitrary_substitution, is_math_function, is_whole_value},
    parser::{ComponentValue, Function, parse_component_values},
    tokenizer::Token,
    types::{NumericType, UnitlessMode, match_numeric},
};

/// Which `<length>`, `<length-percentage>` and `<angle>` types of a
/// presentation attribute value admit a bare `<number>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitlessScope {
    /// SVG 1.1: "When a <length> is used in an SVG presentation attribute,
    /// the syntax must match the following pattern: length ::= number ("em"
    /// | "ex" | "px" | "in" | "cm" | "mm" | "pt" | "pc" | "%")?"
    /// (<https://www.w3.org/TR/SVG11/types.html#DataTypeLength>), and "When an
    /// <angle> is used in an SVG presentation attribute, the syntax must match
    /// the following pattern: angle ::= number ("deg" | "grad" | "rad")?"
    /// (<https://www.w3.org/TR/SVG11/types.html#DataTypeAngle>): every one in
    /// the value.
    WholeValue,
    /// SVG 2: the ones written in the Value column grammar text.
    ValueColumn,
}

impl UnitlessScope {
    /// The scope `profile` defines.
    #[must_use]
    pub const fn for_profile(profile: SpecSnapshotId) -> Self {
        match profile {
            SpecSnapshotId::Svg11Rec20030114 | SpecSnapshotId::Svg11Rec20110816 => Self::WholeValue,
            SpecSnapshotId::Svg2Cr20181004 | SpecSnapshotId::Svg2EditorsDraft => Self::ValueColumn,
        }
    }

    /// How a bare number matches the types of the grammars the Value column
    /// expands to.
    const fn nested(self) -> UnitlessMode {
        match self {
            Self::WholeValue => UnitlessMode::PresentationAttribute,
            Self::ValueColumn => UnitlessMode::Css,
        }
    }
}

/// How a bare number matches numeric types: in the grammar being parsed and
/// in the grammars it expands to.
#[derive(Clone, Copy, Debug)]
struct Unitless {
    own: UnitlessMode,
    nested: UnitlessMode,
}

impl Unitless {
    /// The modes inside a grammar this one expands to.
    const fn expanded(self) -> Self {
        Self {
            own: self.nested,
            nested: self.nested,
        }
    }
}

/// `CssGrammar`: a value matching this CSS value grammar, with bare numbers
/// admitted per `scope`.
pub fn check_css_grammar(
    value: &str,
    grammar: &str,
    graph: &CssGrammarGraph,
    scope: UnitlessScope,
) -> Option<String> {
    check_css_grammar_with_ranges(value, grammar, graph, scope, true)
}

pub(super) fn check_css_grammar_with_ranges(
    value: &str,
    grammar: &str,
    _graph: &CssGrammarGraph,
    scope: UnitlessScope,
    check_ranges: bool,
) -> Option<String> {
    if OPAQUE_GRAMMARS.contains(&grammar) {
        return None;
    }
    if let Some(check) = FOREIGN_GRAMMARS
        .iter()
        .find_map(|&(catalog, check)| (catalog == grammar).then_some(check))
    {
        return check(value);
    }
    let effective = effective_grammar(grammar);
    let root = parse_grammar(effective, scope)?;

    let list = parse_component_values(value);
    if list.depth_limited
        || contains_arbitrary_substitution(&list.values)
        || is_whole_value(&list.values)
        || is_css_wide_keyword_value(value)
    {
        return None;
    }
    let values: Vec<&ComponentValue<'_>> = list.significant().collect();

    let matches = |ranges: bool| {
        Matcher {
            values: &values,
            ranges,
            nested: scope.nested(),
            components: ComponentKeywords::None,
        }
        .ends(&root, 0, true)
        .contains(&values.len())
    };
    if matches(check_ranges) {
        None
    } else if matches(false) {
        Some(format!(
            "a number is outside the range `{effective}` allows"
        ))
    } else {
        Some(format!("does not match `{grammar}`"))
    }
}

/// The grammar matched for the catalog's `grammar`: its widened, ranged or
/// translated form when one is listed, else the catalog text itself.
fn effective_grammar(grammar: &str) -> &str {
    WIDENED_GRAMMARS
        .iter()
        .chain(RANGED_GRAMMARS)
        .chain(TRANSLATED_GRAMMARS)
        .find_map(|&(catalog, matched)| (catalog == grammar).then_some(matched))
        .unwrap_or(grammar)
}

/// Catalog grammars in a notation other than CSS value definition syntax, or
/// naming a type CSS defines differently, rewritten into CSS value definition
/// syntax with names of their own.
const TRANSLATED_GRAMMARS: &[(&str, &str)] = &[
    // keySplines: "<control-point> = <number> ,? <number> ,? <number> ,?
    // <number>" (https://svgwg.org/specs/animations/#DataTypeControlPoint).
    // CSS Shapes defines a different <control-point> for shape() curves.
    (
        "<control-point> [; <control-point>]* ;?",
        "<smil-control-point> [; <smil-control-point>]* ;?",
    ),
    // dur, repeatDur, min, max: SMIL timing values with quoted literals
    // (https://svgwg.org/specs/animations/#DurAttribute).
    ("Clock-value | \"indefinite\"", "<clock-value> | indefinite"),
    ("Clock-value | \"media\"", "<clock-value> | media"),
    (
        "Clock-value | \"media\" | \"indefinite\"",
        "<clock-value> | media | indefinite",
    ),
];

/// A whole-value check: `Some(reason)` for an invalid value.
type ValueCheck = fn(&str) -> Option<String>;

/// Catalog grammars whose whole value follows a non-CSS syntax, with the check
/// for it.
const FOREIGN_GRAMMARS: &[(&str, ValueCheck)] = &[
    ("Name [EBNF]", check_xml_name),
    (
        "_self | _parent | _top | _blank | <XML-Name>",
        check_xml_name,
    ),
    ("auto | sRGB | <name> | <uri>", check_color_profile),
];

/// An XML 1.0 `Name` (<https://www.w3.org/TR/xml/#NT-Name>).
fn check_xml_name(value: &str) -> Option<String> {
    (!super::declarations::is_xml_name(value)).then(|| "is not an XML Name".to_owned())
}

/// The SVG 1.1 `color-profile` property: "auto | sRGB | <name> | <iri> |
/// inherit", where "name ::= [^,()#x20#x9#xD#xA] /* any char except ",",
/// "(", ")" or wsp */" and the IRI is written `url()` in a property value
/// (<https://www.w3.org/TR/SVG11/color.html#ColorProfileProperty>,
/// <https://www.w3.org/TR/SVG11/types.html#DataTypeName>). A `<name>` covers
/// `auto`, `sRGB` and the CSS-wide keywords.
fn check_color_profile(value: &str) -> Option<String> {
    let name = value.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{c}'));
    if !name.is_empty() && !name.contains([',', '(', ')', ' ', '\t', '\r', '\n']) {
        return None;
    }
    let list = parse_component_values(value);
    if list.depth_limited || contains_arbitrary_substitution(&list.values) {
        return None;
    }
    let mut significant = list.significant();
    let url = matches!(
        (significant.next(), significant.next()),
        (Some(only), None) if leaf_matches(Leaf::Url, UnitlessMode::Css, only)
    );
    (!url).then(|| "is neither a color profile name nor a url()".to_owned())
}

/// Catalog grammars that are not checked here.
///
/// - `none | <string>` is the `d` property. Its presentation attribute holds
///   path data, not a CSS `<string>`: "d will become a presentation attribute
///   (no name change) with path data string as value"
///   (<https://svgwg.org/svg2-draft/paths.html#TheDProperty>). Path data has
///   its own check.
/// - `<number>+` is how the catalog renders the Filter Effects number lists
///   (`kernelMatrix`, `tableValues`, `feColorMatrix` `values`), which are
///   "separated by whitespace and/or a comma" and, for `tableValues`, may be
///   empty (<https://drafts.csswg.org/filter-effects-1/#element-attrdef-fecomponenttransfer-tablevalues>).
/// - `<value>` is the animation value of `set` `to`, which "must match the
///   attribute type" of the animated attribute
///   (<https://svgwg.org/specs/animations/#SetElementToAttribute>): its syntax
///   is unknown without that attribute.
const OPAQUE_GRAMMARS: &[&str] = &["none | <string>", "<number>+", "<value>"];

/// Catalog grammars that a later definition of the same property or
/// attribute widens, with the grammar matched in their place: the union of
/// both, so a value either definition allows is accepted.
const WIDENED_GRAMMARS: &[(&str, &str)] = &[
    // width, height, inline-size: "auto | <box-size>"
    // (https://drafts.csswg.org/css-sizing-3/#propdef-width), with the
    // "New values: <anchor-size()>"
    // (https://drafts.csswg.org/css-anchor-position-1/#anchor-size-fn).
    // width and height also size filter primitives, where "A negative or
    // zero value disables the effect of the given filter primitive"
    // (https://drafts.csswg.org/filter-effects-1/#element-attrdef-filter-primitive-width),
    // so the unrestricted <length-percentage> stays.
    (
        "auto | <length-percentage>",
        "auto | <length-percentage> | <box-size> | <anchor-size()>",
    ),
    // font-size: "<absolute-size> | <relative-size> | <length-percentage
    // [0,∞]> | math" (https://drafts.csswg.org/css-fonts-4/#propdef-font-size).
    (
        "<absolute-size> | <relative-size> | <length-percentage>",
        "<absolute-size> | <relative-size> | <length-percentage [0,∞]> | math",
    ),
    // letter-spacing, word-spacing: "normal | <length-percentage>"
    // (https://drafts.csswg.org/css-text-4/#propdef-letter-spacing).
    ("normal | <length>", "normal | <length-percentage>"),
    // font-size-adjust (https://drafts.csswg.org/css-fonts-5/#propdef-font-size-adjust).
    (
        "none | <number>",
        "none | <number> | [ ex-height | cap-height | ch-width | ic-width | ic-height ]? [ \
         from-font | <number [0,∞]> ]",
    ),
    // stroke-width: "[ <length-percentage> | <line-width> ]#"
    // (https://drafts.csswg.org/fill-stroke-3/#propdef-stroke-width). The
    // other attributes with this grammar only gain values.
    (
        "<length-percentage> | <number>",
        "[ <length-percentage> | <line-width> | <number> ]#",
    ),
    // baseline-shift: "User agents may additionally support the keyword
    // baseline as computing to 0 if is necessary for them to support legacy
    // SVG content."
    // (https://drafts.csswg.org/css-inline-3/#baseline-shift-svg-legacy),
    // and SVG 1.1 has "Value: baseline | sub | super | <percentage> |
    // <length> | inherit"
    // (https://www.w3.org/TR/SVG11/text.html#BaselineShiftProperty).
    (
        "<length-percentage> | sub | super | top | center | bottom",
        "<length-percentage> | sub | super | top | center | bottom | baseline",
    ),
    // symbol refX/refY are "Similar to the matching attributes on ‘marker’"
    // (https://svgwg.org/svg2-draft/struct.html#SymbolElementRefXAttribute).
    (
        "<length> | left | center | right",
        "<length-percentage> | <number> | left | center | right",
    ),
    (
        "<length> | top | center | bottom",
        "<length-percentage> | <number> | top | center | bottom",
    ),
];

/// Catalog grammars whose definition restricts a numeric range the catalog
/// text leaves out, with the grammar matched in their place. Each one is
/// the only catalog usage of its grammar text, so the range cannot reach an
/// attribute it does not apply to.
///
/// A range opened with `(` instead of `[` excludes its lower bound, and one
/// closed with `)` instead of `]` its upper bound, for a prose constraint
/// value definition syntax cannot write.
const RANGED_GRAMMARS: &[(&str, &str)] = &[
    // font-width: "normal | <percentage [0,∞]> | ultra-condensed | ..."
    // (https://drafts.csswg.org/css-fonts-4/#propdef-font-width).
    (
        "normal | <percentage> | ultra-condensed | extra-condensed | condensed | semi-condensed | \
         semi-expanded | expanded | extra-expanded | ultra-expanded",
        "normal | <percentage [0,∞]> | ultra-condensed | extra-condensed | condensed | \
         semi-condensed | semi-expanded | expanded | extra-expanded | ultra-expanded",
    ),
    // text-combine-upright: "none | all | [ digits <integer [2,4]>? ]"
    // (https://drafts.csswg.org/css-writing-modes-4/#propdef-text-combine-upright).
    (
        "none | all | [ digits <integer>? ]",
        "none | all | [ digits <integer [2,4]>? ]",
    ),
    // tab-size: "<number [0,∞]> | <length [0,∞]>"
    // (https://drafts.csswg.org/css-text-3/#tab-size-property).
    ("<number> | <length>", "<number [0,∞]> | <length [0,∞]>"),
    // line-height: "normal | <number [0,∞]> | <length-percentage [0,∞]>"
    // (https://drafts.csswg.org/css-inline-3/#propdef-line-height).
    (
        "normal | <number> | <length-percentage>",
        "normal | <number [0,∞]> | <length-percentage [0,∞]>",
    ),
    // rx, ry: "a negative <length-percentage> is [=invalid value|invalid=]"
    // (https://svgwg.org/svg2-draft/geometry.html#geometry-negative-values).
    (
        "<length-percentage> | auto",
        "<length-percentage [0,∞]> | auto",
    ),
    // repeatCount: "Values must be greater than 0."
    // (https://svgwg.org/specs/animations/#RepeatCountAttribute).
    ("<number> | indefinite", "<number (0,∞]> | indefinite"),
];

/// Non-terminals defined by a grammar of their own, expanded in place.
const NAMED_TYPES: &[(&str, &str)] = &[
    // https://drafts.csswg.org/css-sizing-4/#typedef-box-size
    (
        "box-size",
        "<length-percentage [0,∞]> | stretch | contain | min-content | max-content | fit-content \
         | stretch(<length-percentage [0,∞]>) | fit-content(<length-percentage [0,∞]>) | \
         <calc-size()>",
    ),
    // https://drafts.csswg.org/css-borders-4/#typedef-line-width
    (
        "line-width",
        "<length [0,∞]> | hairline | thin | medium | thick",
    ),
    // https://svgwg.org/svg2-draft/painting.html#DataTypeDasharray, with
    // "If any value in the list is negative, the <dasharray> value is
    // invalid." as the range.
    (
        "dasharray",
        "[ [ <length-percentage [0,∞]> | <number [0,∞]> ]+ ]#",
    ),
    // https://svgwg.org/svg2-draft/painting.html#DataTypeMarkerRef
    ("marker-ref", "<url>"),
    // "<align> = none | xMinYMin | xMidYMin | xMaxYMin | xMinYMid | xMidYMid |
    // xMaxYMid | xMinYMax | xMidYMax | xMaxYMax" and "<meetOrSlice> = meet |
    // slice"
    // https://svgwg.org/svg2-draft/coords.html#PreserveAspectRatioAttribute
    (
        "align",
        "none | xMinYMin | xMidYMin | xMaxYMin | xMinYMid | xMidYMid | xMaxYMid | xMinYMax | \
         xMidYMax | xMaxYMax",
    ),
    ("meetOrSlice", "meet | slice"),
    // "The value of the 'viewBox' attribute is a list of four numbers
    // <min-x>, <min-y>, <width> and <height>" and "A negative value for
    // <width> or <height> is an error and invalidates the 'viewBox'
    // attribute."
    // https://svgwg.org/svg2-draft/coords.html#ViewBoxAttribute
    ("min-x", "<number>"),
    ("min-y", "<number>"),
    ("width", "<number [0,∞]>"),
    ("height", "<number [0,∞]>"),
    // "<filter-primitive-reference> is an <custom-ident> [CSS3VAL] and an
    // assigned name for this filter primitive."
    // https://drafts.csswg.org/filter-effects-1/#typedef-result-filter-primitive-reference
    ("filter-primitive-reference", "<custom-ident>"),
    // CSS Fonts 4 renamed these to <font-family-name> and
    // <generic-font-family>: "<font-family-name> = <string> |
    // <custom-ident>+" and "<generic-font-family> =
    // <generic-font-script-specific>| <generic-font-complete> |
    // <generic-font-incomplete>"
    // https://drafts.csswg.org/css-fonts-4/#font-family-name-syntax
    ("family-name", "<string> | <custom-ident>+"),
    (
        "generic-family",
        "<generic-font-script-specific> | <generic-font-complete> | <generic-font-incomplete>",
    ),
    // "<url-set> is a limited version of image-set(), where the <image>
    // sub-production is restricted to <url> only."
    // https://drafts.csswg.org/css-ui-4/#typedef-cursor-url-set
    ("url-set", "<image-set()>"),
    // "Clock-value ::= ( Full-clock-value | Partial-clock-value |
    // Timecount-value )", "Full-clock-value ::= Hours ":" Minutes ":"
    // Seconds ("." Fraction)?", "Partial-clock-value ::= Minutes ":" Seconds
    // ("." Fraction)?", "Timecount-value ::= Timecount ("." Fraction)?
    // (Metric)?", "Metric ::= "h" | "min" | "s" | "ms"" and "Minutes ::=
    // 2DIGIT /* range from 00 to 59 */"
    // https://www.w3.org/TR/SMIL3/smil-timing.html#q22, repeated at
    // https://svgwg.org/specs/animations/#ClockValueSyntax
    (
        "clock-value",
        "<clock-timecount [0,∞]> | <integer [0,∞]> : <integer [0,59]> : <number [0,60)> | \
         <integer [0,59]> : <number [0,60)>",
    ),
    // A literal number 0 (https://drafts.csswg.org/css-values-4/#zero-value),
    // which @webref/css leaves without a syntax.
    ("zero", "<number [0,0]>"),
    // "<url-modifier> = <ident> | <function-token> <any-value> )"
    // (https://drafts.csswg.org/css-values-4/#typedef-url-modifier).
    ("url-modifier", "<ident> | <function-token>"),
    // clip rect(): "<top>, <right>, <bottom>, and <left> may either have a
    // <length> value or 'auto'" (https://drafts.csswg.org/css2/#value-def-shape).
    ("top", "<length> | auto"),
    ("right", "<length> | auto"),
    ("bottom", "<length> | auto"),
    ("left", "<length> | auto"),
    // "<radial-size> = <radial-extent>{1,2} | <length-percentage [0,∞]>{1,2}",
    // which "extends the <radial-size> options to include the additions from
    // the circle() and ellipse() <basic-shape> values"
    // (https://drafts.csswg.org/css-images-4/#radial-size). @webref/css
    // carries the narrower CSS Images 3 definition.
    (
        "radial-size",
        "<radial-extent>{1,2} | <length-percentage [0,∞]>{1,2}",
    ),
    // keySplines: "<control-point> = <number> ,? <number> ,? <number> ,?
    // <number>" (https://svgwg.org/specs/animations/#DataTypeControlPoint),
    // renamed in TRANSLATED_GRAMMARS.
    (
        "smil-control-point",
        "<number> ,? <number> ,? <number> ,? <number>",
    ),
    // SVG 1.1: "number-optional-number ::= number | number comma-wsp number"
    // (https://www.w3.org/TR/SVG11/types.html#DataTypeNumberOptionalNumber),
    // which Filter Effects writes as "<number-optional-number> = <number>
    // <number>?"
    // (https://drafts.csswg.org/filter-effects-1/#typedef-number-optional-number).
    // The union keeps the comma SVG 1.1 content uses.
    ("number-optional-number", "<number> [ ,? <number> ]?"),
];

/// Markers of attribute value syntaxes other than CSS value definition
/// syntax (<https://svgwg.org/svg2-draft/types.html#syntax>).
const FOREIGN_SYNTAX_MARKERS: &[&str] =
    &["[EBNF]", "[ABNF]", "[URL]", "[HTML]", "(see below)", "\""];

/// How deeply named types may expand into each other.
const MAX_EXPANSION_DEPTH: usize = 8;

/// The most operands a `&&` or `||` group may have: the matcher records the
/// operands used in a `u32` bitmask. `font-variant` has 21.
const MAX_INTERLEAVED: usize = 31;

/// A grammar node.
#[derive(Clone, Debug, PartialEq)]
enum Node {
    /// A keyword, matched ASCII case-insensitively.
    Keyword(String),
    /// A literal delimiter: `,` `/` `;` `:` or a quoted one.
    Delim(char),
    /// A basic data type, with an optional range restriction and the
    /// treatment of a bare number in its place.
    Type(Leaf, Range, UnitlessMode),
    /// A functional notation, written literally or as `<name()>`. Only a
    /// function of that name matches, with arguments matching the function's
    /// own definition (see [`function_arguments`]).
    Function(String),
    /// Juxtaposition.
    Seq(Vec<Self>),
    /// `&&`.
    AllOf(Vec<Self>),
    /// `||`.
    AnyOf(Vec<Self>),
    /// `|`.
    OneOf(Vec<Self>),
    /// A multiplier: `min` to `max` repetitions, comma-separated for `#`.
    Repeat {
        node: Box<Self>,
        min: usize,
        max: Option<usize>,
        comma: bool,
    },
    /// `!`: the group must produce at least one value.
    Required(Box<Self>),
}

/// A basic data type the matcher implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Leaf {
    Numeric(NumericType),
    String,
    Url,
    CustomIdent,
    DashedIdent,
    Ident,
    /// `<hex-color>`: a hash token of 3, 4, 6 or 8 hex digits.
    HexColor,
    /// A SMIL `Timecount-value`: a number, optionally with an `h`, `min`, `s`
    /// or `ms` metric.
    ClockTimecount,
    /// `<hash-token>`: any hash token, such as `#a` in `element(#a)`.
    HashToken,
    /// `<function-token>` with its arguments: any function.
    FunctionToken,
}

/// A range restriction, closed unless `min_exclusive` or `max_exclusive` is
/// set. A bound that is unrestricted or cannot be compared is infinite.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Range {
    min: f64,
    max: f64,
    min_exclusive: bool,
    max_exclusive: bool,
}

impl Range {
    /// No restriction: "If no range is indicated ... then [−∞,∞] is
    /// assumed."
    const UNRESTRICTED: Self = Self {
        min: f64::NEG_INFINITY,
        max: f64::INFINITY,
        min_exclusive: false,
        max_exclusive: false,
    };

    fn contains(self, value: f64) -> bool {
        (self.min..=self.max).contains(&value)
            && !(self.min_exclusive && value <= self.min)
            && !(self.max_exclusive && value >= self.max)
    }
}

/// One lexical unit of a grammar.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Lexeme<'g> {
    Keyword(&'g str),
    /// `name(`.
    Function(&'g str),
    /// The text between `<` and `>`.
    Type(&'g str),
    Delim(char),
    OpenGroup,
    CloseGroup,
    CloseParen,
    Bar,
    DoubleBar,
    DoubleAmpersand,
    Star,
    Plus,
    Question,
    Hash,
    Bang,
    Braces(usize, Option<usize>),
}

/// Split a grammar into lexemes, or `None` for text this module does not
/// read as CSS value definition syntax.
fn lex(grammar: &str) -> Option<Vec<Lexeme<'_>>> {
    if FOREIGN_SYNTAX_MARKERS
        .iter()
        .any(|marker| grammar.contains(marker))
    {
        return None;
    }
    let mut lexemes = Vec::new();
    let mut rest = grammar;
    while let Some(c) = rest.chars().next() {
        let (lexeme, len) = match c {
            c if c.is_whitespace() => {
                rest = &rest[c.len_utf8()..];
                continue;
            }
            '[' => (Lexeme::OpenGroup, 1),
            ']' => (Lexeme::CloseGroup, 1),
            ')' => (Lexeme::CloseParen, 1),
            '|' if rest.starts_with("||") => (Lexeme::DoubleBar, 2),
            '|' => (Lexeme::Bar, 1),
            '&' if rest.starts_with("&&") => (Lexeme::DoubleAmpersand, 2),
            '*' => (Lexeme::Star, 1),
            '+' => (Lexeme::Plus, 1),
            '?' => (Lexeme::Question, 1),
            '#' => (Lexeme::Hash, 1),
            '!' => (Lexeme::Bang, 1),
            ',' | '/' | ';' | ':' => (Lexeme::Delim(c), 1),
            '{' => {
                let end = rest.find('}')?;
                (lex_braces(&rest[1..end])?, end + 1)
            }
            '<' => {
                let end = rest.find('>')?;
                (Lexeme::Type(rest[1..end].trim()), end + 1)
            }
            '\'' => {
                let mut chars = rest[1..].chars();
                let delim = chars.next()?;
                if chars.next() != Some('\'') || delim == '\'' {
                    return None;
                }
                (Lexeme::Delim(delim), 2 + delim.len_utf8())
            }
            c if c.is_ascii_alphabetic() || c == '-' || c == '_' => {
                let len = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                    .unwrap_or(rest.len());
                if rest[len..].starts_with('(') {
                    (Lexeme::Function(&rest[..len]), len + 1)
                } else {
                    (Lexeme::Keyword(&rest[..len]), len)
                }
            }
            _ => return None,
        };
        lexemes.push(lexeme);
        rest = &rest[len..];
    }
    Some(lexemes)
}

/// `{A}`, `{A,}` or `{A,B}`, given the text between the braces.
fn lex_braces(inner: &str) -> Option<Lexeme<'static>> {
    let parse = |text: &str| text.trim().parse::<usize>().ok();
    Some(match inner.split_once(',') {
        None => {
            let count = parse(inner)?;
            Lexeme::Braces(count, Some(count))
        }
        Some((min, max)) if max.trim().is_empty() => Lexeme::Braces(parse(min)?, None),
        Some((min, max)) => Lexeme::Braces(parse(min)?, Some(parse(max)?)),
    })
}

/// Parse a catalog grammar, or `None` when it uses anything this module does
/// not implement.
fn parse_grammar(grammar: &str, scope: UnitlessScope) -> Option<Node> {
    let unitless = Unitless {
        own: UnitlessMode::PresentationAttribute,
        nested: scope.nested(),
    };
    parse_grammar_unwrapping(grammar, 0, None, unitless)
}

/// Parse a grammar in which each `unwrap(…)` functional notation stands for
/// its arguments, as a group: the grammar of what may appear inside a
/// function named `unwrap`.
fn parse_grammar_unwrapping(
    grammar: &str,
    depth: usize,
    unwrap: Option<&str>,
    unitless: Unitless,
) -> Option<Node> {
    if depth > MAX_EXPANSION_DEPTH {
        return None;
    }
    let lexemes = lex(grammar)?;
    let mut parser = GrammarParser {
        lexemes: &lexemes,
        pos: 0,
        depth,
        unwrap,
        unitless,
    };
    let node = parser.alternatives()?;
    (parser.pos == lexemes.len()).then_some(node)
}

/// Functions whose definition restricts its arguments in prose, with the
/// grammar matched in place of every catalog definition.
const FUNCTION_SYNTAXES: &[(&str, &str)] = &[
    // circle(): "circle( <radial-size>? [ at <position> ]? )" and "Two
    // <length-percentage> values are invalid."
    // (https://drafts.csswg.org/css-shapes-1/#funcdef-basic-shape-circle),
    // with <radial-size> from NAMED_TYPES.
    (
        "circle",
        "circle( [ <radial-extent>{1,2} | <length-percentage [0,∞]> ]? [ at <position> ]? )",
    ),
];

/// The grammar of the arguments of a function named `name`, from
/// [`FUNCTION_SYNTAXES`] or every catalog definition of it, with bare numbers
/// matched per `unitless`, or `None` when it has none or one this module
/// cannot parse.
fn function_arguments(name: &str, unitless: UnitlessMode) -> Option<Node> {
    let unitless = Unitless {
        own: unitless,
        nested: unitless,
    };
    let name = name.to_ascii_lowercase();
    let syntaxes: Vec<&str> = match FUNCTION_SYNTAXES
        .iter()
        .find(|&&(function, _)| function == name)
    {
        Some(&(_, syntax)) => vec![syntax],
        None => svg_data::css_function_syntaxes(&name).collect(),
    };
    let mut alternatives = syntaxes
        .into_iter()
        .map(|syntax| parse_grammar_unwrapping(syntax, 0, Some(&name), unitless))
        .collect::<Option<Vec<Node>>>()?;
    match alternatives.len() {
        0 => None,
        1 => alternatives.pop(),
        _ => Some(Node::OneOf(alternatives)),
    }
}

/// Recursive descent over grammar lexemes, by combinator precedence:
/// "Juxtaposition is stronger than the double ampersand, the double
/// ampersand is stronger than the double bar, and the double bar is
/// stronger than the bar."
struct GrammarParser<'l, 'g, 'u> {
    lexemes: &'l [Lexeme<'g>],
    pos: usize,
    depth: usize,
    /// The function whose notation is replaced by its arguments, while
    /// parsing outside any of its argument lists.
    unwrap: Option<&'u str>,
    unitless: Unitless,
}

impl<'g> GrammarParser<'_, 'g, '_> {
    fn peek(&self) -> Option<Lexeme<'g>> {
        self.lexemes.get(self.pos).copied()
    }

    fn eat(&mut self, lexeme: Lexeme<'g>) -> bool {
        let found = self.peek() == Some(lexeme);
        if found {
            self.pos += 1;
        }
        found
    }

    /// Operands joined by `separator`, each parsed by `operand`.
    fn joined(
        &mut self,
        separator: Lexeme<'g>,
        operand: fn(&mut Self) -> Option<Node>,
        combine: fn(Vec<Node>) -> Node,
    ) -> Option<Node> {
        let mut operands = vec![operand(self)?];
        while self.eat(separator) {
            operands.push(operand(self)?);
        }
        Some(if operands.len() == 1 {
            operands.pop()?
        } else {
            combine(operands)
        })
    }

    fn alternatives(&mut self) -> Option<Node> {
        self.joined(Lexeme::Bar, Self::any_of, Node::OneOf)
    }

    fn any_of(&mut self) -> Option<Node> {
        let node = self.joined(Lexeme::DoubleBar, Self::all_of, Node::AnyOf)?;
        (!matches!(&node, Node::AnyOf(operands) if operands.len() > MAX_INTERLEAVED))
            .then_some(node)
    }

    fn all_of(&mut self) -> Option<Node> {
        let node = self.joined(Lexeme::DoubleAmpersand, Self::sequence, Node::AllOf)?;
        (!matches!(&node, Node::AllOf(operands) if operands.len() > MAX_INTERLEAVED))
            .then_some(node)
    }

    fn sequence(&mut self) -> Option<Node> {
        let mut terms = Vec::new();
        while let Some(lexeme) = self.peek() {
            if matches!(
                lexeme,
                Lexeme::Bar
                    | Lexeme::DoubleBar
                    | Lexeme::DoubleAmpersand
                    | Lexeme::CloseGroup
                    | Lexeme::CloseParen
            ) {
                break;
            }
            terms.push(self.term()?);
        }
        match terms.len() {
            0 => None,
            1 => terms.pop(),
            _ => Some(Node::Seq(terms)),
        }
    }

    fn term(&mut self) -> Option<Node> {
        let lexeme = self.peek()?;
        self.pos += 1;
        let mut node = match lexeme {
            Lexeme::Keyword(keyword) => Node::Keyword(keyword.to_owned()),
            Lexeme::Delim(delim) => Node::Delim(delim),
            Lexeme::Type(reference) => {
                resolve_type(reference, self.depth, self.unwrap, self.unitless)?
            }
            Lexeme::OpenGroup => {
                let inner = self.alternatives()?;
                if !self.eat(Lexeme::CloseGroup) {
                    return None;
                }
                inner
            }
            Lexeme::Function(name)
                if self
                    .unwrap
                    .is_some_and(|target| target.eq_ignore_ascii_case(name)) =>
            {
                // The argument grammar of the unwrapped function: inside it,
                // functions of the same name are ordinary notations again.
                let target = self.unwrap.take();
                let arguments = if self.peek() == Some(Lexeme::CloseParen) {
                    Some(Node::Seq(Vec::new()))
                } else {
                    self.alternatives()
                };
                self.unwrap = target;
                if !self.eat(Lexeme::CloseParen) {
                    return None;
                }
                arguments?
            }
            Lexeme::Function(name) => {
                // A notation's arguments are matched against the function's
                // own definition when a value uses it (see
                // [`function_arguments`]), so their types are not resolved
                // here: `<color>` reaches itself through
                // `light-dark(<color>, <color>)`.
                let mut open = 1_usize;
                while open > 0 {
                    match self.peek()? {
                        Lexeme::Function(_) => open += 1,
                        Lexeme::CloseParen => open -= 1,
                        _ => {}
                    }
                    self.pos += 1;
                }
                Node::Function(name.to_owned())
            }
            _ => return None,
        };
        while let Some(lexeme) = self.peek() {
            let (min, max, comma) = match lexeme {
                Lexeme::Star => (0, None, false),
                Lexeme::Plus => (1, None, false),
                Lexeme::Question => (0, Some(1), false),
                Lexeme::Braces(min, max) => (min, max, false),
                Lexeme::Hash => {
                    if let Some(Lexeme::Braces(min, max)) = self.lexemes.get(self.pos + 1).copied()
                    {
                        self.pos += 1;
                        (min, max, true)
                    } else {
                        (1, None, true)
                    }
                }
                Lexeme::Bang => {
                    self.pos += 1;
                    node = Node::Required(Box::new(node));
                    continue;
                }
                _ => break,
            };
            self.pos += 1;
            node = Node::Repeat {
                node: Box::new(node),
                min,
                max,
                comma,
            };
        }
        Some(node)
    }
}

/// The node for `<reference>` (the text between `<` and `>`), with its
/// expansion parsed by [`parse_grammar_unwrapping`] for `unwrap`, or `None`
/// for a reference this module does not implement.
fn resolve_type(
    reference: &str,
    depth: usize,
    unwrap: Option<&str>,
    unitless: Unitless,
) -> Option<Node> {
    if let Some(name) = reference.strip_suffix("()") {
        return Some(Node::Function(name.to_owned()));
    }
    if let Some(property) = reference
        .strip_prefix('\'')
        .and_then(|name| name.strip_suffix('\''))
    {
        return parse_grammar_unwrapping(
            svg_data::css_property_syntax(property.trim())?,
            depth + 1,
            unwrap,
            unitless.expanded(),
        );
    }
    let (name, range) = match reference.find(['[', '(']) {
        Some(open) => {
            let min_exclusive = reference[open..].starts_with('(');
            let rest = &reference[open + 1..];
            let (bounds, max_exclusive) = match rest.strip_suffix(']') {
                Some(bounds) => (bounds, false),
                None => (rest.strip_suffix(')')?, true),
            };
            (
                reference[..open].trim(),
                parse_range(bounds, min_exclusive, max_exclusive)?,
            )
        }
        None => (reference, Range::UNRESTRICTED),
    };
    let leaf = match name {
        "number" => Leaf::Numeric(NumericType::Number),
        "integer" => Leaf::Numeric(NumericType::Integer),
        "percentage" => Leaf::Numeric(NumericType::Percentage),
        "length" => Leaf::Numeric(NumericType::Length),
        "length-percentage" => Leaf::Numeric(NumericType::LengthPercentage),
        "angle" => Leaf::Numeric(NumericType::Angle),
        "angle-percentage" => Leaf::Numeric(NumericType::AnglePercentage),
        "time" => Leaf::Numeric(NumericType::Time),
        "time-percentage" => Leaf::Numeric(NumericType::TimePercentage),
        "frequency" => Leaf::Numeric(NumericType::Frequency),
        "frequency-percentage" => Leaf::Numeric(NumericType::FrequencyPercentage),
        "resolution" => Leaf::Numeric(NumericType::Resolution),
        "flex" => Leaf::Numeric(NumericType::Flex),
        "string" => Leaf::String,
        "url" | "url-token" => Leaf::Url,
        "custom-ident" => Leaf::CustomIdent,
        "dashed-ident" => Leaf::DashedIdent,
        "ident" => Leaf::Ident,
        "hex-color" => Leaf::HexColor,
        "clock-timecount" => Leaf::ClockTimecount,
        "hash-token" => Leaf::HashToken,
        "function-token" => Leaf::FunctionToken,
        _ => {
            if reference.contains(['[', '(']) {
                return None;
            }
            let grammar = NAMED_TYPES
                .iter()
                .find_map(|&(named, grammar)| (named == name).then_some(grammar))
                .or_else(|| svg_data::css_type_syntax(name))?;
            return parse_grammar_unwrapping(grammar, depth + 1, unwrap, unitless.expanded());
        }
    };
    Some(Node::Type(leaf, range, unitless.own))
}

/// The bounds of a bracketed range, `min,max` without the brackets. A bound
/// of ±∞, or one with a unit other than a zero, is left unchecked.
fn parse_range(text: &str, min_exclusive: bool, max_exclusive: bool) -> Option<Range> {
    let (min, max) = text.split_once(',')?;
    Some(Range {
        min: parse_bound(min, f64::NEG_INFINITY)?,
        max: parse_bound(max, f64::INFINITY)?,
        min_exclusive,
        max_exclusive,
    })
}

/// One range bound, `unchecked` when it is ±∞ or not comparable to a value
/// written in another unit.
fn parse_bound(text: &str, unchecked: f64) -> Option<f64> {
    let text = text.trim();
    let unsigned = text.strip_prefix(['-', '+', '\u{2212}']).unwrap_or(text);
    if unsigned == "∞" {
        return Some(unchecked);
    }
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+')))
        .unwrap_or(text.len());
    let value: f64 = text[..split].parse().ok()?;
    // "Values of 0 can be written without units": a zero is zero in any
    // unit, other bounds with a unit would need conversion.
    Some(if split == text.len() || value == 0.0 {
        value
    } else {
        unchecked
    })
}

/// Matches parsed values against a grammar node.
struct Matcher<'v, 'a> {
    /// The significant (non-whitespace) top-level component values.
    values: &'v [&'v ComponentValue<'a>],
    /// Whether range restrictions are checked.
    ranges: bool,
    /// How a bare number matches the numeric types of function arguments.
    nested: UnitlessMode,
    /// The identifiers that stand for a number in this argument list.
    components: ComponentKeywords,
}

/// The component keywords of a relative color function, which "are all
/// <number>s that correspond to the origin color's" components (CSS Color 5,
/// <https://drafts.csswg.org/css-color-5/#relative-colors>), and so match
/// wherever the function's grammar takes a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComponentKeywords {
    /// Not a relative color: identifiers are only keywords.
    None,
    /// "the allowed component keywords are" these.
    Listed(&'static [&'static str]),
    /// `color()` with `<custom-params>`, whose keywords are "defined by the
    /// components descriptor on the corresponding @color-profile", which a
    /// value check cannot see.
    Any,
}

impl ComponentKeywords {
    /// The keywords a call of `name` with significant `arguments` allows.
    fn of(name: &str, arguments: &[&ComponentValue<'_>]) -> Self {
        let relative = arguments
            .first()
            .and_then(|value| value.ident())
            .is_some_and(|ident| ident.eq_ignore_ascii_case("from"));
        if !relative {
            return Self::None;
        }
        let keywords: &'static [&'static str] = match name.to_ascii_lowercase().as_str() {
            "rgb" | "rgba" => &["r", "g", "b", "alpha"],
            "hsl" | "hsla" => &["h", "s", "l", "alpha"],
            "hwb" => &["h", "w", "b", "alpha"],
            "lab" | "oklab" => &["l", "a", "b", "alpha"],
            "lch" | "oklch" => &["l", "c", "h", "alpha"],
            "alpha" => &["alpha"],
            "color" => {
                // "color( [from <color>]? <colorspace-params> ...": the
                // color space follows the origin color.
                let space = arguments.get(2).and_then(|value| value.ident());
                return match space {
                    Some(space) if space.starts_with("--") => Self::Any,
                    Some(space)
                        if ["xyz", "xyz-d50", "xyz-d65"]
                            .iter()
                            .any(|xyz| space.eq_ignore_ascii_case(xyz)) =>
                    {
                        Self::Listed(&["x", "y", "z", "alpha"])
                    }
                    _ => Self::Listed(&["r", "g", "b", "alpha"]),
                };
            }
            _ => return Self::None,
        };
        Self::Listed(keywords)
    }

    /// Whether `value` is one of these keywords.
    fn contains(self, value: &ComponentValue<'_>) -> bool {
        let Some(ident) = value.ident() else {
            return false;
        };
        match self {
            Self::None => false,
            Self::Listed(keywords) => keywords
                .iter()
                .any(|keyword| ident.eq_ignore_ascii_case(keyword)),
            Self::Any => true,
        }
    }
}

impl Matcher<'_, '_> {
    /// Every position at which `node`, matched from `pos`, can end.
    ///
    /// `whole` is set while `node` spans the entire value (the root, through
    /// `|` alternatives), where the comma omission rule can be applied
    /// exactly.
    fn ends(&self, node: &Node, pos: usize, whole: bool) -> BTreeSet<usize> {
        match node {
            Node::Keyword(_) | Node::Delim(_) | Node::Type(..) | Node::Function(_) => self
                .values
                .get(pos)
                .filter(|value| self.single(node, value))
                .map(|_| pos + 1)
                .into_iter()
                .collect(),
            Node::Seq(items) => {
                let mut out = BTreeSet::new();
                self.sequence(items, pos, false, false, whole, &mut out);
                out
            }
            Node::AllOf(items) => self.interleaved(items, pos, true),
            Node::AnyOf(items) => self.interleaved(items, pos, false),
            Node::OneOf(items) => items
                .iter()
                .flat_map(|item| self.ends(item, pos, whole))
                .collect(),
            Node::Repeat {
                node,
                min,
                max,
                comma,
            } => self.repeat(node, pos, *min, *max, *comma),
            Node::Required(node) => {
                let mut ends = self.ends(node, pos, whole);
                ends.remove(&pos);
                ends
            }
        }
    }

    /// Whether one component value matches a single-value node.
    fn single(&self, node: &Node, value: &ComponentValue<'_>) -> bool {
        match node {
            Node::Keyword(keyword) => value
                .ident()
                .is_some_and(|ident| ident.eq_ignore_ascii_case(keyword)),
            Node::Delim(delim) => {
                matches!(
                    (delim, value),
                    (',', ComponentValue::Token(Token::Comma))
                        | (';', ComponentValue::Token(Token::Semicolon))
                        | (':', ComponentValue::Token(Token::Colon))
                ) || matches!(value, ComponentValue::Token(Token::Delim(c)) if c == delim)
            }
            Node::Function(name) => matches!(
                value,
                ComponentValue::Function(function)
                    if function.name.eq_ignore_ascii_case(name) && self.arguments_match(function)
            ),
            Node::Type(Leaf::Numeric(_), _, _) if self.components.contains(value) => true,
            Node::Type(leaf, range, unitless) => {
                leaf_matches(*leaf, *unitless, value) && (!self.ranges || in_range(*range, value))
            }
            _ => false,
        }
    }

    /// Whether `function`'s arguments match the catalog definition of a
    /// function of its name. A function without a definition this module can
    /// parse matches by name alone.
    fn arguments_match(&self, function: &Function<'_>) -> bool {
        let Some(arguments) = function_arguments(&function.name, self.nested) else {
            return true;
        };
        let values: Vec<&ComponentValue<'_>> = function
            .value
            .iter()
            .filter(|value| !value.is_whitespace())
            .collect();
        Matcher {
            values: &values,
            ranges: self.ranges,
            nested: self.nested,
            components: ComponentKeywords::of(&function.name, &values),
        }
        .ends(&arguments, 0, true)
        .contains(&values.len())
    }

    fn is_comma(&self, pos: usize) -> bool {
        matches!(
            self.values.get(pos),
            Some(ComponentValue::Token(Token::Comma))
        )
    }

    /// Juxtaposed `items` from `pos`, applying the comma omission rule
    /// (<https://drafts.csswg.org/css-values-4/#comb-comma>): a grammar
    /// comma is matched only between two items that were not omitted, and
    /// adjacent grammar commas collapse into one. `any_before` records that
    /// an earlier item consumed values; `pending` that a grammar comma is
    /// waiting for the next item that does.
    ///
    /// Outside a whole value the items around a group are unknown here, so
    /// at the edges of the group a comma may be either present or omitted.
    fn sequence(
        &self,
        items: &[Node],
        pos: usize,
        any_before: bool,
        pending: bool,
        whole: bool,
        out: &mut BTreeSet<usize>,
    ) {
        let Some((item, rest)) = items.split_first() else {
            out.insert(pos);
            if pending && !whole && self.is_comma(pos) {
                out.insert(pos + 1);
            }
            return;
        };
        if *item == Node::Delim(',') {
            self.sequence(rest, pos, any_before, true, whole, out);
            return;
        }
        let here = self.ends(item, pos, false);
        if here.contains(&pos) {
            self.sequence(rest, pos, any_before, pending, whole, out);
        }
        let comma_required = pending && any_before;
        if !comma_required {
            for &end in here.iter().filter(|&&end| end > pos) {
                self.sequence(rest, end, true, false, whole, out);
            }
        }
        if (comma_required || (pending && !whole)) && self.is_comma(pos) {
            for end in self.ends(item, pos + 1, false) {
                if end > pos + 1 {
                    self.sequence(rest, end, true, false, whole, out);
                }
            }
        }
    }

    /// `&&` (`all`) or `||` operands from `pos`, in any order, each at most
    /// once.
    fn interleaved(&self, items: &[Node], pos: usize, all: bool) -> BTreeSet<usize> {
        let full = (1_u32 << items.len()) - 1;
        let mut out = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut pending = vec![(pos, 0_u32)];
        while let Some((at, used)) = pending.pop() {
            if !seen.insert((at, used)) {
                continue;
            }
            if (all && used == full) || (!all && used != 0) {
                out.insert(at);
            }
            for (index, item) in items.iter().enumerate() {
                let bit = 1 << index;
                if used & bit == 0 {
                    pending.extend(
                        self.ends(item, at, false)
                            .into_iter()
                            .map(|end| (end, used | bit)),
                    );
                }
            }
        }
        out
    }

    /// `min` to `max` repetitions of `node` from `pos`, separated by commas
    /// when `comma` is set.
    fn repeat(
        &self,
        node: &Node,
        pos: usize,
        min: usize,
        max: Option<usize>,
        comma: bool,
    ) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        if min == 0 {
            out.insert(pos);
        }
        let mut frontier = BTreeSet::from([pos]);
        let mut count = 0;
        while !frontier.is_empty() && max.is_none_or(|max| count < max) {
            let mut next = BTreeSet::new();
            for &at in &frontier {
                let start = if comma && count > 0 {
                    if !self.is_comma(at) {
                        continue;
                    }
                    at + 1
                } else {
                    at
                };
                next.extend(self.ends(node, start, false));
            }
            count += 1;
            if count >= min {
                // A position reached again after more repetitions can only
                // reach what it reached before.
                next.retain(|&end| out.insert(end));
            }
            if count > min + self.values.len() {
                break;
            }
            frontier = next;
        }
        out
    }
}

/// The tree-counting functions, which "represent" an `<integer>` and so stand
/// in for a number where a math function could
/// (<https://drafts.csswg.org/css-values-5/#tree-counting>).
const TREE_COUNTING_FUNCTIONS: &[&str] = &["sibling-index", "sibling-count"];

/// Whether `value` is a literal of `leaf`, with a bare number treated per
/// `unitless`.
fn leaf_matches(leaf: Leaf, unitless: UnitlessMode, value: &ComponentValue<'_>) -> bool {
    if let Leaf::Numeric(ty) = leaf {
        if let ComponentValue::Function(function) = value
            && !is_math_function(&function.name)
            && !TREE_COUNTING_FUNCTIONS
                .iter()
                .any(|name| function.name.eq_ignore_ascii_case(name))
        {
            return false;
        }
        return !match_numeric(value, ty, unitless).is_no();
    }
    let token = match value {
        ComponentValue::Token(token) => token,
        // "<url> = <url()> | <src()>": a quoted url("…") or src("…") is a
        // function token. https://drafts.csswg.org/css-values-4/#urls
        ComponentValue::Function(function) => {
            return leaf == Leaf::FunctionToken
                || (leaf == Leaf::Url
                    && ["url", "src"]
                        .iter()
                        .any(|name| function.name.eq_ignore_ascii_case(name)));
        }
        ComponentValue::Block(_) => return false,
    };
    match leaf {
        Leaf::String => matches!(token, Token::String(_)),
        Leaf::Url => matches!(token, Token::Url(_)),
        Leaf::CustomIdent => matches!(
            token,
            Token::Ident(ident)
                if !is_css_wide_keyword(ident) && !ident.eq_ignore_ascii_case("default")
        ),
        Leaf::DashedIdent => matches!(token, Token::Ident(ident) if ident.starts_with("--")),
        Leaf::Ident => matches!(token, Token::Ident(_)),
        Leaf::HexColor => matches!(
            token,
            Token::Hash { value, .. }
                if matches!(value.len(), 3 | 4 | 6 | 8)
                    && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        ),
        Leaf::ClockTimecount => match token {
            Token::Number(_) => true,
            Token::Dimension { unit, .. } => matches!(unit.as_ref(), "h" | "min" | "s" | "ms"),
            _ => false,
        },
        Leaf::HashToken => matches!(token, Token::Hash { .. }),
        Leaf::Numeric(_) | Leaf::FunctionToken => false,
    }
}

/// Whether a numeric literal lies within `range`.
fn in_range(range: Range, value: &ComponentValue<'_>) -> bool {
    match value {
        ComponentValue::Token(
            Token::Number(number) | Token::Percentage(number) | Token::Dimension { number, .. },
        ) => range.contains(number.value),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use svg_data::{AttributeValues, CssGrammarGraph};

    use super::{UnitlessScope, check_css_grammar};

    const NO_GRAPH: CssGrammarGraph = CssGrammarGraph {
        root: 0,
        nodes: &[],
        edges: &[],
    };

    fn check(value: &str, grammar: &str) -> Option<String> {
        check_css_grammar(value, grammar, &NO_GRAPH, UnitlessScope::ValueColumn)
    }

    /// The catalog grammar of `attr` (on `element`, or its canonical value
    /// space), asserted to be the grammar the claim quotes.
    fn real(attr: &str, element: Option<&str>, quoted: &str) -> &'static str {
        let def = svg_data::attribute(attr).unwrap_or_else(|| panic!("{attr} is in the catalog"));
        let values = element.map_or(&def.values, |element| def.values_for_element(Some(element)));
        let AttributeValues::CssGrammar { grammar, .. } = values else {
            panic!("{attr} on {element:?} is not a CSS grammar: {values:?}");
        };
        assert_eq!(*grammar, quoted, "catalog grammar of {attr} on {element:?}");
        grammar
    }

    fn valid(grammar: &str, values: &[&str]) {
        for value in values {
            assert_eq!(
                check(value, grammar),
                None,
                "{value:?} should match {grammar:?}"
            );
        }
    }

    fn invalid(grammar: &str, values: &[&str]) {
        for value in values {
            assert!(
                check(value, grammar).is_some(),
                "{value:?} should not match {grammar:?}"
            );
        }
    }

    /// Every CSS grammar the catalog gives any attribute, in any profile or on
    /// any element.
    fn catalog_css_grammars() -> std::collections::BTreeSet<&'static str> {
        svg_data::attributes()
            .iter()
            .flat_map(|def| {
                std::iter::once(&def.values)
                    .chain(def.value_overrides.iter().map(|(_, values)| values))
                    .chain(def.element_values.iter().map(|scoped| &scoped.values))
            })
            .filter_map(|values| match values {
                AttributeValues::CssGrammar { grammar, .. } => Some(*grammar),
                _ => None,
            })
            .collect()
    }

    // Unchecked grammars accept every value, so a type left unexpanded hides
    // invalid values of every attribute that uses it.
    #[test]
    fn every_catalog_css_grammar_is_checked() {
        let unchecked: Vec<&str> = catalog_css_grammars()
            .into_iter()
            .filter(|grammar| !super::OPAQUE_GRAMMARS.contains(grammar))
            .filter(|grammar| {
                !super::FOREIGN_GRAMMARS
                    .iter()
                    .any(|(catalog, _)| catalog == grammar)
            })
            .filter(|grammar| {
                super::parse_grammar(
                    super::effective_grammar(grammar),
                    UnitlessScope::ValueColumn,
                )
                .is_none()
            })
            .collect();
        assert_eq!(unchecked, Vec::<&str>::new());
    }

    // A function whose definition does not parse matches by name alone, so
    // its arguments would go unchecked. Two kinds never reach argument
    // matching: a value with an arbitrary substitution function (`attr()`,
    // and `type()` inside it) is accepted whole, and the arguments of a math
    // function or of calc-size() ("calc-size( <calc-size-basis>, <calc-sum> )")
    // are a calculation whose type is only known after evaluation.
    #[test]
    fn every_catalog_function_checks_its_arguments() {
        use super::super::syntax::notation::{
            is_arbitrary_substitution_function, is_math_function,
        };
        let unchecked: Vec<&str> = svg_data::css_functions()
            .iter()
            .map(|&(name, _)| name)
            .filter(|&name| {
                !is_arbitrary_substitution_function(name)
                    && !is_math_function(name)
                    && !matches!(name, "type" | "calc-size")
            })
            .filter(|name| super::function_arguments(name, super::UnitlessMode::Css).is_none())
            .collect();
        assert_eq!(unchecked, Vec::<&str>::new());
    }

    #[test]
    fn delta_validation_ignores_ranges_but_checks_syntax() {
        let graph = CssGrammarGraph {
            root: 0,
            nodes: &[],
            edges: &[],
        };
        let scope = UnitlessScope::ValueColumn;
        let grammar = "<number [0,1]>";
        assert!(super::check_css_grammar("-0.5", grammar, &graph, scope).is_some());
        assert_eq!(
            super::check_css_grammar_with_ranges("-0.5", grammar, &graph, scope, false),
            None
        );
        assert!(
            super::check_css_grammar_with_ranges("red", grammar, &graph, scope, false).is_some()
        );
    }

    // Claim: "Keywords are identifiers and are interpreted ASCII
    // case-insensitively (i.e., [a-z] and [A-Z] are equivalent)."
    // https://drafts.csswg.org/css-values-4/#keywords
    // Grammar: "normal | [ fill || stroke || markers ]"
    // https://svgwg.org/svg2-draft/painting.html#PaintOrderProperty
    #[test]
    fn keywords_match_ascii_case_insensitively() {
        let grammar = real(
            "paint-order",
            None,
            "normal | [ fill || stroke || markers ]",
        );
        valid(
            grammar,
            &["normal", "NORMAL", "Fill", "stroke FILL markers"],
        );
        invalid(grammar, &["nomral", "fil", "fills", "paint"]);
    }

    // Claim: "Juxtaposing components means that all of them must occur, in
    // the given order."
    // https://drafts.csswg.org/css-values-4/#component-combinators
    // Grammar: "[ left | center | right | top | bottom | <length-percentage> ]
    // | [ left | center | right | <length-percentage> ] [ top | center |
    // bottom | <length-percentage> ] <length>? | [ [ center | left | right ]
    // && [ center | top | bottom ] ] <length>?"
    // https://drafts.csswg.org/css-transforms-1/#propdef-transform-origin
    #[test]
    fn juxtaposition_requires_every_component_in_order() {
        let grammar = real(
            "transform-origin",
            None,
            "[ left | center | right | top | bottom | <length-percentage> ] | [ left | center | \
             right | <length-percentage> ] [ top | center | bottom | <length-percentage> ] \
             <length>? | [ [ center | left | right ] && [ center | top | bottom ] ] <length>?",
        );
        valid(
            grammar,
            &[
                "left",
                "50%",
                "left top",
                "top left",
                "10px 20px",
                "10px 20px 5px",
                "center center 0",
                "right 30% 2em",
            ],
        );
        invalid(
            grammar,
            &[
                "",
                "left left",
                "top 10px",
                "10px 20px 30%",
                "10px 20px 30px 40px",
                "middle",
            ],
        );
    }

    // Claim: "A double ampersand (&&) separates two or more components, all
    // of which must occur, in any order."
    // https://drafts.csswg.org/css-values-4/#comb-all
    // Grammar: "[ <length-percentage> ] && hanging? && each-line?"
    // https://drafts.csswg.org/css-text-4/#propdef-text-indent
    #[test]
    fn double_ampersand_requires_all_in_any_order() {
        let grammar = real(
            "text-indent",
            None,
            "[ <length-percentage> ] && hanging? && each-line?",
        );
        valid(
            grammar,
            &[
                "1em",
                "hanging 1em",
                "each-line 2em hanging",
                "10% each-line",
            ],
        );
        invalid(
            grammar,
            &[
                "hanging",
                "1em 2em",
                "hanging hanging 1em",
                "1em each-line each-line",
            ],
        );
    }

    // Claim: "A double bar (||) separates two or more options: one or more
    // of them must occur, in any order."
    // https://drafts.csswg.org/css-values-4/#comb-any
    // Grammars: "none | [ underline || overline || line-through || blink ] |
    // spelling-error | grammar-error"
    // https://drafts.csswg.org/css-text-decor-4/#propdef-text-decoration-line
    // and "none | [ first || [ force-end | allow-end ] || last ]"
    // https://drafts.csswg.org/css-text-4/#propdef-hanging-punctuation
    // and "[ auto | none | inter-word | inter-character | ruby ] ||
    // no-compress" https://drafts.csswg.org/css-text-4/#propdef-text-justify
    #[test]
    fn double_bar_requires_one_or_more_in_any_order() {
        let decoration = real(
            "text-decoration-line",
            None,
            "none | [ underline || overline || line-through || blink ] | spelling-error | \
             grammar-error",
        );
        valid(
            decoration,
            &[
                "underline",
                "overline underline",
                "blink line-through overline",
                "grammar-error",
            ],
        );
        invalid(
            decoration,
            &[
                "none underline",
                "underline underline",
                "spelling-error underline",
                "strike",
            ],
        );
        let hanging = real(
            "hanging-punctuation",
            None,
            "none | [ first || [ force-end | allow-end ] || last ]",
        );
        valid(
            hanging,
            &["none", "first", "last first", "first allow-end last"],
        );
        invalid(
            hanging,
            &["force-end allow-end", "first first", "none first"],
        );
        let justify = real(
            "text-justify",
            None,
            "[ auto | none | inter-word | inter-character | ruby ] || no-compress",
        );
        valid(justify, &["auto", "no-compress", "no-compress inter-word"]);
        invalid(
            justify,
            &["auto none", "distribute", "no-compress no-compress"],
        );
    }

    // Claim: "A bar (|) separates two or more alternatives: exactly one of
    // them must occur."
    // https://drafts.csswg.org/css-values-4/#comb-one
    // Grammars: "<length-percentage> | sub | super | top | center | bottom"
    // https://drafts.csswg.org/css-inline-3/#propdef-baseline-shift and
    // "static | relative | absolute | sticky | fixed | <running()>"
    // https://drafts.csswg.org/css-position-3/#propdef-position and the
    // float grammar https://drafts.csswg.org/css-page-floats-3/#propdef-float
    #[test]
    fn bar_requires_exactly_one_alternative() {
        let shift = real(
            "baseline-shift",
            None,
            "<length-percentage> | sub | super | top | center | bottom",
        );
        valid(shift, &["sub", "super", "-2px", "10%", "3", "TOP"]);
        invalid(shift, &["sub super", "2px sub", "auto", "middle"]);
        let position = real(
            "position",
            None,
            "static | relative | absolute | sticky | fixed | <running()>",
        );
        valid(position, &["static", "fixed", "running(header)"]);
        invalid(position, &["static fixed", "float", "running"]);
        let float = real(
            "float",
            None,
            "block-start | block-end | inline-start | inline-end | snap-block | <snap-block()> | \
             snap-inline | <snap-inline()> | left | right | top | bottom | none | footnote",
        );
        valid(
            float,
            &["left", "footnote", "snap-block", "snap-block(1em, near)"],
        );
        invalid(float, &["left right", "center", "start"]);
    }

    // Claim: "Juxtaposition is stronger than the double ampersand, the
    // double ampersand is stronger than the double bar, and the double bar
    // is stronger than the bar."
    // https://drafts.csswg.org/css-values-4/#component-combinators
    // Grammars: "none | [capitalize | uppercase | lowercase ] || full-width
    // || full-size-kana | math-auto"
    // https://drafts.csswg.org/css-text-4/#propdef-text-transform and
    // "fill | none | [contain | cover] || scale-down"
    // https://drafts.csswg.org/css-images-4/#propdef-object-fit
    #[test]
    fn combinator_precedence() {
        let transform = real(
            "text-transform",
            None,
            "none | [capitalize | uppercase | lowercase ] || full-width || full-size-kana | \
             math-auto",
        );
        valid(
            transform,
            &[
                "none",
                "uppercase",
                "full-width uppercase",
                "full-size-kana",
                "math-auto",
            ],
        );
        invalid(
            transform,
            &[
                "none full-width",
                "math-auto uppercase",
                "uppercase lowercase",
            ],
        );
        let fit = real(
            "object-fit",
            None,
            "fill | none | [contain | cover] || scale-down",
        );
        valid(
            fit,
            &[
                "fill",
                "scale-down",
                "contain scale-down",
                "scale-down cover",
            ],
        );
        invalid(
            fit,
            &["fill scale-down", "contain cover", "none scale-down"],
        );
    }

    // Claim: "A question mark (?) indicates that the preceding type, word,
    // or group is optional (occurs zero or one times)."
    // https://drafts.csswg.org/css-values-4/#mult-opt
    // Grammars: "[ anonymous | use-credentials ]?"
    // https://svgwg.org/svg2-draft/embedded.html#ImageElementCrossoriginAttribute
    // and "none | all | [ digits <integer [2,4]>? ]"
    // https://drafts.csswg.org/css-writing-modes-4/#propdef-text-combine-upright
    // (the catalog grammar drops the range)
    #[test]
    fn question_mark_is_optional() {
        let image = real(
            "crossorigin",
            Some("image"),
            "[ anonymous | use-credentials ]?",
        );
        let script = real(
            "crossorigin",
            Some("script"),
            "[ anonymous | use-credentials ]?",
        );
        for grammar in [image, script] {
            valid(
                grammar,
                &["", "anonymous", "use-credentials", "USE-CREDENTIALS"],
            );
            invalid(
                grammar,
                &["anonymous use-credentials", "true", "credentials"],
            );
        }
        let combine = real(
            "text-combine-upright",
            None,
            "none | all | [ digits <integer>? ]",
        );
        valid(
            combine,
            &["none", "all", "digits", "digits 2", "digits 3", "DIGITS 4"],
        );
        invalid(
            combine,
            &[
                "digits 2.5",
                "digits 2 3",
                "2",
                "all digits",
                "digits 1",
                "digits 5",
                "digits 0",
                "digits -2",
            ],
        );
    }

    // Claim: "A comma-separated pair of numbers in curly braces ({A,B})
    // indicates that the preceding type, word, or group occurs at least A
    // and at most B times."
    // https://drafts.csswg.org/css-values-4/#mult-num-range
    // Grammar: "[ clip | ellipsis | <string> | fade | <fade()> ]{1,2}"
    // https://drafts.csswg.org/css-overflow-4/#propdef-text-overflow
    #[test]
    fn braces_bound_the_repetitions() {
        let grammar = real(
            "text-overflow",
            None,
            "[ clip | ellipsis | <string> | fade | <fade()> ]{1,2}",
        );
        valid(
            grammar,
            &[
                "clip",
                "ellipsis clip",
                "'…' fade",
                "fade(10px) clip",
                "clip clip",
            ],
        );
        invalid(grammar, &["", "clip ellipsis fade", "auto", "clip, fade"]);
    }

    // Claim: "A single number in curly braces ({A}) indicates that the
    // preceding type, word, or group occurs A times."
    // https://drafts.csswg.org/css-values-4/#mult-num
    // Grammar: "none | <angle> | [ x | y | z | <number>{3} ] && <angle>"
    // https://drafts.csswg.org/css-transforms-2/#propdef-rotate
    #[test]
    fn single_brace_number_is_exact() {
        let grammar = "none | <angle> | [ x | y | z | <number>{3} ] && <angle>";
        valid(
            grammar,
            &["none", "45deg", "x 90deg", "90deg z", "1 0 0 45deg"],
        );
        invalid(grammar, &["1 0 45deg", "1 0 0 0 45deg", "x y 45deg"]);
    }

    // Claim: "The B may be omitted ({A,}) to indicate that there must be at
    // least A repetitions, with no upper bound on the number of
    // repetitions."
    // https://drafts.csswg.org/css-values-4/#mult-num-range
    // No usage has {A,}; the grammar applies the notation to a leaf type.
    #[test]
    fn open_brace_range_has_no_upper_bound() {
        let grammar = "<integer>{2,}";
        valid(
            grammar,
            &[
                "1 2",
                "1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21",
            ],
        );
        invalid(grammar, &["1", ""]);
    }

    // Claim: "A plus (+) indicates that the preceding type, word, or group
    // occurs one or more times." https://drafts.csswg.org/css-values-4/#mult-one-plus
    // and "A hash mark (#) indicates that the preceding type, word, or group
    // occurs one or more times, separated by comma tokens (which may
    // optionally be surrounded by white space and/or comments)."
    // https://drafts.csswg.org/css-values-4/#mult-comma
    // Grammars: "[ [ <length-percentage> | <number> ]+ ]#"
    // https://svgwg.org/svg2-draft/text.html#TextElementXAttribute and
    // "[ <number>+ ]#" https://svgwg.org/svg2-draft/text.html#TextElementRotateAttribute
    #[test]
    fn plus_and_hash_repeat() {
        for attr in ["x", "y", "dx", "dy"] {
            for element in ["text", "tspan"] {
                let grammar = real(
                    attr,
                    Some(element),
                    "[ [ <length-percentage> | <number> ]+ ]#",
                );
                valid(
                    grammar,
                    &[
                        "10",
                        "10 20 30",
                        "10, 20 30",
                        "1em,2%",
                        " 5 ",
                        "10 /**/ , 20",
                    ],
                );
                invalid(grammar, &["", "10,", ",10", "10,,20", "auto", "10 auto"]);
            }
        }
        for element in ["text", "tspan"] {
            let grammar = real("rotate", Some(element), "[ <number>+ ]#");
            valid(grammar, &["30", "0 45 90", "10,20", "-5, 5 5"]);
            invalid(grammar, &["30deg", "a", "10,", ""]);
        }
    }

    // Claim: "Every type, keyword, or bracketed group may be followed by one
    // of the following modifiers: An asterisk (*) indicates that the
    // preceding type, word, or group occurs zero or more times."
    // https://drafts.csswg.org/css-values-4/#mult-zero-plus
    // and "Other delimiters must be written enclosed in single quotes (such
    // as '+')." https://drafts.csswg.org/css-values-4/#component-types
    // Grammar: "<ident> [ '.' <ident> ]*"
    // https://drafts.csswg.org/css-cascade-5/#typedef-layer-name
    #[test]
    fn asterisk_and_quoted_delimiters() {
        let grammar = "<ident> [ '.' <ident> ]*";
        valid(grammar, &["base", "base.reset", "a.b.c.d"]);
        invalid(
            grammar,
            &["", "base.", ".base", "base..x", "1", "base,reset"],
        );
    }

    // Claim: "An exclamation point (!) after a group indicates that the
    // group is required and must produce at least one value; even if the
    // grammar of the items within the group would otherwise allow the entire
    // contents to be omitted, at least one component value must not be
    // omitted." https://drafts.csswg.org/css-values-4/#mult-req
    // Grammar: "[ center | [ [ left | right | x-start | x-end ]?
    // <length-percentage>? ]! ]#"
    // https://drafts.csswg.org/css-backgrounds-4/#propdef-background-position-x
    #[test]
    fn exclamation_point_requires_a_value() {
        let grammar = "[ center | [ [ left | right | x-start | x-end ]? <length-percentage>? ]! ]#";
        valid(
            grammar,
            &["center", "left", "left 10px", "10px", "right, 5%"],
        );
        invalid(grammar, &["", ", left", "left,", "center 10px"]);
    }

    // Claim: "The + and # multipliers may be stacked as +#; similarly, the #
    // and ? multipliers, {A} and ? multipliers, and {A,B} and ? multipliers
    // may be stacked as #?, {A}?, and {A,B}?, respectively. These stacks
    // each represent the later multiplier applied to the result of the
    // earlier multiplier." https://drafts.csswg.org/css-values-4/#mult-comma
    // Grammars: "none | <length-percentage>+#"
    // https://drafts.csswg.org/fill-stroke-3/#propdef-stroke-dasharray and
    // "<length-percentage [0,∞]>{1,4} [ / <length-percentage [0,∞]>{1,4} ]?"
    // https://drafts.csswg.org/css-borders-4/#propdef-border-radius
    #[test]
    fn stacked_multipliers() {
        let dashes = "none | <length-percentage>+#";
        valid(dashes, &["none", "5", "5 3, 2", "1px 2px,3px"]);
        invalid(dashes, &["5,,3", "none 5", "5,"]);
        let radius = "<length-percentage [0,∞]>{1,4} [ / <length-percentage [0,∞]>{1,4} ]?";
        valid(
            radius,
            &[
                "1px",
                "1px 2px 3px 4px",
                "1px / 2px",
                "1px 2px / 3px 4px 5px 6px",
            ],
        );
        invalid(
            radius,
            &["1px 2px 3px 4px 5px", "/ 2px", "1px /", "1px / 2px / 3px"],
        );
        let optional_list = "<integer>#? <string>";
        valid(optional_list, &["'a'", "1 'a'", "1, 2 'a'"]);
        invalid(optional_list, &["1, 'a'", ", 'a'", "1"]);
    }

    // Claim: "Within a top-level list in a property or other CSS value, or a
    // function’s argument list, a comma specified in the grammar must be
    // omitted if: all items preceding the comma have been omitted all items
    // following the comma have been omitted multiple commas would be
    // adjacent (ignoring white space/comments), due to the items between the
    // commas being omitted." https://drafts.csswg.org/css-values-4/#comb-comma
    // Grammar: the spec's own example, "first? , second? , third?", at the
    // top level.
    #[test]
    fn grammar_commas_are_omitted_next_to_omitted_items() {
        let grammar = "first? , second? , third?";
        valid(
            grammar,
            &[
                "first, second, third",
                "first, second",
                "first, third",
                "second",
                "",
            ],
        );
        invalid(
            grammar,
            &["first, , third", ", second", "first,", "first second"],
        );
        // The background grammar shape, with leaf types substituted:
        // "<bg-layer>#? , <final-bg-layer>"
        // https://drafts.csswg.org/css-backgrounds-3/#propdef-background
        let layers = "<integer>#? , <string>";
        valid(layers, &["'a'", "1, 'a'", "1, 2, 'a'"]);
        invalid(layers, &[", 'a'", "1 'a'", "1,, 'a'", "1,"]);
    }

    // Claim: "All CSS properties also accept the CSS-wide keyword values as
    // the sole component of their property value."
    // https://drafts.csswg.org/css-values-4/#component-types
    // Grammar: "auto | <length-percentage>"
    // https://svgwg.org/svg2-draft/geometry.html#Sizing
    #[test]
    fn css_wide_keywords_alone() {
        let grammar = real("width", Some("mask"), "auto | <length-percentage>");
        valid(
            grammar,
            &["inherit", "INITIAL", " unset ", "revert", "revert-layer"],
        );
        invalid(grammar, &["inherit 5", "auto inherit", "inherit, auto"]);
    }

    // Claim: "Replace all instances of <length> in grammar with [<length> |
    // <number>]." and "Replace all instances of <length-percentage> in
    // grammar with [<length-percentage> | <number>]." and "Replace all
    // instances of <angle> in grammar with [<angle> | <number>]."
    // https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value
    // Grammars: "auto | <length-percentage>", "<length-percentage> | auto"
    // https://svgwg.org/svg2-draft/geometry.html#RxProperty, "auto |
    // <length>" (kerning), "none | <length>"
    // https://svgwg.org/svg2-draft/paths.html#PathLengthProperty
    #[test]
    fn presentation_attributes_accept_unitless_lengths_and_angles() {
        for attr in ["width", "height"] {
            let grammar = real(attr, Some("mask"), "auto | <length-percentage>");
            valid(
                grammar,
                &["100", "0", "-1.5e2", "100px", "50%", "auto", "10PX"],
            );
            invalid(grammar, &["100 px", "10qq", "10deg", "none"]);
        }
        for attr in ["rx", "ry"] {
            let grammar = real(attr, None, "<length-percentage> | auto");
            valid(grammar, &["5", "5%", "1em", "auto", "0"]);
            invalid(grammar, &["none", "5deg", "5 5"]);
        }
        let kerning = real("kerning", None, "auto | <length>");
        valid(kerning, &["auto", "2px", "0", "1.5em", "3"]);
        invalid(kerning, &["10%", "normal", "2px 3px"]);
        let path_length = real("path-length", None, "none | <length>");
        valid(path_length, &["none", "100", "100px"]);
        invalid(path_length, &["50%", "auto"]);
    }

    // Claim: "The 'r', 'rx', 'ry', width and height properties do not: a
    // negative <length-percentage> is [=invalid value|invalid=]."
    // https://svgwg.org/svg2-draft/geometry.html#geometry-negative-values
    // Grammar: "<length-percentage> | auto"
    // https://svgwg.org/svg2-draft/geometry.html#RxProperty
    // width and height share their catalog grammar with filter primitives,
    // where "A negative or zero value disables the effect of the given
    // filter primitive"
    // (https://drafts.csswg.org/filter-effects-1/#element-attrdef-filter-primitive-width),
    // so only rx and ry are range checked.
    #[test]
    fn negative_radii_are_invalid() {
        for attr in ["rx", "ry"] {
            let grammar = real(attr, None, "<length-percentage> | auto");
            valid(
                grammar,
                &["0", "0px", "5", "5%", " 1em ", "AUTO", "calc(-1px)"],
            );
            invalid(grammar, &["-5", "-1px", "-0.5%", "-1e1"]);
            let reason = check("-5", grammar).unwrap_or_default();
            assert!(reason.contains("range"), "{reason}");
        }
        let width = real("width", Some("mask"), "auto | <length-percentage>");
        valid(width, &["-5", "-1px"]);
    }

    // Claim: "Since presentation attributes are parsed as CSS values, not
    // declarations, an !important declaration within a presentation
    // attribute will cause it to have an invalid value."
    // https://svgwg.org/svg2-draft/styling.html#PresentationAttributes
    #[test]
    fn important_makes_a_presentation_attribute_invalid() {
        let grammar = real("width", Some("mask"), "auto | <length-percentage>");
        invalid(
            grammar,
            &["10px !important", "auto !important", "auto!important"],
        );
    }

    // Claim: "When written literally, an integer is one or more decimal
    // digits 0 through 9 and corresponds to a subset of the <number-token>
    // production in the CSS Syntax Module [CSS-SYNTAX-3]. The first digit of
    // an integer may be immediately preceded by - or + to indicate the
    // integer’s sign." https://drafts.csswg.org/css-values-4/#integers
    // Grammars: numOctaves = "<integer>"
    // https://drafts.csswg.org/filter-effects-1/#element-attrdef-feturbulence-numoctaves
    // and targetX = "<integer>"
    // https://drafts.csswg.org/filter-effects-1/#element-attrdef-feconvolvematrix-targetx
    // with "The value must be such that: 0 <= targetX < orderX." (likewise
    // targetY) and, for numOctaves, "Negative values are unsupported."
    // The three attributes share the catalog grammar, and the upper bound
    // depends on `order`, so negative values are not asserted either way.
    #[test]
    fn integers() {
        let octaves = real("numOctaves", Some("feTurbulence"), "<integer>");
        let target_x = real("targetX", Some("feConvolveMatrix"), "<integer>");
        let target_y = real("targetY", Some("feConvolveMatrix"), "<integer>");
        for grammar in [octaves, target_x, target_y] {
            valid(grammar, &["3", "+2", "0", " 4 "]);
            invalid(grammar, &["3.5", "1e2", "3px", "three", "1 2", ""]);
        }
    }

    // Claim: "Number values are denoted by <number>, and represent real
    // numbers, possibly with a fractional component."
    // https://drafts.csswg.org/css-values-4/#numbers
    // Grammars: "<number> | indefinite"
    // https://svgwg.org/specs/animations/#RepeatCountAttribute and "<number>
    // | auto | auto-reverse"
    // https://svgwg.org/specs/animations/#RotateAttribute
    #[test]
    fn numbers() {
        let repeat = real("repeatCount", Some("animate"), "<number> | indefinite");
        valid(repeat, &["2.5", "1e3", ".5", "indefinite", "3"]);
        invalid(repeat, &["2x", "forever", "1 2", "50%"]);
        let canonical = real("repeatCount", None, "<number> | indefinite");
        valid(canonical, &["1", "INDEFINITE"]);
        for element in ["animate", "animateMotion"] {
            let rotate = real("rotate", Some(element), "<number> | auto | auto-reverse");
            valid(rotate, &["auto", "auto-reverse", "-45", "30"]);
            invalid(rotate, &["auto-start-reverse", "45 auto", "auto auto"]);
        }
    }

    // Claim: "This is a (base 10) "floating point" numeric value that
    // specifies the number of iterations. It can include partial iterations
    // expressed as fraction values. A fractional value describes a portion
    // of the simple duration. Values must be greater than 0."
    // https://svgwg.org/specs/animations/#RepeatCountAttribute
    #[test]
    fn repeat_count_is_greater_than_zero() {
        for element in [None, Some("animate")] {
            let grammar = real("repeatCount", element, "<number> | indefinite");
            valid(
                grammar,
                &["0.5", "1e-3", "1", " 2 ", "indefinite", "+3", "calc(0)"],
            );
            invalid(grammar, &["0", "0.0", "-0", "+0", "-1", "-0.5", "0e5"]);
            let reason = check("0", grammar).unwrap_or_default();
            assert!(reason.contains("range"), "{reason}");
        }
    }

    // Claim: "Percentage values are denoted by <percentage>, and indicates a
    // value that is some fraction of another reference value."
    // https://drafts.csswg.org/css-values-4/#percentages
    // Grammar: "normal | <percentage [0,∞]> | ultra-condensed |
    // extra-condensed | condensed | semi-condensed | semi-expanded |
    // expanded | extra-expanded | ultra-expanded"
    // https://drafts.csswg.org/css-fonts-4/#propdef-font-width
    // The catalog grammar drops the range.
    #[test]
    fn percentages() {
        let grammar = real(
            "font-width",
            None,
            "normal | <percentage> | ultra-condensed | extra-condensed | condensed | \
             semi-condensed | semi-expanded | expanded | extra-expanded | ultra-expanded",
        );
        valid(
            grammar,
            &[
                "50%",
                "150.5%",
                "0%",
                "condensed",
                "ULTRA-EXPANDED",
                "normal",
            ],
        );
        invalid(
            grammar,
            &["50", "50px", "narrow", "condensed 50%", "-10%", "-0.5%"],
        );
    }

    // Claim: "Angle values are <dimension>s denoted by <angle>. The angle
    // unit identifiers are: deg ... grad ... rad ... turn"
    // https://drafts.csswg.org/css-values-4/#angles
    // Grammars: "<angle>" (glyph-orientation-horizontal) and "auto |
    // auto-start-reverse | <angle> | <number>"
    // https://svgwg.org/svg2-draft/painting.html#OrientAttribute
    #[test]
    fn angles() {
        let glyph = real("glyph-orientation-horizontal", None, "<angle>");
        valid(
            glyph,
            &["90deg", "0", "90", "1.5rad", "100grad", "0.25turn", "90DEG"],
        );
        invalid(glyph, &["90px", "auto", "90%", "90 deg"]);
        let orient = real(
            "orient",
            Some("marker"),
            "auto | auto-start-reverse | <angle> | <number>",
        );
        valid(orient, &["auto", "auto-start-reverse", "45deg", "-30"]);
        invalid(orient, &["auto-reverse", "45deg 10", "45px"]);
    }

    // Claim: "Time values are dimensions denoted by <time>. The time unit
    // identifiers are: s ... ms" https://drafts.csswg.org/css-values-4/#time
    // and "If the value is outside the allowed range, then unless otherwise
    // specified, the declaration is invalid and must be ignored."
    // https://drafts.csswg.org/css-values-4/#numeric-ranges
    // Grammar: "[ auto | <time [0s,∞]> ]#"
    // https://drafts.csswg.org/css-animations-2/#propdef-animation-duration
    #[test]
    fn times() {
        let grammar = "[ auto | <time [0s,∞]> ]#";
        valid(grammar, &["1s", "250ms, auto", "0s", "1.5S"]);
        invalid(grammar, &["1", "-1s", "1px", "auto auto"]);
    }

    // Claim: "Strings, denoted by <string>, are sequences of characters
    // representing arbitrary textual data. When written literally, they are
    // delimited by double quotes or single quotes, and correspond to the
    // <string-token> production." https://drafts.csswg.org/css-values-4/#strings
    // Grammar: "start | end | left | right | center | <string> | justify |
    // match-parent | justify-all"
    // https://drafts.csswg.org/css-text-4/#propdef-text-align
    #[test]
    fn strings() {
        let grammar = real(
            "text-align",
            None,
            "start | end | left | right | center | <string> | justify | match-parent | justify-all",
        );
        valid(grammar, &["'.'", "\"x\"", "center", "MATCH-PARENT", "''"]);
        invalid(grammar, &["middle", "'a' 'b'", "'unterminated\n"]);
    }

    // Claim: "<url> = <url()> | <src()>" and "<url()> = url( <string>
    // <url-modifier>* ) | <url-token>"
    // https://drafts.csswg.org/css-values-4/#url-value
    // and "<marker-ref> = <url>"
    // https://svgwg.org/svg2-draft/painting.html#DataTypeMarkerRef
    // Grammar: "none | <marker-ref>"
    // https://svgwg.org/svg2-draft/painting.html#MarkerProperty
    // with "Unless otherwise specified, white space and/or comments may
    // appear before, after, and/or between components combined using the
    // above combinators and multipliers."
    // https://drafts.csswg.org/css-values-4/#component-whitespace
    #[test]
    fn urls() {
        for attr in ["marker", "marker-start", "marker-mid", "marker-end"] {
            let grammar = real(attr, None, "none | <marker-ref>");
            valid(
                grammar,
                &[
                    "none",
                    "url(#m)",
                    "url('#m')",
                    "URL(#m)",
                    "src('#m')",
                    " url(#m) ",
                    "url( '#m' )",
                    "url( \"#m\")",
                    "url( #m )",
                ],
            );
            invalid(grammar, &["#m", "m", "url(#a) url(#b)", "none url(#m)"]);
        }
    }

    // Claim: "The CSS-wide keywords are not valid <custom-ident>s. The
    // default keyword is reserved and is also not a valid <custom-ident>."
    // https://drafts.csswg.org/css-values-4/#custom-idents
    // Grammar: "none | <custom-ident>+"
    // https://drafts.csswg.org/css-conditional-5/#propdef-container-name
    #[test]
    fn custom_idents() {
        let grammar = "none | <custom-ident>+";
        valid(grammar, &["none", "sidebar", "a B", "Main-Card"]);
        invalid(grammar, &["a default", "a inherit", "'a'", "1", "a, b"]);
    }

    // Claim: "The <dashed-ident> production is a <custom-ident>, with all
    // the case-sensitivity that implies, with the additional restriction
    // that it must start with two dashes (U+002D HYPHEN-MINUS)."
    // https://drafts.csswg.org/css-values-4/#typedef-dashed-ident
    // Grammar: "none | <dashed-ident>#"
    // https://drafts.csswg.org/animation-triggers-1/#propdef-event-trigger-name
    #[test]
    fn dashed_idents() {
        let grammar = "none | <dashed-ident>#";
        valid(grammar, &["none", "--a", "--a, --B"]);
        invalid(grammar, &["a", "-a", "--a --b"]);
    }

    // Claim: "Properties can restrict numeric values to some range. If the
    // value is outside the allowed range, then unless otherwise specified,
    // the declaration is invalid and must be ignored."
    // https://drafts.csswg.org/css-values-4/#numeric-ranges
    // Grammar: "normal | <number [0,∞]> | <length-percentage [0,∞]>"
    // https://drafts.csswg.org/css-inline-3/#propdef-line-height
    #[test]
    fn literal_values_outside_a_range_are_invalid() {
        let grammar = "normal | <number [0,∞]> | <length-percentage [0,∞]>";
        valid(
            grammar,
            &["normal", "1.5", "0", "20px", "120%", "calc(-1px)"],
        );
        invalid(grammar, &["-1", "-5px", "-10%"]);
        let reason = check("-1", grammar).unwrap_or_default();
        assert!(reason.contains("range"), "{reason}");
        let columns = "auto | <integer [1,∞]>";
        valid(columns, &["auto", "1", "12"]);
        invalid(columns, &["0", "-3"]);
    }

    // Claim: "If any value in the list is negative, the <dasharray> value is
    // invalid." and "<dasharray> = [ [ <length-percentage> | <number> ]+ ]#"
    // https://svgwg.org/svg2-draft/painting.html#DataTypeDasharray
    // Grammar: "none | <dasharray>"
    // https://svgwg.org/svg2-draft/painting.html#StrokeDasharrayProperty
    #[test]
    fn dasharray() {
        let grammar = real("stroke-dasharray", None, "none | <dasharray>");
        valid(
            grammar,
            &["none", "5,3,2", "5 3 2", "5, 10%  2em", "0", "20,10"],
        );
        invalid(grammar, &["5,,3", "none 5", "-1 2", "5 -3", "auto", "5,"]);
        let reason = check("5 -3", grammar).unwrap_or_default();
        assert!(reason.contains("range"), "{reason}");
    }

    // Claim: "<length-percentage> | <number>" for markerWidth, markerHeight
    // https://svgwg.org/svg2-draft/painting.html#MarkerWidthAttribute, refX
    // "<length-percentage> | <number> | left | center | right"
    // https://svgwg.org/svg2-draft/painting.html#MarkerElementRefXAttribute,
    // and the same grammar for stroke-dashoffset, startOffset, textLength
    // and the line x1, y1, x2, y2 attributes.
    // markerWidth and markerHeight: "A negative value for either attribute
    // is an error (see Error processing)."
    // https://svgwg.org/svg2-draft/painting.html#MarkerWidthAttribute and
    // textLength: "A negative value is an error (see Error processing)."
    // https://svgwg.org/svg2-draft/text.html#TextElementTextLengthAttribute
    // The grammar is shared with attributes that accept negative values, so
    // a negative value is asserted valid only for those.
    #[test]
    fn length_percentage_or_number_attributes() {
        let cases: &[(&str, Option<&str>, bool)] = &[
            ("markerWidth", Some("marker"), false),
            ("markerHeight", Some("marker"), false),
            ("stroke-dashoffset", None, true),
            ("startOffset", None, true),
            ("textLength", Some("text"), false),
            ("x1", Some("line"), true),
            ("x2", Some("line"), true),
            ("y1", Some("line"), true),
            ("y2", Some("line"), true),
        ];
        for &(attr, element, negative) in cases {
            let grammar = real(attr, element, "<length-percentage> | <number>");
            valid(grammar, &["3", "0", "10px", "50%", "1e1"]);
            if negative {
                valid(grammar, &["-5", "-1px"]);
            }
            invalid(grammar, &["auto", "3 4", "10deg", ""]);
        }
        let ref_x = real(
            "refX",
            Some("marker"),
            "<length-percentage> | <number> | left | center | right",
        );
        valid(ref_x, &["left", "CENTER", "5", "50%"]);
        invalid(ref_x, &["top", "left 5"]);
        let ref_y = real(
            "refY",
            Some("marker"),
            "<length-percentage> | <number> | top | center | bottom",
        );
        valid(ref_y, &["top", "bottom", "5", "50%"]);
        invalid(ref_y, &["left", "top 5"]);
    }

    // Claim: refX "<length> | left | center | right" and refY "<length> |
    // top | center | bottom", "Similar to the matching attributes on
    // ‘marker’." and "Keyword values have the same meaning as for the
    // ‘refX’ and ‘refY’ attributes on the ‘marker’ element, resolving to 0%,
    // 50%, or 100% in the applicable direction."
    // https://svgwg.org/svg2-draft/struct.html#SymbolElementRefXAttribute
    // The symbol attributes are read as the marker grammar, so a percentage
    // is accepted.
    #[test]
    fn symbol_reference_point() {
        let ref_x = real("refX", Some("symbol"), "<length> | left | center | right");
        valid(ref_x, &["5", "5px", "50%", "right"]);
        invalid(ref_x, &["top", "auto"]);
        let ref_y = real("refY", Some("symbol"), "<length> | top | center | bottom");
        valid(ref_y, &["5", "5px", "50%", "bottom"]);
        invalid(ref_y, &["left", "auto"]);
    }

    // Claim: "Name: width, height, inline-size, block-size Value: auto |
    // <box-size>" https://drafts.csswg.org/css-sizing-3/#propdef-width,
    // extended by "Name: block-size, inline-size, width, height, ... New
    // values: <anchor-size()>"
    // https://drafts.csswg.org/css-anchor-position-1/#anchor-size-fn, with "<box-size> =
    // <length-percentage [0,∞]> | stretch | contain | min-content |
    // max-content | fit-content | stretch(<length-percentage [0,∞]>) |
    // fit-content(<length-percentage [0,∞]>) | <calc-size()>"
    // https://drafts.csswg.org/css-sizing-4/#typedef-box-size
    // Catalog grammars "auto | <length-percentage>" predate <box-size>; the
    // current grammar is accepted as well.
    #[test]
    fn sizing_keywords() {
        for (attr, quoted) in [
            ("width", "auto | <length-percentage>"),
            ("height", "auto | <length-percentage>"),
            ("inline-size", "auto | <length-percentage>"),
            ("block-size", "auto | <box-size> | <anchor-size()>"),
            ("min-width", "auto | <box-size> | <anchor-size()>"),
        ] {
            let element = matches!(attr, "width" | "height").then_some("mask");
            let grammar = real(attr, element, quoted);
            valid(
                grammar,
                &[
                    "max-content",
                    "MIN-CONTENT",
                    "fit-content",
                    "stretch",
                    "contain",
                    "10px",
                    "fit-content(10px)",
                    "anchor-size(width)",
                ],
            );
            invalid(grammar, &["wide", "max-content 10px", "none"]);
        }
        let max = real("max-width", None, "none | <box-size> | <anchor-size()>");
        valid(max, &["none", "max-content", "50%", "0"]);
        invalid(max, &["auto", "wide", "-1px"]);
        let reason = check("-1px", "auto | <box-size>").unwrap_or_default();
        assert!(reason.contains("range"), "{reason}");
        let margin = real(
            "margin-top",
            None,
            "<length-percentage> | auto | <anchor-size()>",
        );
        valid(margin, &["auto", "-5px", "10%", "anchor-size(height)"]);
        invalid(margin, &["max-content", "none"]);
    }

    // Claim: "Value: normal | <length-percentage>" for letter-spacing and
    // word-spacing https://drafts.csswg.org/css-text-4/#propdef-letter-spacing
    // https://drafts.csswg.org/css-text-4/#propdef-word-spacing
    // Catalog grammars "normal | <length>" come from css-text-3.
    #[test]
    fn spacing() {
        for attr in ["letter-spacing", "word-spacing"] {
            let grammar = real(attr, None, "normal | <length>");
            valid(grammar, &["normal", "0.1em", "-1px", "2", "10%"]);
            invalid(grammar, &["wide", "normal 1px", "1px 2px"]);
        }
    }

    // Claim: "Value: none | [ ex-height | cap-height | ch-width | ic-width |
    // ic-height ]? [ from-font | <number [0,∞]> ]"
    // https://drafts.csswg.org/css-fonts-5/#propdef-font-size-adjust
    // The catalog grammar "none | <number>" comes from css-fonts-3.
    #[test]
    fn font_size_adjust() {
        let grammar = real("font-size-adjust", None, "none | <number>");
        valid(
            grammar,
            &[
                "none",
                "0.5",
                "ex-height 0.5",
                "cap-height from-font",
                "from-font",
                "-1",
            ],
        );
        invalid(
            grammar,
            &["ex-height", "big", "none 0.5", "0.5 ex-height", "50%"],
        );
    }

    // Claim: "Value: [ <length-percentage> | <line-width> ]#"
    // https://drafts.csswg.org/fill-stroke-3/#propdef-stroke-width and
    // "<line-width> = <length [0,∞]> | hairline | thin | medium | thick"
    // https://drafts.csswg.org/css-borders-4/#typedef-line-width
    // The catalog grammar "<length-percentage> | <number>" comes from SVG 2.
    #[test]
    fn stroke_width() {
        let grammar = real("stroke-width", None, "<length-percentage> | <number>");
        valid(
            grammar,
            &["1", "2px", "50%", "thin", "THICK", "hairline", "1px, 2px"],
        );
        invalid(grammar, &["auto", "1px 2px", "thin thick", "1px,"]);
    }

    // Claim: "User agents may additionally support the keyword baseline as
    // computing to 0 if is necessary for them to support legacy SVG
    // content." https://drafts.csswg.org/css-inline-3/#baseline-shift-svg-legacy
    // and "Value: baseline | sub | super | <percentage> | <length> |
    // inherit" https://www.w3.org/TR/SVG11/text.html#BaselineShiftProperty
    // The SVG 2 sentence that the value "was removed" is a non-normative
    // annotation (https://svgwg.org/svg2-draft/text.html#BaselineShiftProperty),
    // so `baseline` is accepted.
    #[test]
    fn baseline_shift_legacy_baseline() {
        let grammar = real(
            "baseline-shift",
            None,
            "<length-percentage> | sub | super | top | center | bottom",
        );
        valid(grammar, &["baseline", "BASELINE", " baseline ", "0"]);
        invalid(grammar, &["baseline 0", "baseline sub", "baselines"]);
    }

    // Claim: "Value: <number [0,∞]> | <length [0,∞]>" for tab-size
    // https://drafts.csswg.org/css-text-3/#tab-size-property, "Value: normal
    // | <number [0,∞]> | <length-percentage [0,∞]>" for line-height
    // https://drafts.csswg.org/css-inline-3/#propdef-line-height (SVG 2
    // defers: "the normative definition of the line-height property is in
    // the CSS 2.1 specification"
    // https://svgwg.org/svg2-draft/text.html#LineHeightProperty, where
    // "Negative values are illegal."
    // https://www.w3.org/TR/CSS21/visudet.html#propdef-line-height), and
    // "<rect()> | auto" for clip
    // https://drafts.csswg.org/css-masking-1/#propdef-clip
    // The catalog grammars for tab-size and line-height drop the ranges.
    #[test]
    fn other_leaf_grammars() {
        let tab = real("tab-size", None, "<number> | <length>");
        valid(tab, &["4", "2em", "0", "0px"]);
        invalid(tab, &["50%", "auto", "4 4", "-1", "-4px"]);
        let line = real(
            "line-height",
            None,
            "normal | <number> | <length-percentage>",
        );
        valid(line, &["normal", "1.2", "120%", "14px", "0", "calc(-1px)"]);
        invalid(line, &["auto", "1.2 1.2", "single", "-1", "-2px", "-10%"]);
        let clip = real("clip", None, "<rect()> | auto");
        valid(
            clip,
            &["auto", "rect(1px, 2px, 3px, 4px)", "rect(1px 2px 3px 4px)"],
        );
        invalid(clip, &["none", "rect", "auto auto", "1px 2px 3px 4px"]);
    }

    // Claim: "To parse something according to a CSS grammar (aka simply
    // parse) given an input and a CSS grammar production: Normalize input,
    // and set input to the result. Parse a list of component values from
    // input, and let result be the return value. Attempt to match result
    // against grammar. If this is successful, return the matched result;
    // otherwise, return failure."
    // https://drafts.csswg.org/css-syntax-3/#parse-grammar
    // and "Unless otherwise specified, white space and/or comments may
    // appear before, after, and/or between components combined using the
    // above combinators and multipliers."
    // https://drafts.csswg.org/css-values-4/#component-whitespace
    #[test]
    fn values_are_parsed_as_component_values() {
        let grammar = real("width", Some("mask"), "auto | <length-percentage>");
        valid(
            grammar,
            &["  auto  ", "/* c */ 5px", "\t10\n", "\\61uto", "5px/**/"],
        );
        invalid(grammar, &["auto;", "(5px)", "[auto]", "5px)", "{}"]);
    }

    // Claim: "For example, a value of '10\px/**/' would successfully parse
    // as '10px' in the ‘x’ presentation attribute of the ‘rect’ element."
    // https://svgwg.org/svg2-draft/types.html#attribute-css-value
    #[test]
    fn escapes_and_comments_in_presentation_attributes() {
        let grammar = real("width", Some("mask"), "auto | <length-percentage>");
        valid(grammar, &["10\\px/**/"]);
        invalid(grammar, &["10\\qx"]);
    }

    // Claim: "If a property value contains one or more arbitrary
    // substitution functions, and all of those functions are themselves
    // syntactically valid according to their argument grammars, the entire
    // value’s grammar must be assumed to be valid at parse time."
    // https://drafts.csswg.org/css-values-5/#resolve-property
    // (the check accepts such a value without checking the functions'
    // arguments, which only accepts more)
    // and "A math function represents a numeric value, one of: <length>,
    // <frequency>, <angle>, <time>, <flex>, <resolution>, <percentage>,
    // <number>, <integer> ...or the <length-percentage>/etc mixed types, and
    // can be used wherever such a value would be valid."
    // https://drafts.csswg.org/css-values-4/#math
    // Accept-only: the stub accepts these too.
    #[test]
    fn functions_and_substitutions_accept() {
        let width = real("width", Some("mask"), "auto | <length-percentage>");
        valid(
            width,
            &[
                "calc(100% - 10px)",
                "var(--w)",
                "env(safe-area-inset-left)",
                "attr(w px)",
                "min(1px, 2px)",
                "foo var(--x)",
            ],
        );
        let x = real(
            "x",
            Some("text"),
            "[ [ <length-percentage> | <number> ]+ ]#",
        );
        valid(x, &["calc(1px + 2px) 5", "var(--a), 1"]);
        let order = real(
            "paint-order",
            None,
            "normal | [ fill || stroke || markers ]",
        );
        valid(order, &["var(--p) fill", "fill if(style(--x: 1): stroke)"]);
    }

    // Claim: "Property value ranges, which represent the same pattern of
    // values as a property bearing the same name. These are written as the
    // property name, surrounded by single quotes, between < and >" and
    // "Other non-terminals. These are written as the name of the
    // non-terminal between < and >"
    // https://drafts.csswg.org/css-values-4/#component-types
    // Non-terminals and property value ranges expand through the catalog's
    // `@webref/css` definitions or the SVG-local types, so each usage checks
    // its value: a valid value matches and garbage does not.
    #[test]
    fn referenced_types_and_properties_expand() {
        let cases: &[(&str, Option<&str>, &str)] = &[
            ("alignment-baseline", None, "middle"),
            ("background", None, "red url(a.png) no-repeat"),
            ("baseFrequency", Some("feTurbulence"), "0.05,0.1"),
            ("border-style", None, "solid dashed"),
            ("clip-path", None, "url(#c)"),
            ("color-profile", None, "sRGB"),
            ("color-interpolation", None, "in oklab"),
            ("content", None, "'x' / 'alt'"),
            ("cursor", None, "url(c.svg), pointer"),
            ("dominant-baseline", None, "hanging"),
            ("filter", None, "url(#f) blur(2px)"),
            ("font", None, "italic bold 12px/30px Georgia, serif"),
            ("font-family", None, "Arial, 'Helvetica Neue', sans-serif"),
            ("font-feature-settings", None, "'liga' 1"),
            ("font-size", None, "math"),
            ("font-variant", None, "small-caps"),
            ("font-variant-ligatures", None, "common-ligatures"),
            ("in", Some("feGaussianBlur"), "SourceGraphic"),
            ("in2", Some("feBlend"), "blurred"),
            ("kernelUnitLength", Some("feConvolveMatrix"), "1 1"),
            ("keySplines", Some("animate"), "0 0 1 1; 0.5 0 0.5 1"),
            ("mask", None, "url(#m)"),
            ("mode", Some("feBlend"), "multiply"),
            ("object-position", None, "right 10px top 5px"),
            ("order", Some("feConvolveMatrix"), "3 3"),
            ("outline", None, "1px solid red"),
            ("points", Some("polygon"), "0,0 10,10 20,0"),
            ("points", Some("polyline"), "0,0 10,10"),
            ("preserveAspectRatio", Some("svg"), "xMidYMid meet"),
            ("radius", Some("feMorphology"), "2 1"),
            ("result", Some("feGaussianBlur"), "blur"),
            ("shape-outside", None, "circle(50%)"),
            ("stdDeviation", Some("feGaussianBlur"), "2,3"),
            ("stdDeviation", Some("feDropShadow"), "2"),
            ("target", Some("a"), "_blank"),
            ("text-decoration", None, "underline red"),
            ("vertical-align", None, "middle"),
            ("viewBox", Some("svg"), "0 0 100 100"),
            ("white-space", None, "pre"),
            ("will-change", None, "transform"),
        ];
        for &(attr, element, value) in cases {
            let def =
                svg_data::attribute(attr).unwrap_or_else(|| panic!("{attr} is in the catalog"));
            let values =
                element.map_or(&def.values, |element| def.values_for_element(Some(element)));
            let AttributeValues::CssGrammar { grammar, .. } = values else {
                panic!("{attr} on {element:?} is not a CSS grammar: {values:?}");
            };
            valid(grammar, &[value]);
            invalid(grammar, &["#!garbage)"]);
        }
    }

    // Claim: "Value: <clip-source> | [ <basic-shape> || <geometry-box> ] |
    // none" https://drafts.csswg.org/css-masking-1/#the-clip-path
    // A function matches a notation of the same name, and its arguments
    // match that function's own definition.
    #[test]
    fn clip_path_values_and_shape_arguments_are_checked() {
        let clip = real(
            "clip-path",
            None,
            "<clip-source> | [ <basic-shape> || <geometry-box> ] | none",
        );
        valid(
            clip,
            &[
                "url(#c)",
                "url('#c')",
                "circle(50%)",
                "circle(closest-side at 10px 20px)",
                "circle()",
                "ellipse(10px 20px at center)",
                "inset(1px) fill-box",
                "inset(10px round 5px)",
                "inset(1px 2px 3px 4px)",
                "polygon(0 0, 10px 0, 5px 10px)",
                "polygon(evenodd, 0 0, 10px 0, 5px 10px)",
                "path('M0 0 L10 10')",
                "xywh(0 0 10px 10px)",
                "shape(from 0px 0px, curve to 10px 10px with 5px 5px)",
                "shape(from 0px 0px, smooth to 10px 10px with 5px 5px)",
                "rect(1px 2px 3px 4px round 2px) stroke-box",
                "border-box",
                "none",
            ],
        );
        invalid(
            clip,
            &[
                "url(#clip) red",
                "circle(50%) red",
                "blur(2px)",
                "banana",
                "circle(banana)",
                "circle(10px at)",
                "polygon(0 0 0)",
                "shape(from 0px 0px, smooth to 10px 10px with 5px 5px / 1px 1px)",
                "inset()",
            ],
        );
    }

    // Claim: "Value: none | <filter-value-list>"
    // https://drafts.csswg.org/filter-effects-1/#FilterProperty
    // A numeric slot takes a math function, never another function.
    #[test]
    fn filter_values_and_function_arguments_are_checked() {
        let filter = real("filter", None, "none | <filter-value-list>");
        valid(
            filter,
            &[
                "url(#f)",
                "blur(2px)",
                "blur(2px) url(#f) drop-shadow(1px 1px red)",
                "none",
                "hue-rotate(90deg)",
                "hue-rotate(0)",
                "saturate(50%) brightness(1.2) contrast(200%)",
                "grayscale() opacity(0.5) sepia(1) invert(100%)",
                "drop-shadow(2px 2px 2px rgb(0 0 0 / 50%))",
                "drop-shadow(red 1px 1px)",
                "blur()",
                "blur(calc(1px + 2px))",
                "opacity(sibling-index())",
            ],
        );
        invalid(
            filter,
            &[
                "blur(2px) junk",
                "red",
                "circle(1px)",
                "blur(red)",
                "blur(1px 2px)",
                "hue-rotate(red)",
                "drop-shadow(red)",
                "blur(linear-gradient(red, blue))",
            ],
        );
    }

    // Claim: "Value: <mask-layer>#" https://drafts.csswg.org/css-masking-1/#the-mask
    #[test]
    fn mask_values_are_checked() {
        let mask = real("mask", None, "<mask-layer>#");
        valid(
            mask,
            &[
                "url(#m)",
                "url(#m) no-repeat",
                "url(#a), url(#b) luminance",
                "linear-gradient(red, blue)",
                "linear-gradient(to right, red 10%, blue) luminance",
                "radial-gradient(circle, black, transparent)",
                "url(#m) center / 50% no-repeat",
            ],
        );
        invalid(
            mask,
            &[
                "url(#m) banana",
                "red",
                "linear-gradient(to nowhere, red, blue)",
                "url(#m) center / banana",
            ],
        );
    }

    // Claim: "<control-point> = <number> ,? <number> ,? <number> ,? <number>"
    // https://svgwg.org/specs/animations/#DataTypeControlPoint, for keySplines
    // alone: CSS Shapes curves keep their own <control-point>.
    #[test]
    fn key_splines_take_four_numbers() {
        let splines = real(
            "keySplines",
            Some("animate"),
            "<control-point> [; <control-point>]* ;?",
        );
        valid(splines, &["0 0 1 1", "0,0,1,1; .5 0 .5 1;"]);
        invalid(splines, &["0 0 1", "0px 0 1 1", "0 0 1 1 1"]);
    }

    // Claim: "Within a relative color syntax rgb() or rgba() function, the
    // allowed component keywords are: r, g, and b are all <number>s" and the
    // matching lists for the other functions, with color() keywords chosen by
    // its color space.
    // https://drafts.csswg.org/css-color-5/#relative-colors
    #[test]
    fn relative_colors_take_their_component_keywords() {
        let mask = real("mask", None, "<mask-layer>#");
        valid(
            mask,
            &[
                "linear-gradient(rgb(from red r g b), blue)",
                "linear-gradient(rgb(from red r g b / alpha), blue)",
                "linear-gradient(hsl(from red h s l / 50%), blue)",
                "linear-gradient(hwb(from red h w b), blue)",
                "linear-gradient(oklch(from red l c h), blue)",
                "linear-gradient(lab(from red l a b), blue)",
                "linear-gradient(color(from red srgb r g b), blue)",
                "linear-gradient(color(from red xyz x y z), blue)",
                "linear-gradient(color(from red --profile cyan magenta yellow black), blue)",
                "linear-gradient(alpha(from red / alpha), blue)",
                "linear-gradient(rgb(from rgb(from red r g b) r g b), blue)",
            ],
        );
        invalid(
            mask,
            &[
                "linear-gradient(rgb(from red h s l), blue)",
                "linear-gradient(rgb(r g b), blue)",
                "linear-gradient(color(from red srgb x y z), blue)",
                "linear-gradient(oklch(from red l c banana), blue)",
            ],
        );
    }

    // Claim: "Value: [<cursor-image>,]* <cursor-predefined>"
    // https://drafts.csswg.org/css-ui-4/#cursor
    #[test]
    fn cursor_values_are_checked() {
        let cursor = real("cursor", None, "[<cursor-image>,]* <cursor-predefined>");
        valid(
            cursor,
            &["pointer", "url(c.cur) 2 3, pointer", "url(c.svg), auto"],
        );
        invalid(cursor, &["url(c.svg)", "pointer, url(c.svg)", "hand"]);
    }

    // Claim: "Replace all instances of <length> in grammar with [<length> |
    // <number>]", likewise for <length-percentage> and <angle>, where grammar
    // is "the grammar given in the attribute definition table's Value
    // column".
    // https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value
    // The types a named type or a function's arguments expand to are not in
    // that text, so they keep CSS parsing.
    #[test]
    fn unitless_numbers_replace_only_value_column_types() {
        let filter = real("filter", None, "none | <filter-value-list>");
        valid(
            filter,
            &["hue-rotate(90deg)", "blur(0)", "drop-shadow(0 0 red)"],
        );
        invalid(
            filter,
            &["hue-rotate(90)", "blur(2)", "drop-shadow(1 1 red)"],
        );
        let clip = real(
            "clip-path",
            None,
            "<clip-source> | [ <basic-shape> || <geometry-box> ] | none",
        );
        valid(clip, &["circle(5px)", "inset(0)"]);
        invalid(clip, &["circle(5)", "inset(1)"]);
        let mask = real("mask", None, "<mask-layer>#");
        valid(mask, &["url(#m) 10px 20px", "url(#m) 0 0"]);
        invalid(mask, &["url(#m) 10 20"]);
        let width = real("width", Some("mask"), "auto | <length-percentage>");
        valid(width, &["100"]);
        let clip_rect = real("clip", None, "<rect()> | auto");
        invalid(clip_rect, &["rect(0, 50, 50, 0)"]);
    }

    // Claim: "When a <length> is used in an SVG presentation attribute, the
    // syntax must match the following pattern: length ::= number ("em" | "ex"
    // | "px" | "in" | "cm" | "mm" | "pt" | "pc" | "%")?"
    // https://www.w3.org/TR/SVG11/types.html#DataTypeLength
    // Claim: "When an <angle> is used in an SVG presentation attribute, the
    // syntax must match the following pattern: angle ::= number ("deg" |
    // "grad" | "rad")?" https://www.w3.org/TR/SVG11/types.html#DataTypeAngle
    #[test]
    fn svg_1_1_admits_unitless_numbers_in_the_whole_value() {
        let svg11 = |value: &str, grammar: &str| {
            check_css_grammar(value, grammar, &NO_GRAPH, UnitlessScope::WholeValue)
        };
        let cases = [
            (
                real("filter", None, "none | <filter-value-list>"),
                &["hue-rotate(90)", "blur(2)", "drop-shadow(1 1 red)"][..],
            ),
            (
                real("clip", None, "<rect()> | auto"),
                &["rect(0, 50, 50, 0)"],
            ),
            (real("mask", None, "<mask-layer>#"), &["url(#m) 10 20"]),
        ];
        for (grammar, values) in cases {
            for value in values {
                assert_eq!(svg11(value, grammar), None, "{value:?} in SVG 1.1");
            }
        }
        assert!(
            svg11(
                "hue-rotate(red)",
                real("filter", None, "none | <filter-value-list>")
            )
            .is_some()
        );
    }

    // Claim: "<circle()> = circle( <radial-size>? [ at <position> ]? )" and
    // "Two <length-percentage> values are invalid."
    // https://drafts.csswg.org/css-shapes-1/#funcdef-basic-shape-circle
    #[test]
    fn circle_takes_one_radius_length() {
        let clip = real(
            "clip-path",
            None,
            "<clip-source> | [ <basic-shape> || <geometry-box> ] | none",
        );
        valid(
            clip,
            &[
                "circle(1px)",
                "circle(closest-side farthest-side)",
                "circle(closest-side at 1px 2px)",
                "circle(at center)",
                "ellipse(1px 2px)",
            ],
        );
        invalid(
            clip,
            &[
                "circle(1px 2px)",
                "circle(-1px)",
                "circle(closest-side 1px)",
            ],
        );
    }

    // Claim: "Value: auto | sRGB | <name> | <iri> | inherit" and "name ::=
    // [^,()#x20#x9#xD#xA] /* any char except ",", "(", ")" or wsp */"
    // https://www.w3.org/TR/SVG11/color.html#ColorProfileProperty
    // https://www.w3.org/TR/SVG11/types.html#DataTypeName
    #[test]
    fn color_profile_is_a_name_or_url() {
        let profile = real("color-profile", None, "auto | sRGB | <name> | <uri>");
        valid(
            profile,
            &[
                "auto",
                "sRGB",
                "inherit",
                "my-profile",
                "Profile_2.icc",
                "#p",
                " url(#p) ",
                "url('p.icc')",
                "var(--p)",
            ],
        );
        invalid(profile, &["", "two names", "a,b", "url(#p) extra", "f(x)"]);
    }

    // The parser takes any CSS value for clip-path, so an invalid one is a
    // lint diagnostic on that attribute alone.
    #[test]
    fn invalid_clip_path_is_reported_on_its_attribute() {
        let messages: Vec<String> = crate::lint(
            br#"<svg xmlns="http://www.w3.org/2000/svg"><clipPath id="c"/><rect clip-path="url(#c) red" filter="blur(2px)"/></svg>"#,
        )
        .into_iter()
        .filter(|diagnostic| diagnostic.code == crate::types::DiagnosticCode::InvalidAttributeValue)
        .map(|diagnostic| diagnostic.message)
        .collect();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains("clip-path"), "{messages:?}");
    }

    // Claim: "The SVG 2 specification defines attribute values ... By
    // reference to an EBNF symbol defined in this or another specification
    // [xml]. For external definitions, this is indicated by [EBNF] appearing
    // in the Value column." https://svgwg.org/svg2-draft/types.html#syntax
    // and the SMIL-style grammars quote literal values ("media",
    // "indefinite") and name the Clock-value production
    // (https://svgwg.org/specs/animations/#ClockValueSyntax). Each is checked
    // by its own syntax.
    #[test]
    fn smil_and_xml_grammars_are_checked() {
        type GrammarCase = (
            &'static str,
            Option<&'static str>,
            &'static [&'static str],
            &'static [&'static str],
        );
        let cases: &[GrammarCase] = &[
            (
                "attributeName",
                Some("animate"),
                &["fill", "xlink:href", "stroke-width", "_x.y"],
                &["not a name", "1fill", ""],
            ),
            (
                "dur",
                Some("animate"),
                &[
                    "2s",
                    "0.5ms",
                    "1.5h",
                    "3min",
                    "12",
                    "02:30:03",
                    "50:00:10.25",
                    "02:33",
                    "00:10.5",
                    "indefinite",
                    "media",
                ],
                &["2 s", "2sec", "-1s", "01:60", "02:61:00", "forever"],
            ),
            (
                "max",
                Some("animate"),
                &["5s", "media"],
                &["indefinite", "5 s"],
            ),
            ("min", Some("animate"), &["0", "media"], &["indefinite"]),
            (
                "repeatDur",
                Some("animate"),
                &["01:30", "indefinite"],
                &["media", "1:2:3:4"],
            ),
            (
                "target",
                Some("a"),
                &["_blank", "_self", "frame1", "my-window"],
                &["two words", "1frame"],
            ),
        ];
        for &(attr, element, accepted, rejected) in cases {
            let def =
                svg_data::attribute(attr).unwrap_or_else(|| panic!("{attr} is in the catalog"));
            let AttributeValues::CssGrammar { grammar, .. } =
                element.map_or(&def.values, |element| def.values_for_element(Some(element)))
            else {
                panic!("{attr} is not a CSS grammar");
            };
            valid(grammar, accepted);
            invalid(grammar, rejected);
        }
    }

    // Claim: "Resolved that "d will become a presentation attribute (no name
    // change) with path data string as value"" and "The contents of the
    // <string> value must match the svg-path EBNF grammar defined below"
    // https://svgwg.org/svg2-draft/paths.html#TheDProperty
    // The `d` attribute holds path data, not a CSS <string>; path data has
    // its own check, so this one accepts (accept-only).
    #[test]
    fn path_data_attribute_accepts() {
        let grammar = real("d", None, "none | <string>");
        valid(grammar, &["M0 0L10 10", "m 1,2 3,4z", "none", "'M0 0'"]);
    }

    // Claim: "values is a list of 20 matrix values (a00 a01 a02 a03 a04 a10
    // a11 ... a34), separated by whitespace and/or a comma."
    // https://drafts.csswg.org/filter-effects-1/#element-attrdef-fecolormatrix-values
    // and "When type="table", the list of <number> s v0,v1,...vn, separated
    // by white space and/or a comma, which define the lookup table. An empty
    // list results in an identity transfer function."
    // https://drafts.csswg.org/filter-effects-1/#element-attrdef-fecomponenttransfer-tablevalues
    // and "The list of <number> s that make up the kernel matrix for the
    // convolution. Values are separated by space characters and/or a comma."
    // https://drafts.csswg.org/filter-effects-1/#element-attrdef-feconvolvematrix-kernelmatrix
    // The catalog's "<number>+" would reject the commas and the empty list,
    // so these attributes are accepted (accept-only).
    #[test]
    fn filter_number_lists_accept() {
        let cases: &[(&str, Option<&str>)] = &[
            ("kernelMatrix", Some("feConvolveMatrix")),
            ("tableValues", Some("feComponentTransfer")),
            ("values", Some("feColorMatrix")),
        ];
        for &(attr, element) in cases {
            let grammar = real(attr, element, "<number>+");
            valid(grammar, &["1 0 0, 0 1 0", "1,0,0", "", "0.5"]);
        }
    }

    // Helper test, not a spec claim: an invalid value's reason names the
    // grammar it failed.
    #[test]
    fn reason_names_the_grammar() {
        let reason = check("wide", "normal | <length>").unwrap_or_default();
        assert!(reason.contains("normal | <length>"), "{reason}");
    }
}
