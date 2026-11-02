//! The CSS-wide keywords.
//!
//! CSS Values 4 §4.1.1 (<https://drafts.csswg.org/css-values-4/#common-keywords>)
//! says every property accepts the CSS-wide keywords and defers their
//! definition to CSS Cascade, whose §7.3 "Explicit Defaulting"
//! (<https://drafts.csswg.org/css-cascade-5/#defaulting-keywords>) defines
//! `initial`, `inherit`, `unset`, `revert`, `revert-layer` and
//! `revert-rule`. Keywords match ASCII case-insensitively (CSS Values 4
//! §4.1).

use super::tokenizer::{Token, tokenize};

/// The CSS-wide keywords, in ASCII lowercase.
pub const CSS_WIDE_KEYWORDS: &[&str] = &[
    "initial",
    "inherit",
    "unset",
    "revert",
    "revert-layer",
    "revert-rule",
];

/// Whether `keyword` (an identifier's value) is a CSS-wide keyword.
#[must_use]
pub fn is_css_wide_keyword(keyword: &str) -> bool {
    CSS_WIDE_KEYWORDS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(keyword))
}

/// Whether `value`, tokenized as CSS, is a single CSS-wide keyword with
/// nothing but whitespace and comments around it. Escapes count: `\69nherit`
/// is `inherit`.
#[must_use]
pub fn is_css_wide_keyword_value(value: &str) -> bool {
    let mut tokens = tokenize(value).filter(|token| *token != Token::Whitespace);
    matches!(
        (tokens.next(), tokens.next()),
        (Some(Token::Ident(keyword)), None) if is_css_wide_keyword(&keyword)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Claim: "all properties accept the CSS-wide keywords, which represent
    // value computations common to all CSS properties."
    // https://drafts.csswg.org/css-values-4/#common-keywords
    // Claim: "The initial CSS-wide keyword represents the value defined as the
    // property’s initial value." / "The inherit CSS-wide keyword represents
    // the property’s computed value on the parent element." / "The unset
    // CSS-wide keyword acts as either inherit or initial" / "The revert
    // CSS-wide keyword rolls back the cascade" / "The revert-layer CSS-wide
    // keyword rolls back the cascade similar to revert" / "The revert-rule
    // CSS-wide keyword rolls back the cascade similar to revert and
    // revert-layer"
    // https://drafts.csswg.org/css-cascade-5/#defaulting-keywords
    #[test]
    fn css_wide_keywords() {
        for keyword in [
            "initial",
            "inherit",
            "unset",
            "revert",
            "revert-layer",
            "revert-rule",
        ] {
            assert!(is_css_wide_keyword(keyword), "{keyword}");
        }
        assert_eq!(CSS_WIDE_KEYWORDS.len(), 6);
        for keyword in ["default", "none", "inherits", "revert-", "", "auto"] {
            assert!(!is_css_wide_keyword(keyword), "{keyword}");
        }
    }

    // Claim: "Keywords are identifiers and are interpreted ASCII
    // case-insensitively (i.e., [a-z] and [A-Z] are equivalent)."
    // https://drafts.csswg.org/css-values-4/#keywords
    #[test]
    fn css_wide_keywords_match_ascii_case_insensitively() {
        for keyword in ["INHERIT", "Initial", "UnSet", "REVERT-LAYER"] {
            assert!(is_css_wide_keyword(keyword), "{keyword}");
        }
        // U+0130 and U+212A are not ASCII case variants.
        assert!(!is_css_wide_keyword("\u{130}nherit"));
        assert!(!is_css_wide_keyword("u\u{212A}set"));
    }

    // Claim: "Keywords are identifiers" (an identifier corresponds to "the
    // <ident-token> grammar", so escapes, comments and surrounding whitespace
    // follow tokenization).
    // https://drafts.csswg.org/css-values-4/#keywords
    #[test]
    fn css_wide_keyword_values() {
        for value in [
            "inherit",
            " inherit ",
            "\tINHERIT\n",
            "/* x */unset",
            "\\69nherit",
            "revert-rule/**/",
        ] {
            assert!(is_css_wide_keyword_value(value), "{value:?}");
        }
        for value in [
            "inherit inherit",
            "'inherit'",
            "inherit;",
            "inherit()",
            "",
            "  ",
            "-inherit",
            "inherit !important",
        ] {
            assert!(!is_css_wide_keyword_value(value), "{value:?}");
        }
    }
}
