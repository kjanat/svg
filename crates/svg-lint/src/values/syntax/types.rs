//! Classifying component values as CSS numeric data types.
//!
//! [`match_numeric`] answers whether one component value is a literal of a
//! CSS Values 4 numeric data type (§5 "Numeric Data Types" to §7 "Other
//! Quantities", <https://drafts.csswg.org/css-values-4/#numeric-types>).
//! [`match_value`] does the same for a whole attribute value.
//!
//! The answer is three-valued. Any function is [`Match::Unknown`]: math
//! functions (§10) and the wider family of numeric functions (§9.1) resolve
//! to a numeric value whose type is only known after type checking, and an
//! arbitrary substitution function makes the whole value valid at parse time.
//!
//! [`UnitlessMode::PresentationAttribute`] applies the SVG 2 rule for
//! presentation attributes
//! (<https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value>):
//! a `<number>` is accepted wherever the property grammar has `<length>`,
//! `<length-percentage>` or `<angle>`.
//!
//! Only the type is classified. Range restrictions (non-negative lengths,
//! non-negative resolutions) are left to the checks.

use super::{
    cascade::is_css_wide_keyword_value,
    notation::contains_arbitrary_substitution,
    parser::{ComponentValue, parse_component_values},
    tokenizer::{Number, Token},
    units::{UnitCategory, unit_category},
};

/// Whether a value matches a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    /// The value is a literal of the type.
    Yes,
    /// The value cannot be of the type.
    No,
    /// The value cannot be decided here (a function, a CSS-wide keyword, an
    /// arbitrary substitution, a nesting limit); a check must accept it.
    Unknown,
}

impl Match {
    /// The match of a value that may be either type: `Yes` if either is,
    /// else `Unknown` if either is, else `No`.
    #[must_use]
    pub const fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Yes, _) | (_, Self::Yes) => Self::Yes,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::No, Self::No) => Self::No,
        }
    }

    /// Whether the value is certainly not of the type.
    #[must_use]
    pub const fn is_no(self) -> bool {
        matches!(self, Self::No)
    }
}

/// A CSS numeric data type, or a mixed `*-percentage` type (CSS Values 4
/// §5.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericType {
    /// `<number>`.
    Number,
    /// `<integer>`.
    Integer,
    /// `<percentage>`.
    Percentage,
    /// `<length>`.
    Length,
    /// `<length-percentage>`.
    LengthPercentage,
    /// `<angle>`.
    Angle,
    /// `<angle-percentage>`.
    AnglePercentage,
    /// `<time>`.
    Time,
    /// `<time-percentage>`.
    TimePercentage,
    /// `<frequency>`.
    Frequency,
    /// `<frequency-percentage>`.
    FrequencyPercentage,
    /// `<resolution>`.
    Resolution,
    /// `<flex>`.
    Flex,
}

/// How a bare `<number>` is treated where a dimension is expected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnitlessMode {
    /// CSS property value parsing: only a zero stands for a `<length>`; a
    /// bare zero `<angle>` is [`Match::Unknown`], as legacy uses allow it.
    #[default]
    Css,
    /// SVG 2 presentation attribute parsing: any `<number>` is accepted for
    /// `<length>`, `<length-percentage>` and `<angle>` (and so for
    /// `<angle-percentage>`).
    PresentationAttribute,
}

/// Whether one component value is a literal of `ty`.
#[must_use]
pub fn match_numeric(value: &ComponentValue<'_>, ty: NumericType, mode: UnitlessMode) -> Match {
    match value {
        ComponentValue::Function(_) => Match::Unknown,
        ComponentValue::Token(token) => match_token(token, ty, mode),
        ComponentValue::Block(_) => Match::No,
    }
}

/// [`match_numeric`] for a preserved token.
fn match_token(token: &Token<'_>, ty: NumericType, mode: UnitlessMode) -> Match {
    match token {
        Token::Number(number) => match_number(number, ty, mode),
        Token::Percentage(_) => Match::from(ty.allows_percentage()),
        Token::Dimension { unit, .. } => {
            Match::from(unit_category(unit).is_some_and(|category| ty.has_unit(category)))
        }
        _ => Match::No,
    }
}

/// [`match_numeric`] for a `<number-token>`.
fn match_number(number: &Number, ty: NumericType, mode: UnitlessMode) -> Match {
    let presentation = mode == UnitlessMode::PresentationAttribute;
    match ty {
        NumericType::Number => Match::Yes,
        NumericType::Integer => Match::from(number.is_integer()),
        NumericType::Length | NumericType::LengthPercentage => {
            Match::from(presentation || number.value == 0.0)
        }
        NumericType::Angle | NumericType::AnglePercentage if presentation => Match::Yes,
        // "some uses of <angle> allow a bare 0 to mean 0deg"
        NumericType::Angle | NumericType::AnglePercentage if number.value == 0.0 => Match::Unknown,
        _ => Match::No,
    }
}

