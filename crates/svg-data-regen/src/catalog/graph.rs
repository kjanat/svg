//! Build the derived cross-reference graph over the catalog.
//!
//! Nodes are elements, attributes, CSS properties, value grammars, categories,
//! and profiles; edges record membership, applicability, containment, value
//! spaces, per-profile presence, and compat descriptions. The graph is a
//! secondary projection of the primary catalog data, emitted so consumers can
//! traverse relationships without re-deriving them.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    CatalogAttribute, CatalogAttributeApplicability, CatalogAttributeValues, CatalogContentModel,
    CatalogElement, CatalogGraph, CatalogGraphEdge, CatalogGraphEdgeKind, CatalogGraphNode,
    CatalogGraphNodeKind, CatalogInventory, CatalogSpecSnapshotId, canonical_attribute_name,
};
use crate::chapter::PropertyValueDef;
use crate::compat::CompatCatalog;
use crate::extract::Definitions;

pub(super) fn build_catalog_graph(
    modules: &[Definitions],
    properties: &[PropertyValueDef],
    compat: Option<&CompatCatalog>,
    elements: &[CatalogElement],
    attributes: &[CatalogAttribute],
    inventories: &[CatalogInventory],
) -> CatalogGraph {
    let mut builder = CatalogGraphBuilder::default();
    add_graph_profile_nodes(&mut builder);
    add_graph_element_nodes(&mut builder, elements);
    add_graph_attribute_nodes(&mut builder, attributes);
    add_graph_css_property_nodes(&mut builder, modules, properties);
    add_graph_category_edges(&mut builder, modules);
    add_graph_element_edges(&mut builder, elements);
    add_graph_attribute_edges(&mut builder, elements, attributes);
    add_graph_value_edges(&mut builder, attributes);
    add_graph_inventory_edges(&mut builder, inventories);
    add_graph_compat_edges(&mut builder, compat);
    builder.finish()
}

#[derive(Default)]
struct CatalogGraphBuilder {
    nodes: BTreeMap<String, CatalogGraphNode>,
    edges: BTreeSet<CatalogGraphEdge>,
}

impl CatalogGraphBuilder {
    fn node(&mut self, kind: CatalogGraphNodeKind, name: impl Into<String>) -> String {
        let name = name.into();
        let id = catalog_graph_node_id(kind, &name);
        self.node_with_id(kind, id, name)
    }

    fn node_with_id(
        &mut self,
        kind: CatalogGraphNodeKind,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> String {
        let id = id.into();
        let name = name.into();
        self.nodes
            .entry(id.clone())
            .or_insert_with(|| CatalogGraphNode {
                id: id.clone(),
                kind,
                name,
            });
        id
    }

    fn edge(&mut self, from: &str, to: &str, kind: CatalogGraphEdgeKind) {
        self.edges.insert(CatalogGraphEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            kind,
        });
    }

    /// Node an attribute by its canonical name and link it as a member of `category_id`.
    fn attribute_member_of(&mut self, attribute_name: &str, category_id: &str) {
        let attribute_id = self.node(
            CatalogGraphNodeKind::Attribute,
            canonical_attribute_name(attribute_name).as_ref(),
        );
        self.edge(&attribute_id, category_id, CatalogGraphEdgeKind::MemberOf);
    }

    fn finish(self) -> CatalogGraph {
        CatalogGraph {
            nodes: self.nodes.into_values().collect(),
            edges: self.edges.into_iter().collect(),
        }
    }
}

fn catalog_graph_node_id(kind: CatalogGraphNodeKind, name: &str) -> String {
    format!("{}:{name}", catalog_graph_node_prefix(kind))
}

const fn catalog_graph_node_prefix(kind: CatalogGraphNodeKind) -> &'static str {
    match kind {
        CatalogGraphNodeKind::Element => "element",
        CatalogGraphNodeKind::Attribute => "attribute",
        CatalogGraphNodeKind::ElementCategory => "element-category",
        CatalogGraphNodeKind::AttributeCategory => "attribute-category",
        CatalogGraphNodeKind::Profile => "profile",
        CatalogGraphNodeKind::CssProperty => "css-property",
        CatalogGraphNodeKind::ValueGrammar => "value",
        CatalogGraphNodeKind::CompatFeature => "compat",
    }
}

