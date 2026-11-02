//! Functional notations that change how a value is validated.
//!
//! - Math functions (CSS Values 4 §10.8,
//!   <https://drafts.csswg.org/css-values-4/#calc-syntax>) stand in for a
//!   numeric value; their result type is only known after type checking, so
//!   the numeric classification treats them as unknown.
//! - Arbitrary substitution functions (CSS Values 5 Appendix A,
//!   <https://drafts.csswg.org/css-values-5/#arbitrary-substitution>): when a
//!   value contains one, "the entire value’s grammar must be assumed to be
//!   valid at parse time". [`contains_arbitrary_substitution`] finds them
//!   anywhere in a parsed value.
//!
//! Function names match ASCII case-insensitively (CSS Values 4 §9).

use super::parser::ComponentValue;

/// The math functions of CSS Values 4 §10.8, in ASCII lowercase.
pub const MATH_FUNCTIONS: &[&str] = &[
    "calc", "min", "max", "clamp", "round", "mod", "rem", "sin", "cos", "tan", "asin", "acos",
    "atan", "atan2", "pow", "sqrt", "hypot", "log", "exp", "abs", "sign",
];

/// The named arbitrary substitution functions, in ASCII lowercase. Any
/// `<dashed-function>` (a name starting with `--`) is one as well.
pub const ARBITRARY_SUBSTITUTION_FUNCTIONS: &[&str] = &[
    "var",
    "env",
    "attr",
    "if",
    "inherit",
    "ident",
    "random-item",
];

/// Whether `name` (a function name without its `(`) is a math function.
#[must_use]
pub fn is_math_function(name: &str) -> bool {
    MATH_FUNCTIONS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(name))
}

/// Whether `name` (a function name without its `(`) is an arbitrary
/// substitution function.
#[must_use]
pub fn is_arbitrary_substitution_function(name: &str) -> bool {
    name.starts_with("--")
        || ARBITRARY_SUBSTITUTION_FUNCTIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(name))
}

/// Whether an arbitrary substitution function appears anywhere in
/// `values`, including inside functions and blocks.
#[must_use]
pub fn contains_arbitrary_substitution(values: &[ComponentValue<'_>]) -> bool {
    // An explicit stack: nesting is bounded by the parser, but recursion is
    // not needed.
    let mut pending = vec![values];
    while let Some(values) = pending.pop() {
        for value in values {
            match value {
                ComponentValue::Function(function) => {
                    if is_arbitrary_substitution_function(&function.name) {
                        return true;
                    }
                    pending.push(&function.value);
                }
                ComponentValue::Block(block) => pending.push(&block.value),
                ComponentValue::Token(_) => {}
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::syntax::parser::parse_component_values;

    fn substitutes(input: &str) -> bool {
        contains_arbitrary_substitution(&parse_component_values(input).values)
    }

    // Claim: "<calc()> = calc( <calc-sum> ) <min()> = min( <calc-sum># )
    // <max()> = max( <calc-sum># ) <clamp()> = clamp( [ <calc-sum> | none ],
    // <calc-sum>, [ <calc-sum> | none ] ) <round()> = round(
    // <rounding-strategy>?, <calc-sum>, <calc-sum>? ) <mod()> = mod(
    // <calc-sum>, <calc-sum> ) <rem()> = rem( <calc-sum>, <calc-sum> ) <sin()>
    // = sin( <calc-sum> ) <cos()> = cos( <calc-sum> ) <tan()> = tan(
    // <calc-sum> ) <asin()> = asin( <calc-sum> ) <acos()> = acos( <calc-sum> )
    // <atan()> = atan( <calc-sum> ) <atan2()> = atan2( <calc-sum>, <calc-sum>
    // ) <pow()> = pow( <calc-sum>, <calc-sum> ) <sqrt()> = sqrt( <calc-sum> )
    // <hypot()> = hypot( <calc-sum># ) <log()> = log( <calc-sum>, <calc-sum>?
    // ) <exp()> = exp( <calc-sum> ) <abs()> = abs( <calc-sum> ) <sign()> =
    // sign( <calc-sum> )"
    // https://drafts.csswg.org/css-values-4/#calc-syntax
    // Claim: "Like keywords, function names are ASCII case-insensitive."
    // https://drafts.csswg.org/css-values-4/#functional-notations
    #[test]
    fn math_function_names() {
        for name in [
            "calc", "min", "max", "clamp", "round", "mod", "rem", "sin", "cos", "tan", "asin",
            "acos", "atan", "atan2", "pow", "sqrt", "hypot", "log", "exp", "abs", "sign", "CALC",
            "Clamp",
        ] {
            assert!(is_math_function(name), "{name}");
        }
        assert_eq!(MATH_FUNCTIONS.len(), 21);
        for name in ["rgb", "url", "var", "calcx", "", "-calc"] {
            assert!(!is_math_function(name), "{name}");
        }
    }

    // Claim: "The var() function is an arbitrary substitution function"
    // https://drafts.csswg.org/css-variables-1/#using-variables
    // Claim: "If a property contains one or more env() functions, and those
    // functions are syntactically valid, the entire property’s grammar must
    // be assumed to be valid at parse time."
    // https://drafts.csswg.org/css-env-1/#env-function
    // Claim: "attr() is an arbitrary substitution function"
    // https://drafts.csswg.org/css-values-5/#attr-substitution
    // Claim: "The if() function is an arbitrary substitution function"
    // https://drafts.csswg.org/css-values-5/#if-notation
    // Claim: "inherit() is an arbitrary substitution function"
    // https://drafts.csswg.org/css-values-5/#inherit-notation
    // Claim: "The ident() function represents an arbitrary <ident>, ... It is
    // an arbitrary substitution function."
    // https://drafts.csswg.org/css-values-5/#funcdef-ident
    // Claim: "The random-item() function is an arbitrary substitution
    // function" https://drafts.csswg.org/css-values-5/#funcdef-random-item
    // Claim: "A <dashed-function> is a functional notation whose function name
    // starts with two dashes (U+002D HYPHEN-MINUS)." and "<dashed-function>s
    // are arbitrary substitution functions, like var()."
    // https://drafts.csswg.org/css-mixins-1/#typedef-dashed-function
    #[test]
    fn arbitrary_substitution_function_names() {
        for name in [
            "var",
            "env",
            "attr",
            "if",
            "inherit",
            "ident",
            "random-item",
            "VAR",
            "Env",
            "--negative",
            "--",
        ] {
            assert!(is_arbitrary_substitution_function(name), "{name}");
        }
        assert_eq!(ARBITRARY_SUBSTITUTION_FUNCTIONS.len(), 7);
        for name in ["calc", "variable", "-x", "url", ""] {
            assert!(!is_arbitrary_substitution_function(name), "{name}");
        }
    }

    // Claim: "If a property value contains one or more arbitrary substitution
    // functions, and all of those functions are themselves syntactically
    // valid according to their argument grammars, the entire value’s grammar
    // must be assumed to be valid at parse time."
    // https://drafts.csswg.org/css-values-5/#resolve-property
    #[test]
    fn substitution_is_found_at_any_depth() {
        assert!(substitutes("var(--x)"));
        assert!(substitutes("1px VAR(--x)"));
        assert!(substitutes("calc(1px + env(safe-area-inset-top))"));
        assert!(substitutes("[ (attr(data-x)) ]"));
        assert!(substitutes("--f(1)"));
        assert!(!substitutes("calc(1px + 2px)"));
        assert!(!substitutes("var"));
        assert!(!substitutes("--x"));
        assert!(!substitutes("'var(--x)'"));
        assert!(!substitutes(""));
    }
}
