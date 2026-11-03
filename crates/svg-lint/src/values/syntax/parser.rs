//! CSS Syntax 3 component values.
//!
//! [`parse_component_values`] implements "parse a list of component values"
//! (§5.4.9, <https://drafts.csswg.org/css-syntax-3/#parse-list-of-component-values>):
//! the token stream grouped into preserved tokens, functions and simple
//! blocks (§5.5.7 to §5.5.10). The result also records whether every
//! function and block was closed by its mirror token and no stray closing
//! token was seen, which the spec itself does not report.
//!
//! [`comma_separated`] splits a parsed list the way "parse a
//! comma-separated list of component values" (§5.4.10) does.
//!
//! Parsing is iterative, so deeply nested input cannot overflow the stack;
//! nesting beyond [`MAX_NESTING`] stops the parse and sets
//! [`ComponentValueList::depth_limited`].

use std::borrow::Cow;

use super::tokenizer::{Token, tokenize};

/// How deeply functions and blocks may nest before parsing stops.
pub const MAX_NESTING: usize = 256;

/// The associated token of a simple block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// A `[]-block`.
    Square,
    /// A `()-block`.
    Paren,
    /// A `{}-block`.
    Curly,
}

/// A function (§5.3): a name and the component values of its arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct Function<'a> {
    /// The function name, as written (function names match ASCII
    /// case-insensitively).
    pub name: Cow<'a, str>,
    /// The argument component values, whitespace and commas included.
    pub value: Vec<ComponentValue<'a>>,
    /// Whether a `)` closed the function (rather than the end of input).
    pub closed: bool,
}

/// A simple block (§5.3).
#[derive(Clone, Debug, PartialEq)]
pub struct SimpleBlock<'a> {
    /// The associated token.
    pub kind: BlockKind,
    /// The component values inside the block.
    pub value: Vec<ComponentValue<'a>>,
    /// Whether the mirror token closed the block (rather than the end of
    /// input).
    pub closed: bool,
}

/// A component value (§5.3): a preserved token, a function or a simple
/// block.
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentValue<'a> {
    /// Any token except `<function-token>`, `<{-token>`, `<(-token>` and
    /// `<[-token>`.
    Token(Token<'a>),
    /// A function.
    Function(Function<'a>),
    /// A simple block.
    Block(SimpleBlock<'a>),
}

impl ComponentValue<'_> {
    /// Whether this is a `<whitespace-token>`.
    #[must_use]
    pub const fn is_whitespace(&self) -> bool {
        matches!(self, Self::Token(Token::Whitespace))
    }

    /// The value of an `<ident-token>`.
    #[must_use]
    pub fn ident(&self) -> Option<&str> {
        match self {
            Self::Token(Token::Ident(value)) => Some(value),
            _ => None,
        }
    }
}

/// The result of [`parse_component_values`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentValueList<'a> {
    /// The top-level component values, whitespace included.
    pub values: Vec<ComponentValue<'a>>,
    /// Every function and block was closed by its mirror token, and no
    /// `)`, `]` or `}` appeared without a matching opener.
    pub balanced: bool,
    /// Nesting exceeded [`MAX_NESTING`]: the values are incomplete and a
    /// check must treat the input as unknown.
    pub depth_limited: bool,
}

impl<'a> ComponentValueList<'a> {
    /// The top-level values that are not whitespace.
    pub fn significant(&self) -> impl Iterator<Item = &ComponentValue<'a>> {
        self.values.iter().filter(|value| !value.is_whitespace())
    }
}

/// An open function or block, with the values of its parent.
struct Frame<'a> {
    kind: FrameKind<'a>,
    parent: Vec<ComponentValue<'a>>,
}

enum FrameKind<'a> {
    Function(Cow<'a, str>),
    Block(BlockKind),
}

impl<'a> FrameKind<'a> {
    /// Whether `token` is the mirror token that closes this frame.
    const fn is_closed_by(&self, token: &Token<'_>) -> bool {
        matches!(
            (self, token),
            (
                Self::Function(_) | Self::Block(BlockKind::Paren),
                Token::CloseParen
            ) | (Self::Block(BlockKind::Square), Token::CloseSquare)
                | (Self::Block(BlockKind::Curly), Token::CloseCurly)
        )
    }