fn add_graph_profile_nodes(builder: &mut CatalogGraphBuilder) {
    for profile in [
        CatalogSpecSnapshotId::Svg11Rec20030114,
        CatalogSpecSnapshotId::Svg11Rec20110816,
        CatalogSpecSnapshotId::Svg2Cr20181004,
        CatalogSpecSnapshotId::Svg2EditorsDraft,
    ] {
        builder.node(CatalogGraphNodeKind::Profile, catalog_profile_name(profile));
    }
}

const fn catalog_profile_name(profile: CatalogSpecSnapshotId) -> &'static str {
    match profile {
        CatalogSpecSnapshotId::Svg11Rec20030114 => "Svg11Rec20030114",
        CatalogSpecSnapshotId::Svg11Rec20110816 => "Svg11Rec20110816",
        CatalogSpecSnapshotId::Svg2Cr20181004 => "Svg2Cr20181004",
        CatalogSpecSnapshotId::Svg2EditorsDraft => "Svg2EditorsDraft",
    }
}

fn add_graph_element_nodes(builder: &mut CatalogGraphBuilder, elements: &[CatalogElement]) {
    for element in elements {
        builder.node(CatalogGraphNodeKind::Element, &element.name);
    }
}

fn add_graph_attribute_nodes(builder: &mut CatalogGraphBuilder, attributes: &[CatalogAttribute]) {
    for attribute in attributes {
        builder.node(CatalogGraphNodeKind::Attribute, &attribute.name);
    }
}

fn add_graph_css_property_nodes(
    builder: &mut CatalogGraphBuilder,
    modules: &[Definitions],
    properties: &[PropertyValueDef],
) {
    for property in properties {
        builder.node(CatalogGraphNodeKind::CssProperty, &property.name);
    }
    for module in modules {
        for property in &module.properties {
            builder.node(CatalogGraphNodeKind::CssProperty, &property.name);
        }
    }
}

fn add_graph_category_edges(builder: &mut CatalogGraphBuilder, modules: &[Definitions]) {
    let global_category = builder.node(CatalogGraphNodeKind::AttributeCategory, "global");
    for module in modules {
        for attribute in &module.global_attributes {
            builder.attribute_member_of(&attribute.name, &global_category);
        }
        add_graph_element_category_edges(builder, module);
        add_graph_attribute_category_edges(builder, module);
    }
}

fn add_graph_element_category_edges(builder: &mut CatalogGraphBuilder, module: &Definitions) {
    for category in &module.element_categories {
        let category_id = builder.node(CatalogGraphNodeKind::ElementCategory, &category.name);
        for element in &category.elements {
            let element_id = builder.node(CatalogGraphNodeKind::Element, element);
            builder.edge(&element_id, &category_id, CatalogGraphEdgeKind::MemberOf);
        }
    }
}

fn add_graph_attribute_category_edges(builder: &mut CatalogGraphBuilder, module: &Definitions) {
    for category in &module.attribute_categories {
        let category_id = builder.node(CatalogGraphNodeKind::AttributeCategory, &category.name);
        for attribute in &category.attributes {
            builder.attribute_member_of(&attribute.name, &category_id);
        }
        for attribute in &category.presentation_attributes {
            builder.attribute_member_of(attribute, &category_id);
        }
    }
}

