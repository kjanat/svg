//! Map the extracted spec data into the committed catalog JSON that
//! `svg-data`'s `build.rs` turns into static `ElementDef` arrays.
//!
//! The extraction types mirror the spec's own shapes; this module reshapes them
//! into the runtime catalog's vocabulary. The structural content model is
//! flattened here (categories resolved to their member elements, unioned with
//! the explicit allowed elements) so the runtime never depends on a category
//! enum staying in sync with the spec's taxonomy. The output is sorted, so the
//! same upstream commit always yields byte-identical JSON.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::extract::{ContentModelKind, Definitions};

/// The full derived catalog written to `svg-data/data/catalog.json`.
#[derive(Debug, Serialize)]
pub struct Catalog {
    /// The upstream commit this catalog was derived from.
    pub commit: String,
    /// Element definitions, sorted by name.
    pub elements: Vec<CatalogElement>,
}

/// One element's spec-derived catalog entry.
#[derive(Debug, Serialize)]
pub struct CatalogElement {
    /// Element tag name.
    pub name: String,
    /// Resolved spec permalink (the module's anchor base joined with the href).
    pub spec_url: Option<String>,
    /// Structural child-content model.
    pub content_model: CatalogContentModel,
    /// Element-specific attribute names (sorted, deduped).
    pub attrs: Vec<String>,
    /// Whether the element carries the SVG global (`core`) attributes.
    pub global_attrs: bool,
}

/// The runtime content-model shapes the catalog emits. The spec's category
/// taxonomy is already flattened into [`Self::ChildrenSet`].
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogContentModel {
    /// Accepts exactly the listed child element names.
    ChildrenSet {
        /// Allowed child element names (sorted, deduped).
        elements: Vec<String>,
    },
    /// Accepts any element from the SVG namespace.
    AnySvg,
    /// Primarily character data.
    Text,
}

/// Build the catalog from every definitions module's extracted entities.
///
/// `editors_draft_base` is the SVG 2 editor's-draft base URL (from
/// `publish.xml`'s `<cvs>`), used to resolve permalinks for the modules defined
/// within `svgwg` itself; modules carrying their own external `anchor_base`
/// (CSS drafts) resolve against that instead.
#[must_use]
pub fn build_catalog(modules: &[Definitions], editors_draft_base: &str, commit: &str) -> Catalog {
    let members = category_members(modules);
    let mut elements = Vec::new();
    for module in modules {
        let base = module.anchor_base.as_deref().unwrap_or(editors_draft_base);
        for element in &module.elements {
            elements.push(build_element(element, base, &members));
        }
    }
    elements.sort_by(|a, b| a.name.cmp(&b.name));
    Catalog {
        commit: commit.to_owned(),
        elements,
    }
}

/// Element-category membership across all modules: category name to its member
/// element names.
fn category_members(modules: &[Definitions]) -> BTreeMap<&str, Vec<&str>> {
    let mut members: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for module in modules {
        for category in &module.element_categories {
            let entry = members.entry(category.name.as_str()).or_default();
            entry.extend(category.elements.iter().map(String::as_str));
        }
    }
    members
}

/// Reshape one extracted element into its catalog entry.
fn build_element(
    element: &crate::extract::ElementDef,
    base: &str,
    members: &BTreeMap<&str, Vec<&str>>,
) -> CatalogElement {
    let content_model = match element.content_model {
        Some(ContentModelKind::Text) => CatalogContentModel::Text,
        Some(ContentModelKind::AnyOf | ContentModelKind::TextOrAnyOf) => {
            CatalogContentModel::ChildrenSet {
                elements: flatten_children(element, members),
            }
        }
        // `any`, and description-only models (e.g. `a`, whose children mirror the
        // parent): over-approximate as "any SVG element" so valid children never
        // trip a false "invalid child" diagnostic.
        Some(ContentModelKind::Any) | None => CatalogContentModel::AnySvg,
    };

    let mut attrs: Vec<String> = element
        .attributes
        .iter()
        .map(|attribute| attribute.name.clone())
        .chain(element.common_attributes.iter().cloned())
        .collect();
    attrs.sort();
    attrs.dedup();

    CatalogElement {
        name: element.name.clone(),
        spec_url: element.href.as_ref().map(|href| format!("{base}{href}")),
        content_model,
        attrs,
        global_attrs: element
            .attribute_categories
            .iter()
            .any(|category| category == "core"),
    }
}

/// Flatten an element's allowed child categories (resolved to their members)
/// unioned with its explicit allowed elements, sorted and deduped.
fn flatten_children(
    element: &crate::extract::ElementDef,
    members: &BTreeMap<&str, Vec<&str>>,
) -> Vec<String> {
    let mut names: Vec<String> = element
        .allowed_element_categories
        .iter()
        .filter_map(|category| members.get(category.as_str()))
        .flatten()
        .map(|name| (*name).to_owned())
        .chain(element.allowed_elements.iter().cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}