    fn finish(self, value: Vec<ComponentValue<'a>>, closed: bool) -> ComponentValue<'a> {
        match self {
            Self::Function(name) => ComponentValue::Function(Function {
                name,
                value,
                closed,
            }),
            Self::Block(kind) => ComponentValue::Block(SimpleBlock {
                kind,
                value,
                closed,
            }),
        }
    }
}

/// "Parse a list of component values" (§5.4.9) from a string.
///
/// "Consume a list of component values" (§5.5.7) with `nested` false,
/// "consume a component value" (§5.5.8), "consume a simple block" (§5.5.9)
/// and "consume a function" (§5.5.10), run with an explicit stack.
#[must_use]
pub fn parse_component_values(input: &str) -> ComponentValueList<'_> {
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut current = Vec::new();
    let mut balanced = true;
    let mut depth_limited = false;
    for token in tokenize(input) {
        let opened = match token {
            Token::Function(name) => FrameKind::Function(name),
            Token::OpenParen => FrameKind::Block(BlockKind::Paren),
            Token::OpenSquare => FrameKind::Block(BlockKind::Square),
            Token::OpenCurly => FrameKind::Block(BlockKind::Curly),
            token => {
                if stack
                    .last()
                    .is_some_and(|frame| frame.kind.is_closed_by(&token))
                {
                    if let Some(frame) = stack.pop() {
                        let value = std::mem::replace(&mut current, frame.parent);
                        current.push(frame.kind.finish(value, true));
                    }
                } else {
                    if matches!(
                        token,
                        Token::CloseParen | Token::CloseSquare | Token::CloseCurly
                    ) {
                        balanced = false;
                    }
                    current.push(ComponentValue::Token(token));
                }
                continue;
            }
        };
        if stack.len() == MAX_NESTING {
            depth_limited = true;
            break;
        }
        stack.push(Frame {
            kind: opened,
            parent: std::mem::take(&mut current),
        });
    }
    // The end of input closes whatever is still open.
    while let Some(frame) = stack.pop() {
        balanced = false;
        let value = std::mem::replace(&mut current, frame.parent);
        current.push(frame.kind.finish(value, false));
    }
    ComponentValueList {
        values: current,
        balanced: balanced && !depth_limited,
        depth_limited,
    }
}