fn add_graph_element_edges(builder: &mut CatalogGraphBuilder, elements: &[CatalogElement]) {
    let element_names: Vec<&str> = elements
        .iter()
        .map(|element| element.name.as_str())
        .collect();
    let global_category = builder.node(CatalogGraphNodeKind::AttributeCategory, "global");
    for element in elements {
        let element_id = builder.node(CatalogGraphNodeKind::Element, &element.name);
        if element.global_attrs {
            builder.edge(
                &element_id,
                &global_category,
                CatalogGraphEdgeKind::AcceptsGlobalAttributes,
            );
        }
        match &element.content_model {
            CatalogContentModel::ChildrenSet { elements } => {
                for child in elements {
                    let child_id = builder.node(CatalogGraphNodeKind::Element, child);
                    builder.edge(&element_id, &child_id, CatalogGraphEdgeKind::AllowsChild);
                }
            }
            CatalogContentModel::AnySvg => {
                for child in &element_names {
                    let child_id = builder.node(CatalogGraphNodeKind::Element, *child);
                    builder.edge(&element_id, &child_id, CatalogGraphEdgeKind::AllowsChild);
                }
            }
            CatalogContentModel::Foreign | CatalogContentModel::Text => {}
        }
    }
}

fn add_graph_attribute_edges(
    builder: &mut CatalogGraphBuilder,
    elements: &[CatalogElement],
    attributes: &[CatalogAttribute],
) {
    for element in elements {
        let element_id = builder.node(CatalogGraphNodeKind::Element, &element.name);
        for attribute in &element.attrs {
            let attribute_id = builder.node(CatalogGraphNodeKind::Attribute, attribute);
            builder.edge(
                &element_id,
                &attribute_id,
                CatalogGraphEdgeKind::HasAttribute,
            );
        }
    }
    for attribute in attributes {
        let attribute_id = builder.node(CatalogGraphNodeKind::Attribute, &attribute.name);
        add_graph_attribute_applicability_edges(builder, &attribute_id, attribute, elements);
        if let Some(property) = attribute.presentation_attribute.as_deref() {
            let property_id = builder.node(CatalogGraphNodeKind::CssProperty, property);
            builder.edge(
                &attribute_id,
                &property_id,
                CatalogGraphEdgeKind::UsesCssProperty,
            );
        }
    }
}

fn add_graph_attribute_applicability_edges(
    builder: &mut CatalogGraphBuilder,
    attribute_id: &str,
    attribute: &CatalogAttribute,
    elements: &[CatalogElement],
) {
    match &attribute.applicability {
        CatalogAttributeApplicability::Global => {
            for element in elements.iter().filter(|element| element.global_attrs) {
                let element_id = builder.node(CatalogGraphNodeKind::Element, &element.name);
                builder.edge(attribute_id, &element_id, CatalogGraphEdgeKind::AppliesTo);
            }
        }
        CatalogAttributeApplicability::Elements { elements } => {
            for element in elements {
                let element_id = builder.node(CatalogGraphNodeKind::Element, element);
                builder.edge(attribute_id, &element_id, CatalogGraphEdgeKind::AppliesTo);
            }
        }
        CatalogAttributeApplicability::None => {}
    }
}

fn add_graph_value_edges(builder: &mut CatalogGraphBuilder, attributes: &[CatalogAttribute]) {
    for attribute in attributes {
        let attribute_id = builder.node(CatalogGraphNodeKind::Attribute, &attribute.name);
        let value_id = builder.node_with_id(
            CatalogGraphNodeKind::ValueGrammar,
            catalog_graph_node_id(CatalogGraphNodeKind::ValueGrammar, &attribute.name),
            format!(
                "{} ({})",
                attribute.name,
                catalog_attribute_values_kind(&attribute.values)
            ),
        );
        builder.edge(
            &attribute_id,
            &value_id,
            CatalogGraphEdgeKind::HasValueGrammar,
        );
        for override_ in &attribute.value_overrides {
            let profile = catalog_profile_name(override_.profile);
            let override_key = format!("{}@{profile}", attribute.name);
            let override_value_id = builder.node_with_id(
                CatalogGraphNodeKind::ValueGrammar,
                catalog_graph_node_id(CatalogGraphNodeKind::ValueGrammar, &override_key),
                format!(
                    "{}@{} ({})",
                    attribute.name,
                    profile,
                    catalog_attribute_values_kind(&override_.values)
                ),
            );
            let profile_id = builder.node(CatalogGraphNodeKind::Profile, profile);
            builder.edge(
                &attribute_id,
                &override_value_id,
                CatalogGraphEdgeKind::HasValueGrammar,
            );
            builder.edge(
                &override_value_id,
                &profile_id,
                CatalogGraphEdgeKind::OverridesValueInProfile,
            );
        }
    }
}