impl From<bool> for Match {
    fn from(matches: bool) -> Self {
        if matches { Self::Yes } else { Self::No }
    }
}

impl NumericType {
    /// Whether a `<percentage>` is of this type.
    const fn allows_percentage(self) -> bool {
        matches!(
            self,
            Self::Percentage
                | Self::LengthPercentage
                | Self::AnglePercentage
                | Self::TimePercentage
                | Self::FrequencyPercentage
        )
    }

    /// Whether a dimension in `category` is of this type.
    fn has_unit(self, category: UnitCategory) -> bool {
        let expected = match self {
            Self::Number | Self::Integer | Self::Percentage => return false,
            Self::Length | Self::LengthPercentage => UnitCategory::Length,
            Self::Angle | Self::AnglePercentage => UnitCategory::Angle,
            Self::Time | Self::TimePercentage => UnitCategory::Time,
            Self::Frequency | Self::FrequencyPercentage => UnitCategory::Frequency,
            Self::Resolution => UnitCategory::Resolution,
            Self::Flex => UnitCategory::Flex,
        };
        category == expected
    }
}

/// Whether a whole value, parsed as CSS component values, is a single
/// literal of `ty` with only whitespace and comments around it.
///
/// A CSS-wide keyword, an arbitrary substitution function anywhere in the
/// value, or a nesting limit makes the answer [`Match::Unknown`]; so does a
/// function, as in [`match_numeric`].
#[must_use]
pub fn match_value(value: &str, ty: NumericType, mode: UnitlessMode) -> Match {
    let list = parse_component_values(value);
    if list.depth_limited
        || contains_arbitrary_substitution(&list.values)
        || is_css_wide_keyword_value(value)
    {
        return Match::Unknown;
    }
    let mut significant = list.significant();
    match (significant.next(), significant.next()) {
        (Some(only), None) => match_numeric(only, ty, mode),
        _ => Match::No,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css(value: &str, ty: NumericType) -> Match {
        match_value(value, ty, UnitlessMode::Css)
    }

    fn attr(value: &str, ty: NumericType) -> Match {
        match_value(value, ty, UnitlessMode::PresentationAttribute)
    }

    fn assert_all(values: &[&str], ty: NumericType, mode: UnitlessMode, expected: Match) {
        for value in values {
            assert_eq!(
                match_value(value, ty, mode),
                expected,
                "{value:?} as {ty:?} in {mode:?}"
            );
        }
    }

    use Match::{No, Unknown, Yes};
    use NumericType as T;
    use UnitlessMode::{Css, PresentationAttribute as Pres};

    // Helper test, not a spec claim: the `Match::or` and `Match::is_no`
    // combinators the checks use to merge alternatives.
    #[test]
    fn match_combinators() {
        assert_eq!(Yes.or(No), Yes);
        assert_eq!(No.or(Yes), Yes);
        assert_eq!(No.or(Unknown), Unknown);
        assert_eq!(Unknown.or(Yes), Yes);
        assert_eq!(No.or(No), No);
        assert!(No.is_no());
        assert!(!Unknown.is_no());
        assert!(!Yes.is_no());
    }

    // Claim: "When written literally, an integer is one or more decimal digits
    // 0 through 9 and corresponds to a subset of the <number-token> production
    // in the CSS Syntax Module [CSS-SYNTAX-3]. The first digit of an integer
    // may be immediately preceded by - or + to indicate the integer’s sign."
    // https://drafts.csswg.org/css-values-4/#integers
    #[test]
    fn integers() {
        assert_all(&["0", "7", "-3", "+12", " 4 "], T::Integer, Css, Yes);
        assert_all(
            &["1.5", "1e3", "1.0", "5px", "50%", "a", "", "1 2", "- 1"],
            T::Integer,
            Css,
            No,
        );
    }

    // Claim: "When written literally, a number is either an integer, or zero
    // or more decimal digits followed by a dot (.) followed by one or more
    // decimal digits; optionally, it can be concluded by the letter “e” or “E”
    // followed by an integer indicating the base-ten exponent in scientific
    // notation. It corresponds to the <number-token> production in the CSS
    // Syntax Module [CSS-SYNTAX-3]."
    // https://drafts.csswg.org/css-values-4/#numbers
    #[test]
    fn numbers() {
        assert_all(
            &["0", "-1.5", ".5", "+.5e-3", "1E3", "\t2\n", "1/**/"],
            T::Number,
            Css,
            Yes,
        );
        assert_all(
            &["1.", "1px", "1%", "one", "1,", "#1", "'1'"],
            T::Number,
            Css,
            No,
        );
    }

    // Claim: "When written literally, a percentage consists of a number
    // immediately followed by a percent sign %."
    // https://drafts.csswg.org/css-values-4/#percentages
    #[test]
    fn percentages() {
        assert_all(&["50%", "-0.5%", "1e2%"], T::Percentage, Css, Yes);
        assert_all(&["50", "50 %", "50px", "%"], T::Percentage, Css, No);
    }

    // Claim: "Lengths refer to distance measurements and are denoted by
    // <length> in the property definitions. A length is a dimension. For zero
    // lengths the unit identifier is optional (i.e. can be syntactically
    // represented as the <number> 0)."
    // https://drafts.csswg.org/css-values-4/#lengths
    #[test]
    fn lengths() {
        assert_all(
            &[
                "1px", "-2.5em", "3Q", "4vmin", "5dvh", "6cqi", "7rlh", "0", "0.0", "-0", "1PX",
                "1\\70x",
            ],
            T::Length,
            Css,
            Yes,
        );
        assert_all(
            &["1", "-2", "1deg", "1s", "50%", "1qx", "px", "1 px", "1fr"],
            T::Length,
            Css,
            No,
        );
    }

    // Claim: "<length-percentage> Equivalent to [ <length> | <percentage> ],
    // where the <percentage> will resolve to a <length>."
    // https://drafts.csswg.org/css-values-4/#typedef-length-percentage
    #[test]
    fn length_percentages() {
        assert_all(&["1px", "50%", "0"], T::LengthPercentage, Css, Yes);
        assert_all(&["5", "1deg", "auto"], T::LengthPercentage, Css, No);
    }

    // Claim: "Angle values are <dimension>s denoted by <angle>. The angle unit
    // identifiers are: deg ... grad ... rad ... turn"
    // https://drafts.csswg.org/css-values-4/#angles
    // Claim: "Note: For legacy reasons, some uses of <angle> allow a bare 0 to
    // mean 0deg." (so a bare zero is undecidable without the context)
    // https://drafts.csswg.org/css-values-4/#angles
    #[test]
    fn angles() {
        assert_all(
            &["45deg", "1.5rad", "100grad", "0.25turn", "90DEG"],
            T::Angle,
            Css,
            Yes,
        );
        assert_all(&["0"], T::Angle, Css, Unknown);
        assert_all(&["45", "1px", "50%"], T::Angle, Css, No);
        assert_all(&["50%", "1deg"], T::AnglePercentage, Css, Yes);
        assert_all(&["1px"], T::AnglePercentage, Css, No);
    }

    // Claim: "Time values are dimensions denoted by <time>. The time unit
    // identifiers are: s Seconds. ms Milliseconds."
    // https://drafts.csswg.org/css-values-4/#time
    // Claim: "Frequency values are dimensions denoted by <frequency>."
    // https://drafts.csswg.org/css-values-4/#frequency
    // Claim: "Resolution units are dimensions denoted by <resolution>."
    // https://drafts.csswg.org/css-values-4/#resolution
    // Claim: "A flexible length or <flex> is a dimension with the fr unit"
    // https://drafts.csswg.org/css-grid-1/#fr-unit
    #[test]
    fn other_dimensions() {
        assert_all(&["1s", "250ms", "1MS"], T::Time, Css, Yes);
        assert_all(&["1", "0", "1px", "1hz"], T::Time, Css, No);
        assert_all(&["10%", "2s"], T::TimePercentage, Css, Yes);
        assert_all(&["440Hz", "1khz"], T::Frequency, Css, Yes);
        assert_all(&["440"], T::Frequency, Css, No);
        assert_all(&["10%", "1kHz"], T::FrequencyPercentage, Css, Yes);
        assert_all(&["2x", "96dpi", "1dppx", "2dpcm"], T::Resolution, Css, Yes);
        assert_all(&["2"], T::Resolution, Css, No);
        assert_all(&["1fr", "2.5FR"], T::Flex, Css, Yes);
        assert_all(&["1", "1px"], T::Flex, Css, No);
    }

    // Claim: "Replace all instances of <length> in grammar with [<length> |
    // <number>]. Replace all instances of <length-percentage> in grammar with
    // [<length-percentage> | <number>]. Replace all instances of <angle> in
    // grammar with [<angle> | <number>]."
    // https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value
    #[test]
    fn presentation_attributes_allow_unitless_lengths_and_angles() {
        assert_all(&["10", "-2.5", "1e3", "10px"], T::Length, Pres, Yes);
        assert_all(&["10", "10%"], T::LengthPercentage, Pres, Yes);
        assert_all(&["45", "0", "45deg"], T::Angle, Pres, Yes);
        assert_all(&["45"], T::AnglePercentage, Pres, Yes);
        assert_all(&["10deg", "10s", "a"], T::Length, Pres, No);
        // Only lengths and angles gain the quirk.
        assert_all(&["1"], T::Time, Pres, No);
        assert_all(&["1"], T::Percentage, Pres, No);
        assert_eq!(css("10", T::Length), No);
        assert_eq!(attr("10", T::Length), Yes);
    }

    // Claim: "A math function represents a numeric value, one of: <length>,
    // <frequency>, <angle>, <time>, <flex>, <resolution>, <percentage>,
    // <number>, <integer> ...or the <length-percentage>/etc mixed types, and
    // can be used wherever such a value would be valid."
    // https://drafts.csswg.org/css-values-4/#math
    // Claim: "Any functional notation that resolves solely to a numeric data
    // type is a numeric function."
    // https://drafts.csswg.org/css-values-4/#functional-numeric
    #[test]
    fn functions_are_unknown() {
        for ty in [
            T::Number,
            T::Integer,
            T::Length,
            T::Angle,
            T::Time,
            T::Percentage,
        ] {
            for value in [
                "calc(1px + 2px)",
                "MIN(1, 2)",
                "clamp(1px, 2vw, 3px)",
                "sibling-index()",
                "progress(1, 0, 2)",
                "foo(1)",
                "calc(1px",
            ] {
                assert_eq!(css(value, ty), Unknown, "{value:?} as {ty:?}");
            }
        }
        let list = parse_component_values("sin(1deg)");
        assert_eq!(match_numeric(&list.values[0], T::Number, Css), Unknown);
    }

    // Claim: "If a property value contains one or more arbitrary substitution
    // functions, and all of those functions are themselves syntactically
    // valid according to their argument grammars, the entire value’s grammar
    // must be assumed to be valid at parse time."
    // https://drafts.csswg.org/css-values-5/#resolve-property
    // Claim: "all properties accept the CSS-wide keywords"
    // https://drafts.csswg.org/css-values-4/#common-keywords
    #[test]
    fn substitution_and_css_wide_keywords_are_unknown() {
        assert_eq!(css("var(--w)", T::Length), Unknown);
        assert_eq!(css("1px var(--w)", T::Length), Unknown);
        assert_eq!(css("[env(x)]", T::Length), Unknown);
        assert_eq!(css("inherit", T::Length), Unknown);
        assert_eq!(css(" INITIAL ", T::Number), Unknown);
        assert_eq!(css(&"(".repeat(1000), T::Length), Unknown);
        assert_eq!(css("1px 2px", T::Length), No);
        assert_eq!(css("(1px)", T::Length), No);
        assert_eq!(css("1px;", T::Length), No);
        assert_eq!(css("'1px'", T::Length), No);
        assert_eq!(css("url(x)", T::Length), No);
    }

    // Claim: "When written literally, a dimension is a number immediately
    // followed by a unit identifier, which is an identifier. It corresponds
    // to the <dimension-token> production in the CSS Syntax Module
    // [CSS-SYNTAX-3]." https://drafts.csswg.org/css-values-4/#dimensions
    // Claim: "When written literally, a percentage consists of a number
    // immediately followed by a percent sign %. It corresponds to the
    // <percentage-token> production in the CSS Syntax Module [CSS-SYNTAX-3]."
    // https://drafts.csswg.org/css-values-4/#percentages
    // (with the <number-token> claim of `numbers` above: a block, comma,
    // hash, string or ident is none of these tokens)
    #[test]
    fn non_numeric_component_values_do_not_match() {
        let list = parse_component_values("[1px] , #1 'a' a");
        for value in list.significant() {
            assert_eq!(match_numeric(value, T::Length, Pres), No, "{value:?}");
        }
    }

    // Claim: "All properties implicitly accept a <whole-value> as their entire
    // value, just as they accept the CSS-wide keywords as their entire value."
    // https://drafts.csswg.org/css-values-5/#whole-value
    // Claim: "background-position: cycle(50px, center) 50px; is not."
    // https://drafts.csswg.org/css-values-5/#whole-value
    #[test]
    fn whole_value_is_unknown() {
        assert_eq!(css("first-valid(1px)", T::Length), Unknown);
        assert_eq!(attr(" cycle(1, 2) ", T::Length), Unknown);
        assert_eq!(css("interpolate(50%, 0: 1, 1: 2)", T::Number), Unknown);
        assert_eq!(css("cycle(50px, center) 50px", T::Length), No);
        assert_eq!(css("50px first-valid(1px)", T::Length), No);
    }
}
