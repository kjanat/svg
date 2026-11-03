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
//! - the value is a CSS-wide keyword, a `<whole-value>` notation, contains an
//!   arbitrary substitution function, or has any top-level function (math
//!   functions and the many other functional notations stand in for values
//!   whose type is only known after evaluation).
//!
//! Presentation attributes are parsed with `<number>` admitted wherever the
//! grammar has `<length>`, `<length-percentage>` or `<angle>`
//! (<https://svgwg.org/svg2-draft/types.html#presentation-attribute-css-value>).
//! The check cannot tell a presentation attribute from another attribute, so
//! it always does: that only accepts more.

use std::collections::BTreeSet;

use svg_data::CssGrammarGraph;

use super::syntax::{
    cascade::{is_css_wide_keyword, is_css_wide_keyword_value},
    notation::{contains_arbitrary_substitution, is_whole_value},
    parser::{ComponentValue, parse_component_values},
    tokenizer::Token,
    types::{NumericType, UnitlessMode, match_numeric},
};

/// `CssGrammar`: a value matching this CSS value grammar.
pub fn check_css_grammar(value: &str, grammar: &str, _graph: &CssGrammarGraph) -> Option<String> {
    if OPAQUE_GRAMMARS.contains(&grammar) {
        return None;
    }
    let effective = WIDENED_GRAMMARS
        .iter()
        .find_map(|&(catalog, widened)| (catalog == grammar).then_some(widened))
        .unwrap_or(grammar);
    let root = parse_grammar(effective, 0)?;

    let list = parse_component_values(value);
    if list.depth_limited
        || contains_arbitrary_substitution(&list.values)
        || is_whole_value(&list.values)
        || is_css_wide_keyword_value(value)
    {
        return None;
    }
    let values: Vec<&ComponentValue<'_>> = list.significant().collect();
    if values
        .iter()
        .any(|value| matches!(value, ComponentValue::Function(_)))
    {
        return None;
    }

    let matches = |ranges: bool| {
        Matcher {
            values: &values,
            ranges,
        }
        .ends(&root, 0, true)
        .contains(&values.len())
    };
    if matches(true) {
        None
    } else if matches(false) {
        Some(format!("a number is outside the range `{grammar}` allows"))
    } else {
        Some(format!("does not match `{grammar}`"))
    }
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
const OPAQUE_GRAMMARS: &[&str] = &["none | <string>", "<number>+"];

/// Catalog grammars that a later definition of the same property or
/// attribute widens, with the grammar matched in their place: the union of
/// both, so a value either definition allows is accepted.
const WIDENED_GRAMMARS: &[(&str, &str)] = &[
    // width, height, inline-size: "auto | <box-size> | <anchor-size()>"
    // (https://drafts.csswg.org/css-sizing-3/#propdef-width).
    (
        "auto | <length-percentage>",
        "auto | <length-percentage> | <box-size> | <anchor-size()>",
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
];

/// Markers of attribute value syntaxes other than CSS value definition
/// syntax (<https://svgwg.org/svg2-draft/types.html#syntax>).
const FOREIGN_SYNTAX_MARKERS: &[&str] =
    &["[EBNF]", "[ABNF]", "[URL]", "[HTML]", "(see below)", "\""];

/// How deeply named types may expand into each other.
const MAX_EXPANSION_DEPTH: usize = 8;

/// The most operands a `&&` or `||` group may have.
const MAX_INTERLEAVED: usize = 16;

/// A grammar node.
#[derive(Clone, Debug, PartialEq)]
enum Node {
    /// A keyword, matched ASCII case-insensitively.
    Keyword(String),
    /// A literal delimiter: `,` `/` `;` `:` or a quoted one.
    Delim(char),
    /// A basic data type, with an optional range restriction.
    Type(Leaf, Range),
    /// A functional notation, written literally or as `<name()>`. Only a
    /// function of that name matches; its arguments are not checked.
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
}

/// A closed range restriction. A bound that is unrestricted or cannot be
/// compared is infinite.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Range {
    min: f64,
    max: f64,
}

impl Range {
    /// No restriction: "If no range is indicated ... then [−∞,∞] is
    /// assumed."
    const UNRESTRICTED: Self = Self {
        min: f64::NEG_INFINITY,
        max: f64::INFINITY,
    };

    fn contains(self, value: f64) -> bool {
        (self.min..=self.max).contains(&value)
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

/// Parse a grammar, or `None` when it uses anything this module does not
/// implement.
fn parse_grammar(grammar: &str, depth: usize) -> Option<Node> {
    if depth > MAX_EXPANSION_DEPTH {
        return None;
    }
    let lexemes = lex(grammar)?;
    let mut parser = GrammarParser {
        lexemes: &lexemes,
        pos: 0,
        depth,
    };
    let node = parser.alternatives()?;
    (parser.pos == lexemes.len()).then_some(node)
}

/// Recursive descent over grammar lexemes, by combinator precedence:
/// "Juxtaposition is stronger than the double ampersand, the double
/// ampersand is stronger than the double bar, and the double bar is
/// stronger than the bar."
struct GrammarParser<'l, 'g> {
    lexemes: &'l [Lexeme<'g>],
    pos: usize,
    depth: usize,
}

impl<'g> GrammarParser<'_, 'g> {
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
            Lexeme::Type(reference) => resolve_type(reference, self.depth)?,
            Lexeme::OpenGroup => {
                let inner = self.alternatives()?;
                if !self.eat(Lexeme::CloseGroup) {
                    return None;
                }
                inner
            }
            Lexeme::Function(name) => {
                // The arguments are parsed for well-formedness only: a
                // function in a value is accepted without matching.
                if self.peek() != Some(Lexeme::CloseParen) {
                    self.alternatives()?;
                }
                if !self.eat(Lexeme::CloseParen) {
                    return None;
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

/// The node for `<reference>` (the text between `<` and `>`), or `None` for
/// a reference this module does not implement.
fn resolve_type(reference: &str, depth: usize) -> Option<Node> {
    if reference.starts_with('\'') {
        // A property value range, <'property'>.
        return None;
    }
    if let Some(name) = reference.strip_suffix("()") {
        return Some(Node::Function(name.to_owned()));
    }
    let (name, range) = match reference.split_once('[') {
        Some((name, range)) => (name.trim(), parse_range(range.strip_suffix(']')?)?),
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
        "url" => Leaf::Url,
        "custom-ident" => Leaf::CustomIdent,
        "dashed-ident" => Leaf::DashedIdent,
        "ident" => Leaf::Ident,
        _ => {
            if reference.contains('[') {
                return None;
            }
            let (_, grammar) = NAMED_TYPES.iter().find(|(named, _)| *named == name)?;
            return parse_grammar(grammar, depth + 1);
        }
    };
    Some(Node::Type(leaf, range))
}

/// The bounds of a bracketed range, `min,max` without the brackets. A bound
/// of ±∞, or one with a unit other than a zero, is left unchecked.
fn parse_range(text: &str) -> Option<Range> {
    let (min, max) = text.split_once(',')?;
    Some(Range {
        min: parse_bound(min, f64::NEG_INFINITY)?,
        max: parse_bound(max, f64::INFINITY)?,
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
            Node::Function(name) => {
                matches!(value, ComponentValue::Function(function) if function.name.eq_ignore_ascii_case(name))
            }
            Node::Type(leaf, range) => {
                leaf_matches(*leaf, value) && (!self.ranges || in_range(*range, value))
            }
            _ => false,
        }
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

/// Whether `value` is a literal of `leaf`.
fn leaf_matches(leaf: Leaf, value: &ComponentValue<'_>) -> bool {
    let ComponentValue::Token(token) = value else {
        return false;
    };
    match leaf {
        Leaf::Numeric(ty) => !match_numeric(value, ty, UnitlessMode::PresentationAttribute).is_no(),
        Leaf::String => matches!(token, Token::String(_)),
        Leaf::Url => matches!(token, Token::Url(_)),
        Leaf::CustomIdent => matches!(
            token,
            Token::Ident(ident)
                if !is_css_wide_keyword(ident) && !ident.eq_ignore_ascii_case("default")
        ),
        Leaf::DashedIdent => matches!(token, Token::Ident(ident) if ident.starts_with("--")),
        Leaf::Ident => matches!(token, Token::Ident(_)),
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

    use super::check_css_grammar;

    const NO_GRAPH: CssGrammarGraph = CssGrammarGraph {
        root: 0,
        nodes: &[],
        edges: &[],
    };

    fn check(value: &str, grammar: &str) -> Option<String> {
        check_css_grammar(value, grammar, &NO_GRAPH)
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
        let grammar = real("width", None, "auto | <length-percentage>");
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
            let grammar = real(attr, None, "auto | <length-percentage>");
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
        let width = real("width", None, "auto | <length-percentage>");
        valid(width, &["-5", "-1px"]);
    }

    // Claim: "Since presentation attributes are parsed as CSS values, not
    // declarations, an !important declaration within a presentation
    // attribute will cause it to have an invalid value."
    // https://svgwg.org/svg2-draft/styling.html#PresentationAttributes
    #[test]
    fn important_makes_a_presentation_attribute_invalid() {
        let grammar = real("width", None, "auto | <length-percentage>");
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
            let grammar = real(attr, None, quoted);
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
        let grammar = real("width", None, "auto | <length-percentage>");
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
        let grammar = real("width", None, "auto | <length-percentage>");
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
        let width = real("width", None, "auto | <length-percentage>");
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
    // Every usage whose grammar has a non-terminal or property value range
    // the matcher does not expand is accepted whole, so these values are
    // accepted (accept-only: the stub accepts them too).
    #[test]
    fn unexpanded_references_accept() {
        let cases: &[(&str, Option<&str>, &str)] = &[
            ("alignment-baseline", None, "middle"),
            ("background", None, "red url(a.png) no-repeat"),
            ("baseFrequency", Some("feTurbulence"), "0.05,0.1"),
            ("border-style", None, "solid dashed"),
            ("clip-path", None, "url(#c)"),
            ("color-profile", None, "sRGB"),
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
            ("object-position", None, "right 10px top"),
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
            ("to", Some("set"), "visible"),
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
            valid(grammar, &[value, "#!garbage"]);
        }
    }

    // Claim: "The SVG 2 specification defines attribute values ... By
    // reference to an EBNF symbol defined in this or another specification
    // [xml]. For external definitions, this is indicated by [EBNF] appearing
    // in the Value column." https://svgwg.org/svg2-draft/types.html#syntax
    // and the SMIL-style grammars quote literal values ("media",
    // "indefinite") and name non-CSS productions (Clock-value). These are
    // not CSS value definition syntax and are accepted (accept-only).
    #[test]
    fn non_css_grammars_accept() {
        let cases: &[(&str, Option<&str>, &str)] = &[
            ("attributeName", Some("animate"), "fill"),
            ("dur", Some("animate"), "2s"),
            ("dur", Some("animate"), "indefinite"),
            ("max", Some("animate"), "5s"),
            ("min", Some("animate"), "media"),
            ("repeatDur", Some("animate"), "01:30"),
        ];
        for &(attr, element, value) in cases {
            let def =
                svg_data::attribute(attr).unwrap_or_else(|| panic!("{attr} is in the catalog"));
            let AttributeValues::CssGrammar { grammar, .. } =
                element.map_or(&def.values, |element| def.values_for_element(Some(element)))
            else {
                panic!("{attr} is not a CSS grammar");
            };
            valid(grammar, &[value, "not a clock"]);
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