const fn catalog_attribute_values_kind(values: &CatalogAttributeValues) -> &'static str {
    match values {
        CatalogAttributeValues::Enum { .. } => "enum",
        CatalogAttributeValues::Transform { .. } => "transform",
        CatalogAttributeValues::Color => "color",
        CatalogAttributeValues::Paint => "paint",
        CatalogAttributeValues::Length => "length",
        CatalogAttributeValues::Url => "url",
        CatalogAttributeValues::Iri => "iri",
        CatalogAttributeValues::Boolean => "boolean",
        CatalogAttributeValues::TokenList => "token_list",
        CatalogAttributeValues::CommaTokenList => "comma_token_list",
        CatalogAttributeValues::UrlTokenList => "url_token_list",
        CatalogAttributeValues::LanguageTag => "language_tag",
        CatalogAttributeValues::Integer => "integer",
        CatalogAttributeValues::MediaType => "media_type",
        CatalogAttributeValues::MediaQueryList => "media_query_list",
        CatalogAttributeValues::CssDeclarationList => "css_declaration_list",
        CatalogAttributeValues::Id => "id",
        CatalogAttributeValues::ReferrerPolicy => "referrer_policy",
        CatalogAttributeValues::SuggestedFileName => "suggested_file_name",
        CatalogAttributeValues::PathData => "path_data",
        CatalogAttributeValues::SemicolonNumberList => "semicolon_number_list",
        CatalogAttributeValues::CoordinatePair => "coordinate_pair",
        CatalogAttributeValues::CoordinatePairList => "coordinate_pair_list",
        CatalogAttributeValues::NumberOrPercentage => "number_or_percentage",
        CatalogAttributeValues::Number => "number",
        CatalogAttributeValues::IdList => "id_list",
        CatalogAttributeValues::CssGrammar { .. } => "css_grammar",
        CatalogAttributeValues::FreeText => "free_text",
        CatalogAttributeValues::Unresolved => "unresolved",
    }
}

fn add_graph_compat_edges(builder: &mut CatalogGraphBuilder, compat: Option<&CompatCatalog>) {
    let Some(compat) = compat else {
        return;
    };
    for feature in &compat.provenance.unmodeled_features {
        let feature_id = builder.node(CatalogGraphNodeKind::CompatFeature, &feature.compat_key);
        let element_id = builder.node(CatalogGraphNodeKind::Element, &feature.element);
        builder.edge(&feature_id, &element_id, CatalogGraphEdgeKind::Describes);
        if !feature.name.is_empty() {
            let attribute_id = builder.node(CatalogGraphNodeKind::Attribute, &feature.name);
            builder.edge(&feature_id, &attribute_id, CatalogGraphEdgeKind::Describes);
        }
    }
}

fn add_graph_inventory_edges(builder: &mut CatalogGraphBuilder, inventories: &[CatalogInventory]) {
    for inventory in inventories {
        let profile_id = builder.node(
            CatalogGraphNodeKind::Profile,
            catalog_profile_name(inventory.profile),
        );
        for element in &inventory.elements {
            let element_id = builder.node(CatalogGraphNodeKind::Element, &element.name);
            builder.edge(&element_id, &profile_id, CatalogGraphEdgeKind::PresentIn);
        }
        for attribute in &inventory.attributes {
            let attribute_id = builder.node(
                CatalogGraphNodeKind::Attribute,
                canonical_attribute_name(attribute).as_ref(),
            );
            builder.edge(&attribute_id, &profile_id, CatalogGraphEdgeKind::PresentIn);
        }
    }
}
