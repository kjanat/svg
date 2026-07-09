//! Build the value-grammar graph for a CSS/SVG attribute value expression.
//!
//! Tokenizes a value grammar (`<number> | auto | <length>#`, …) into a small
//! node/edge graph — type references, keywords, combinators, and multipliers —
//! so consumers can inspect a value space structurally rather than re-parsing
//! the grammar string.

use super::{
    CatalogCssGrammarEdge, CatalogCssGrammarEdgeKind, CatalogCssGrammarGraph,
    CatalogCssGrammarNode, CatalogCssGrammarNodeKind,
};

#[derive(Clone, Copy)]
struct GrammarContext {
    node_id: u16,
    last_child: Option<u16>,
}

pub(super) fn css_grammar_graph(value: &str) -> CatalogCssGrammarGraph {
    let mut graph = empty_css_grammar_graph();
    let mut contexts = vec![GrammarContext {
        node_id: 0,
        last_child: None,
    }];
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'<' => {
                let Some(end) = value[index..].find('>') else {
                    break;
                };
                let text = &value[index..=index + end];
                push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Type,
                    Some(text),
                );
                index += end + 1;
            }
            b'[' => {
                let id = push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Group,
                    None,
                );
                contexts.push(GrammarContext {
                    node_id: id,
                    last_child: None,
                });
                index += 1;
            }
            b']' => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
                index += 1;
            }
            b'|' if bytes.get(index + 1) == Some(&b'|') => {
                push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Operator,
                    Some("||"),
                );
                index += 2;
            }
            b'&' if bytes.get(index + 1) == Some(&b'&') => {
                push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Operator,
                    Some("&&"),
                );
                index += 2;
            }
            b'|' | b',' | b'?' | b'*' | b'+' | b'#' | b'!' => {
                let text = &value[index..=index];
                push_grammar_node(
                    &mut graph,
                    &mut contexts,
                    CatalogCssGrammarNodeKind::Operator,
                    Some(text),
                );
                index += 1;
            }
            byte if byte.is_ascii_alphanumeric() || byte == b'-' => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'-')
                {
                    index += 1;
                }
                let text = &value[start..index];
                let kind = if bytes.get(index) == Some(&b'(') {
                    while index < bytes.len() && bytes[index] != b')' {
                        index += 1;
                    }
                    if index < bytes.len() {
                        index += 1;
                    }
                    CatalogCssGrammarNodeKind::Function
                } else {
                    CatalogCssGrammarNodeKind::Keyword
                };
                push_grammar_node(&mut graph, &mut contexts, kind, Some(text));
            }
            _ => index += 1,
        }
    }

    graph
}

fn empty_css_grammar_graph() -> CatalogCssGrammarGraph {
    CatalogCssGrammarGraph {
        root: 0,
        nodes: vec![CatalogCssGrammarNode {
            id: 0,
            kind: CatalogCssGrammarNodeKind::Root,
            text: None,
        }],
        edges: Vec::new(),
    }
}

fn push_grammar_node(
    graph: &mut CatalogCssGrammarGraph,
    contexts: &mut [GrammarContext],
    kind: CatalogCssGrammarNodeKind,
    text: Option<&str>,
) -> u16 {
    let Ok(id) = u16::try_from(graph.nodes.len()) else {
        return u16::MAX;
    };
    graph.nodes.push(CatalogCssGrammarNode {
        id,
        kind,
        text: text.map(str::to_owned),
    });

    let Some(current) = contexts.last_mut() else {
        return id;
    };
    graph.edges.push(CatalogCssGrammarEdge {
        from: current.node_id,
        to: id,
        kind: CatalogCssGrammarEdgeKind::Contains,
    });
    if let Some(previous) = current.last_child {
        graph.edges.push(CatalogCssGrammarEdge {
            from: previous,
            to: id,
            kind: CatalogCssGrammarEdgeKind::Next,
        });
    }
    current.last_child = Some(id);
    id
}
