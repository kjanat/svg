use std::collections::HashMap;

use svg_tree::find_ancestor_any;
use tree_sitter::Tree;

use crate::{
    DefinitionTarget,
    occurrences::{OccurrenceRole, SymbolOccurrence, collect_symbol_occurrences},
};

/// An id reference paired with the tag name of the element it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdReferenceTarget {
    /// The reference.
    pub reference: SymbolOccurrence,
    /// Tag name of the first element declaring the id.
    pub element_name: String,
}

/// Resolve every local id reference to the element that declares the id; unresolved
/// references are left out.
#[must_use]
pub fn resolve_id_references(source: &[u8], tree: &Tree) -> Vec<IdReferenceTarget> {
    let occurrences = collect_symbol_occurrences(source, tree);
    let mut declared: HashMap<&str, String> = HashMap::new();
    for occurrence in &occurrences {
        if occurrence.role != OccurrenceRole::Declaration {
            continue;
        }
        let DefinitionTarget::Id(name) = &occurrence.target else {
            continue;
        };
        if declared.contains_key(name.as_str()) {
            continue;
        }
        let element_name = tree
            .root_node()
            .descendant_for_byte_range(occurrence.start_byte, occurrence.end_byte)
            .and_then(|node| find_ancestor_any(node, &["start_tag", "self_closing_tag"]))
            .and_then(|tag| tag.child_by_field_name("name"))
            .and_then(|name| name.utf8_text(source).ok());
        if let Some(element_name) = element_name {
            declared.insert(name, element_name.to_owned());
        }
    }
    occurrences
        .iter()
        .filter(|occurrence| occurrence.role == OccurrenceRole::Reference)
        .filter_map(|occurrence| {
            let DefinitionTarget::Id(name) = &occurrence.target else {
                return None;
            };
            declared
                .get(name.as_str())
                .map(|element_name| IdReferenceTarget {
                    reference: occurrence.clone(),
                    element_name: element_name.clone(),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use tree_sitter::Parser;

    use super::*;

    fn parse_svg(source: &str) -> Result<Tree, Box<dyn Error>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
        parser
            .parse(source, None)
            .ok_or_else(|| "parse returned None".into())
    }

    #[test]
    fn resolves_references_to_declaring_element() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><defs><linearGradient id=\"g\"/><clipPath id=\"c\"/></defs>",
            "<rect fill=\"url(#g)\" clip-path=\"url(#c)\"/><use href=\"#missing\"/>",
            "<style>#g { }</style></svg>",
        );
        let tree = parse_svg(source)?;
        let resolved = resolve_id_references(source.as_bytes(), &tree);
        let summary: Vec<_> = resolved
            .iter()
            .map(|target| {
                (
                    &source[target.reference.start_byte..target.reference.end_byte],
                    target.element_name.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("g", "linearGradient"),
                ("c", "clipPath"),
                ("g", "linearGradient")
            ]
        );
        Ok(())
    }
}