/// The groups of "parse a comma-separated list of component values"
/// (§5.4.10), taken from an already parsed list: `values` split at its
/// top-level `<comma-token>`s. As in the spec, a final comma does not start
/// another group, and an empty list has no groups.
pub fn comma_separated<'v, 'a>(
    values: &'v [ComponentValue<'a>],
) -> impl Iterator<Item = &'v [ComponentValue<'a>]> {
    let is_comma =
        |value: &ComponentValue<'_>| matches!(value, ComponentValue::Token(Token::Comma));
    let body = match values.split_last() {
        Some((last, init)) if is_comma(last) => init,
        _ => values,
    };
    (!values.is_empty())
        .then(|| body.split(is_comma))
        .into_iter()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::syntax::tokenizer::{Number, NumberType};

    fn ident(value: &str) -> ComponentValue<'_> {
        ComponentValue::Token(Token::Ident(Cow::Borrowed(value)))
    }

    const fn token(token: Token<'static>) -> ComponentValue<'static> {
        ComponentValue::Token(token)
    }

    const fn int(value: f64) -> ComponentValue<'static> {
        ComponentValue::Token(Token::Number(Number {
            value,
            kind: NumberType::Integer,
            sign: None,
        }))
    }

    const WS: ComponentValue<'static> = ComponentValue::Token(Token::Whitespace);

    // Claim: "A component value is one of the preserved tokens, a function, or
    // a simple block." and "preserved tokens: Any token produced by the
    // tokenizer except for <function-token>s, <{-token>s, <(-token>s, and
    // <[-token>s."
    // https://drafts.csswg.org/css-syntax-3/#component-value
    // Claim: "To parse a list of component values from input: Normalize input,
    // and set input to the result. Consume a list of component values from
    // input, and return the result."
    // https://drafts.csswg.org/css-syntax-3/#parse-list-of-component-values
    #[test]
    fn top_level_tokens_are_preserved() {
        let list = parse_component_values("a 1,;");
        assert_eq!(
            list.values,
            [
                ident("a"),
                WS,
                int(1.0),
                token(Token::Comma),
                token(Token::Semicolon)
            ]
        );
        assert!(list.balanced);
        assert!(!list.depth_limited);
        assert_eq!(parse_component_values("").values, []);
        assert!(parse_component_values("").balanced);
    }

    // Claim: "Consume a token from input, and let function be a new function
    // with its name equal the returned token’s value, and a value set to an
    // empty list. Process input: <eof-token> <)-token> Discard a token from
    // input. Return function. anything else Consume a component value from
    // input and append the result to function’s value."
    // https://drafts.csswg.org/css-syntax-3/#consume-function
    #[test]
    fn functions_hold_their_argument_values() {
        let list = parse_component_values("calc(1 + f(2))");
        assert!(list.balanced);
        let [ComponentValue::Function(calc)] = list.values.as_slice() else {
            panic!("one function: {:?}", list.values);
        };
        assert_eq!(calc.name, "calc");
        assert!(calc.closed);
        assert_eq!(calc.value.len(), 5);
        assert_eq!(calc.value[0], int(1.0));
        assert_eq!(calc.value[2], token(Token::Delim('+')));
        let ComponentValue::Function(inner) = &calc.value[4] else {
            panic!("nested function: {:?}", calc.value[4]);
        };
        assert_eq!(inner.name, "f");
        assert_eq!(inner.value, [int(2.0)]);

        let open = parse_component_values("f(1");
        assert!(!open.balanced);
        let [ComponentValue::Function(f)] = open.values.as_slice() else {
            panic!("one function: {:?}", open.values);
        };
        assert!(!f.closed);
        assert_eq!(f.value, [int(1.0)]);
    }

    // Claim: "Let ending token be the mirror variant of the next token. (E.g. if
    // it was called with <[-token>, the ending token is <]-token>.) Let block
    // be a new simple block with its associated token set to the next token
    // and with its value initially set to an empty list. Discard a token from
    // input. Process input: <eof-token> ending token Discard a token from
    // input. Return block. anything else Consume a component value from input
    // and append the result to block’s value."
    // https://drafts.csswg.org/css-syntax-3/#consume-simple-block
    #[test]
    fn simple_blocks_match_their_mirror_token() {
        let list = parse_component_values("[a](b){c}");
        assert!(list.balanced);
        let kinds: Vec<_> = list
            .values
            .iter()
            .map(|value| match value {
                ComponentValue::Block(block) => Some((block.kind, block.closed, block.value.len())),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            [
                Some((BlockKind::Square, true, 1)),
                Some((BlockKind::Paren, true, 1)),
                Some((BlockKind::Curly, true, 1)),
            ]
        );

        // A mismatched closer is an ordinary value inside the block.
        let mismatched = parse_component_values("(a]");
        assert!(!mismatched.balanced);
        let [ComponentValue::Block(block)] = mismatched.values.as_slice() else {
            panic!("one block: {:?}", mismatched.values);
        };
        assert!(!block.closed);
        assert_eq!(block.value, [ident("a"), token(Token::CloseSquare)]);
    }

    // Claim: "<}-token> If nested is true, return values. Otherwise, this is a
    // parse error. Consume a token from input and append the result to
    // values." and "Note: The tokens <}-token>s, <)-token>s, <]-token>,
    // <bad-string-token>, and <bad-url-token> are always parse errors, but
    // they are preserved in the token stream"
    // https://drafts.csswg.org/css-syntax-3/#consume-list-of-components
    #[test]
    fn stray_closers_are_preserved_and_unbalanced() {
        for input in ["a)", "]", "}"] {
            let list = parse_component_values(input);
            assert!(!list.balanced, "{input:?} is unbalanced");
            assert!(!list.values.is_empty(), "{input:?} keeps its tokens");
        }
        assert_eq!(
            parse_component_values("}").values,
            [token(Token::CloseCurly)]
        );
        assert!(parse_component_values("(a) [b]").balanced);
    }

    // Implementation limit, not a spec claim: nesting deeper than MAX_NESTING
    // stops the parse instead of exhausting the stack.
    #[test]
    fn nesting_is_limited() {
        let deep = "(".repeat(MAX_NESTING + 10);
        let list = parse_component_values(&deep);
        assert!(list.depth_limited);
        assert!(!list.balanced);
        let fine = "(".repeat(MAX_NESTING);
        assert!(!parse_component_values(&fine).depth_limited);
    }

    // Claim: "Let groups be an empty list. While input is not empty: Consume a
    // list of component values from input, with <comma-token> as the stop
    // token, and append the result to groups. Discard a token from input.
    // Return groups."
    // https://drafts.csswg.org/css-syntax-3/#parse-comma-separated-list-of-component-values
    #[test]
    fn comma_separated_groups() {
        let list = parse_component_values("a, f(b, c),,d,");
        let groups: Vec<_> = comma_separated(&list.values).collect();
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0], [ident("a")]);
        assert_eq!(groups[1].len(), 2);
        assert_eq!(groups[2], []);
        assert_eq!(groups[3], [ident("d")]);
        assert_eq!(comma_separated(&[]).count(), 0);
        assert_eq!(
            comma_separated(&parse_component_values(",").values).count(),
            1
        );
    }

    // Helper test, not a spec claim: the `significant`, `ident` and
    // `is_whitespace` accessors.
    #[test]
    fn component_value_accessors() {
        let list = parse_component_values(" a ");
        assert_eq!(list.significant().count(), 1);
        assert_eq!(list.values[1].ident(), Some("a"));
        assert!(list.values[0].is_whitespace());
        assert_eq!(list.values[0].ident(), None);
    }
}
