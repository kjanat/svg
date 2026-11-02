//! CSS value-definition-syntax grammars.
//!
//! Each check returns `None` for a valid value and `Some(reason)` for an
//! invalid one. Constructs a check does not model must be accepted, never
//! reported: a false positive on valid SVG is worse than a missed error.

use svg_data::CssGrammarGraph;

/// `CssGrammar`: a value matching this CSS value grammar.
#[allow(
    clippy::missing_const_for_fn,
    reason = "stub: replaced by the real check"
)]
pub fn check_css_grammar(value: &str, grammar: &str, graph: &CssGrammarGraph) -> Option<String> {
    let _ = (value, grammar, graph);
    None
}
