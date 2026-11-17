//! CSS type, property and function grammars the catalog's CSS value grammars
//! reference.
//!
//! A catalog grammar such as `<clip-source> | [ <basic-shape> || <geometry-box> ] | none`
//! names types it does not define. Consumers that validate values against the
//! grammar need those definitions, so the catalog carries every `<type>`,
//! `<'property'>` and functional notation reachable from any attribute
//! grammar, with the `@webref/css` syntax that defines it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Serialize;

use super::{CatalogAttribute, CatalogAttributeValues};
use crate::treesitter::GrammarProjectionInputs;

/// Definitions of the CSS types, property value ranges and functions that
/// catalog value grammars reference, transitively.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CatalogCssDefinitions {
    /// `<type>` grammars, keyed by type name without brackets.
    pub types: BTreeMap<String, String>,
    /// `<'property'>` value grammars, keyed by property name.
    pub properties: BTreeMap<String, String>,
    /// Function syntaxes, keyed by function name without `()`. A function
    /// defined for several properties (`rect()` for `<basic-shape>` and for
    /// `clip`) has every definition.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub functions: BTreeMap<String, Vec<String>>,
}

/// One reference inside a value definition syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reference<'g> {
    /// `<name>` or `<name [min,max]>`.
    Type(&'g str),
    /// `<'name'>`.
    Property(&'g str),
    /// `<name()>`, or `name(` written literally.
    Function(&'g str),
}

const fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

/// Every type, property and function reference in `grammar`.
fn references(grammar: &str) -> Vec<Reference<'_>> {
    let bytes = grammar.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'<' => {
                let Some(close) = grammar[at + 1..].find('>') else {
                    break;
                };
                let inner = grammar[at + 1..at + 1 + close].trim();
                at += close + 2;
                if let Some(property) = inner
                    .strip_prefix('\'')
                    .and_then(|name| name.strip_suffix('\''))
                {
                    out.push(Reference::Property(property.trim()));
                } else if let Some(function) = inner.strip_suffix("()") {
                    out.push(Reference::Function(function.trim()));
                } else {
                    let name = inner
                        .split(['[', '(', ' '])
                        .next()
                        .unwrap_or_default()
                        .trim();
                    if !name.is_empty() {
                        out.push(Reference::Type(name));
                    }
                }
            }
            byte if is_name_byte(byte) => {
                let start = at;
                while at < bytes.len() && is_name_byte(bytes[at]) {
                    at += 1;
                }
                if bytes.get(at) == Some(&b'(') {
                    out.push(Reference::Function(&grammar[start..at]));
                }
            }
            _ => at += 1,
        }
    }
    out
}

/// Every CSS value grammar the catalog gives an attribute: canonical, per
/// profile and per element.
fn attribute_grammars(attributes: &[CatalogAttribute]) -> Vec<&str> {
    attributes
        .iter()
        .flat_map(|attribute| {
            std::iter::once(&attribute.values)
                .chain(attribute.value_overrides.iter().map(|o| &o.values))
                .chain(attribute.element_values.iter().map(|scoped| &scoped.values))
        })
        .filter_map(|values| match values {
            CatalogAttributeValues::CssGrammar { grammar, .. } => Some(grammar.as_str()),
            _ => None,
        })
        .collect()
}

/// The definitions reachable from the catalog's attribute grammars, resolved
/// through `syntaxes_of`. References it cannot resolve (basic types such as
/// `<length>`, SVG-local types) are left out.
fn collect<'a>(
    roots: impl IntoIterator<Item = &'a str>,
    syntaxes_of: impl Fn(Reference<'_>) -> Vec<&'a str>,
) -> CatalogCssDefinitions {
    let mut definitions = CatalogCssDefinitions::default();
    let mut pending: Vec<&str> = roots.into_iter().collect();
    while let Some(grammar) = pending.pop() {
        for reference in references(grammar) {
            let known = match reference {
                Reference::Type(name) => definitions.types.contains_key(name),
                Reference::Property(name) => definitions.properties.contains_key(name),
                Reference::Function(name) => definitions.functions.contains_key(name),
            };
            if known {
                continue;
            }
            let syntaxes = syntaxes_of(reference);
            let Some(&first) = syntaxes.first() else {
                continue;
            };
            match reference {
                Reference::Type(name) => {
                    definitions.types.insert(name.to_owned(), first.to_owned());
                }
                Reference::Property(name) => {
                    definitions
                        .properties
                        .insert(name.to_owned(), first.to_owned());
                }
                Reference::Function(name) => {
                    definitions.functions.insert(
                        name.to_owned(),
                        syntaxes.iter().map(|&syntax| syntax.to_owned()).collect(),
                    );
                }
            }
            pending.extend(syntaxes);
        }
    }
    definitions
}

/// The `@webref/css` definitions of every type, property range and function
/// the catalog's attribute grammars reach.
pub(super) fn build_css_definitions(
    attributes: &[CatalogAttribute],
    inputs: &GrammarProjectionInputs,
) -> CatalogCssDefinitions {
    collect(
        attribute_grammars(attributes),
        |reference| match reference {
            Reference::Type(name) => inputs.type_syntax(name).into_iter().collect(),
            Reference::Property(name) => inputs.property_syntax(name).into_iter().collect(),
            Reference::Function(name) => inputs
                .function_syntaxes(name)
                .iter()
                .map(String::as_str)
                .collect(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_split_types_properties_and_functions() {
        assert_eq!(
            references(
                "<'font-style'> || <number [0,∞]> | <url()> | < basic-shape > | blur( <length>? ) \
                 | none"
            ),
            [
                Reference::Property("font-style"),
                Reference::Type("number"),
                Reference::Function("url"),
                Reference::Type("basic-shape"),
                Reference::Function("blur"),
                Reference::Type("length"),
            ]
        );
    }

    #[test]
    fn collect_follows_definitions_transitively_and_skips_unknowns() {
        let syntaxes_of = |reference: Reference<'_>| match reference {
            Reference::Type("clip-source") => vec!["<url>"],
            Reference::Type("geometry-box") => vec!["<shape-box> | fill-box"],
            Reference::Type("shape-box") => vec!["<visual-box> | margin-box"],
            Reference::Type("visual-box") => vec!["content-box | border-box"],
            Reference::Type("basic-shape") => vec!["<rect()> | <circle()>"],
            Reference::Function("rect") => vec!["rect( <length>{4} )", "rect( <top>, <left> )"],
            Reference::Function("circle") => vec!["circle( <radial-size>? )"],
            Reference::Property("outline-style") => vec!["auto | <'border-top-style'>"],
            Reference::Property("border-top-style") => vec!["<line-style>"],
            _ => Vec::new(),
        };
        let definitions = collect(
            [
                "<clip-source> | <geometry-box> | <basic-shape> | none",
                "<'outline-style'>",
            ],
            syntaxes_of,
        );
        assert_eq!(
            definitions
                .types
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "basic-shape",
                "clip-source",
                "geometry-box",
                "shape-box",
                "visual-box"
            ]
        );
        assert_eq!(
            definitions
                .properties
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["border-top-style", "outline-style"]
        );
        assert_eq!(
            definitions.functions.get("rect").map(Vec::as_slice),
            Some(
                &[
                    "rect( <length>{4} )".to_owned(),
                    "rect( <top>, <left> )".to_owned()
                ][..]
            )
        );
        assert!(definitions.functions.contains_key("circle"));
    }
}
