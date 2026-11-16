use tower_lsp_server::ls_types::{Location, Position, Uri};

use crate::{
    DocumentState,
    positions::{LineIndex, byte_offset_for_position, named_span_location},
    references::span_range,
    stylesheets::CachedStylesheet,
};

pub struct DefinitionContext {
    pub target: svg_references::DefinitionTarget,
    pub inline_locations: Vec<Location>,
    pub stylesheet_hrefs: Vec<String>,
}

pub fn build_definition_context(
    uri: &Uri,
    pos: Position,
    doc: &DocumentState,
) -> Option<DefinitionContext> {
    let source = doc.source.as_bytes();
    let byte_offset = byte_offset_for_position(source, pos);
    let occurrences = svg_references::collect_symbol_occurrences(source, &doc.tree);
    let target = svg_references::symbol_occurrence_at(&occurrences, byte_offset)
        .map(|occurrence| occurrence.target.clone())
        .or_else(|| svg_references::definition_target_at(source, &doc.tree, byte_offset))?;
    let index = LineIndex::new(source);
    let inline_locations = occurrences
        .iter()
        .filter(|occurrence| {
            occurrence.target == target
                && occurrence.role == svg_references::OccurrenceRole::Declaration
        })
        .map(|occurrence| Location::new(uri.clone(), span_range(&index, source, &occurrence.span)))
        .collect();
    let stylesheet_hrefs = match target {
        svg_references::DefinitionTarget::Id(_) => Vec::new(),
        svg_references::DefinitionTarget::Class(_)
        | svg_references::DefinitionTarget::CustomProperty(_) => {
            svg_references::extract_stylesheet_hrefs(source, &doc.tree)
        }
    };

    Some(DefinitionContext {
        target,
        inline_locations,
        stylesheet_hrefs,
    })
}

pub fn stylesheet_definition_locations(
    stylesheet: &CachedStylesheet,
    target: &svg_references::DefinitionTarget,
) -> Vec<Location> {
    match target {
        svg_references::DefinitionTarget::Class(target_class) => stylesheet
            .class_definitions
            .iter()
            .filter(|definition| definition.name == *target_class)
            .map(|definition| {
                named_span_location(
                    stylesheet.uri.clone(),
                    stylesheet.source.as_bytes(),
                    definition,
                )
            })
            .collect(),
        svg_references::DefinitionTarget::CustomProperty(target_property) => stylesheet
            .custom_property_definitions
            .iter()
            .filter(|definition| definition.name == *target_property)
            .map(|definition| {
                named_span_location(
                    stylesheet.uri.clone(),
                    stylesheet.source.as_bytes(),
                    definition,
                )
            })
            .collect(),
        svg_references::DefinitionTarget::Id(_) => Vec::new(),
    }
}
