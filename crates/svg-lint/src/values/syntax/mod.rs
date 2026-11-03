//! The shared lexical layer the attribute value checks build on.
//!
//! Every helper here implements one named section of a specification, so a
//! check can say exactly which production it applies:
//!
//! - [`tokenizer`]: CSS Syntax 3 §4 "Tokenization"
//!   (<https://drafts.csswg.org/css-syntax-3/#tokenization>), including the
//!   §3.3 input preprocessing, escapes and the "would start an ident
//!   sequence / a number" checks of §4.3.
//! - [`parser`]: CSS Syntax 3 §5.4.9 "Parse a list of component values"
//!   (<https://drafts.csswg.org/css-syntax-3/#parse-list-of-component-values>)
//!   and §5.4.10 "Parse a comma-separated list of component values".
//! - [`units`]: the unit identifiers of CSS Values 4 §6 and §7
//!   (<https://drafts.csswg.org/css-values-4/#lengths>), the container query
//!   units of CSS Conditional 5 §7 and the `fr` unit of CSS Grid 1 §7.2.4.
//! - [`types`]: classification of one component value as a CSS Values 4
//!   numeric data type (§5 to §7), with the SVG 2 presentation attribute
//!   rule that admits unitless lengths and angles
//!   (<https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value>)
//!   as an explicit mode.
//! - [`notation`]: the math functions of CSS Values 4 §10.8 and CSS Values
//!   5, the `<whole-value>` notations of CSS Values 5 §8.1, and the
//!   arbitrary substitution functions of CSS Values 5 Appendix A, CSS
//!   Variables 1, CSS Environment Variables 1 and CSS Mixins 1.
//! - [`cascade`]: the CSS-wide keywords of CSS Cascade 5 §7.3.
//! - [`microsyntax`]: XML 1.0 `S`, the SVG 2 path data `wsp`, `comma_wsp`,
//!   `number`, `coordinate` and `flag` productions
//!   (<https://svgwg.org/svg2-draft/paths.html#PathDataBNF>; for path data
//!   and the attributes that still refer to them, not for values written in
//!   CSS value definition syntax), and the
//!   Infra string operations HTML microsyntaxes use (ASCII whitespace,
//!   ASCII case-insensitive matching, splitting on whitespace and commas).
//!
//! The layer only answers lexical questions. When it cannot decide (a math
//! function, an arbitrary substitution function, a construct it does not
//! model), it says so ([`types::Match::Unknown`]) so the checks can accept the
//! value: a false positive on valid SVG is worse than a missed error.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "used by the value checks that build on this layer"
    )
)]

pub mod cascade;
pub mod microsyntax;
pub mod notation;
pub mod parser;
pub mod tokenizer;
pub mod types;
pub mod units;
