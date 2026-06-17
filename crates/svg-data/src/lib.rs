//! Structured SVG specification data.
//!
//! The catalog is generated at build time from structured data extracted from
//! the canonical SVG specification (fetched fresh by the regeneration step —
//! never from a local checkout). This crate exposes a typed, profile-aware view
//! of that data for the SVG language server and linter: element/attribute
//! lookups, content models, compatibility verdicts, and spec permalinks.

pub mod compat_parse;
pub mod edition;
pub mod inventory;
pub mod profile;
pub mod xlink;

mod catalog;
pub mod types;

pub use types::{
    AttributeApplicability, AttributeDef, AttributeValues, BaselineQualifier, BaselineStatus,
    BrowserFlag, BrowserSupport, BrowserVersion, CompatVerdict, ContentModel, ElementCategory,
    ElementDef, ProfileLookup, ProfiledAttribute, ProfiledElement, SnapshotMetadata, SpecLifecycle,
    SpecSnapshotId, VerdictReason, VerdictRecommendation,
};

use catalog::{ATTRIBUTES, ELEMENTS, SNAPSHOT_METADATA};

/// All snapshots the catalog tracks, oldest first.
#[must_use]
pub const fn spec_snapshots() -> &'static [SpecSnapshotId] {
    &[
        SpecSnapshotId::Svg11Rec20030114,
        SpecSnapshotId::Svg11Rec20110816,
        SpecSnapshotId::Svg2Cr20181004,
        SpecSnapshotId::Svg2EditorsDraft,
    ]
}

/// Look up an element definition by tag name.
#[must_use]
pub fn element(name: &str) -> Option<&'static ElementDef> {
    ELEMENTS.iter().find(|element| element.name == name)
}

/// Look up an attribute definition by (canonical) name.
#[must_use]
pub fn attribute(name: &str) -> Option<&'static AttributeDef> {
    let canonical = xlink::canonical_svg_attribute_name(name);
    ATTRIBUTES
        .iter()
        .find(|attribute| attribute.name == canonical.as_ref())
}

/// All element definitions in the union catalog.
#[must_use]
pub const fn elements() -> &'static [ElementDef] {
    ELEMENTS
}

/// Profile-aware element lookup.
#[must_use]
pub fn element_for_profile(profile: SpecSnapshotId, name: &str) -> ProfileLookup<ElementDef> {
    // Per-profile presence (`UnsupportedInProfile`) is produced once the spec
    // inventory is extracted; until then every known element is present+stable.
    let _ = profile;
    element(name).map_or(ProfileLookup::Unknown, |value| ProfileLookup::Present {
        value,
        lifecycle: SpecLifecycle::Stable,
    })
}

/// Profile-aware attribute lookup.
#[must_use]
pub fn attribute_for_profile(profile: SpecSnapshotId, name: &str) -> ProfileLookup<AttributeDef> {
    let _ = profile;
    attribute(name).map_or(ProfileLookup::Unknown, |value| ProfileLookup::Present {
        value,
        lifecycle: SpecLifecycle::Stable,
    })
}

/// Attributes that apply to `elem_name` in `profile`.
#[must_use]
pub fn attributes_for_with_profile(
    profile: SpecSnapshotId,
    elem_name: &str,
) -> Vec<ProfiledAttribute> {
    let _ = profile;
    let Some(element) = element(elem_name) else {
        return Vec::new();
    };
    ATTRIBUTES
        .iter()
        .filter(|attribute| {
            attribute
                .applicability
                .includes(elem_name, element.global_attrs)
        })
        .map(|attribute| ProfiledAttribute {
            attribute,
            lifecycle: SpecLifecycle::Stable,
        })
        .collect()
}

/// Concrete child elements allowed inside `parent` in `profile`.
#[must_use]
pub fn allowed_children_with_profile(
    profile: SpecSnapshotId,
    parent_name: &str,
) -> Vec<ProfiledElement> {
    let _ = profile;
    let Some(parent) = element(parent_name) else {
        return Vec::new();
    };
    allowed_child_names(&parent.content_model)
        .into_iter()
        .filter_map(element)
        .map(|element| ProfiledElement {
            element,
            lifecycle: SpecLifecycle::Stable,
        })
        .collect()
}

/// Whether `parent` hosts foreign-namespace (e.g. HTML) children.
#[must_use]
pub fn allows_foreign_children(parent_name: &str) -> bool {
    element(parent_name)
        .is_some_and(|element| matches!(element.content_model, ContentModel::Foreign))
}

/// The compat verdict for an element in a profile, when one was derived.
#[must_use]
pub fn compat_verdict_for_element(
    element: &ElementDef,
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    verdict_for(element.verdicts, profile)
}

/// The compat verdict for an attribute in a profile, when one was derived.
#[must_use]
pub fn compat_verdict_for_attribute(
    attribute: &AttributeDef,
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    verdict_for(attribute.verdicts, profile)
}

/// Resolve a `version="…"` attribute value to a snapshot by major family.
#[must_use]
pub fn snapshot_for_svg_version_attr(version: &str) -> Option<SpecSnapshotId> {
    match version.trim().split('.').next().unwrap_or_default() {
        "1" => Some(SpecSnapshotId::Svg11Rec20110816),
        "2" => Some(SpecSnapshotId::Svg2EditorsDraft),
        _ => None,
    }
}

/// Resolve a `version="…"` attribute value to an edition id.
#[must_use]
pub fn edition_for_svg_version_attr(version: &str) -> Option<inventory::EditionId> {
    snapshot_for_svg_version_attr(version).map(inventory::EditionId::for_snapshot)
}

/// Metadata (aliases, …) for a snapshot.
#[must_use]
pub fn snapshot_metadata(snapshot: SpecSnapshotId) -> SnapshotMetadata {
    SNAPSHOT_METADATA
        .iter()
        .find(|metadata| metadata.snapshot == snapshot)
        .cloned()
        .unwrap_or(SnapshotMetadata {
            snapshot,
            aliases: &[],
        })
}

/// Resolve a requested profile string (id or alias) to a snapshot.
#[must_use]
pub fn resolve_profile_id(requested: &str) -> Option<SpecSnapshotId> {
    spec_snapshots().iter().copied().find(|snapshot| {
        snapshot.as_str() == requested || snapshot_metadata(*snapshot).aliases.contains(&requested)
    })
}

/// Resolve a requested edition string to an edition id.
#[must_use]
pub fn resolve_edition_id(requested: &str) -> Option<inventory::EditionId> {
    resolve_profile_id(requested).map(inventory::EditionId::for_snapshot)
}

fn verdict_for(
    verdicts: &'static [(SpecSnapshotId, CompatVerdict)],
    profile: SpecSnapshotId,
) -> Option<CompatVerdict> {
    verdicts
        .iter()
        .find_map(|(snapshot, verdict)| (*snapshot == profile).then_some(*verdict))
}

fn allowed_child_names(content_model: &ContentModel) -> Vec<&'static str> {
    match content_model {
        ContentModel::Children {
            categories,
            elements,
        } => {
            let mut names: Vec<&'static str> = categories
                .iter()
                .flat_map(|category| elements_in_category(*category))
                .copied()
                .chain(elements.iter().copied())
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        }
        ContentModel::ChildrenSet(names) => {
            let mut names: Vec<&'static str> = (*names).to_vec();
            names.sort_unstable();
            names.dedup();
            names
        }
        ContentModel::AnySvg => ELEMENTS.iter().map(|element| element.name).collect(),
        ContentModel::Foreign | ContentModel::Void | ContentModel::Text => Vec::new(),
    }
}

const fn elements_in_category(category: ElementCategory) -> &'static [&'static str] {
    let _ = category;
    // Category membership is part of the extracted data; empty until it lands.
    &[]
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn circle_is_catalogued_with_real_content_model() {
        let Some(circle) = element("circle") else {
            panic!("circle missing from catalog");
        };
        assert!(circle.global_attrs, "circle carries core attributes");
        assert!(
            circle.attrs.contains(&"pathLength"),
            "circle has pathLength"
        );
        assert!(circle.spec_url.is_some(), "circle has a spec permalink");

        // The flattened content model resolves to real child elements.
        let children = allowed_children_with_profile(SpecSnapshotId::LATEST, "circle");
        let names: Vec<&str> = children.iter().map(|child| child.element.name).collect();
        assert!(names.contains(&"animate"), "animation members are allowed");
        assert!(names.contains(&"desc"), "descriptive members are allowed");
        assert!(names.contains(&"clipPath"), "explicit children are allowed");
    }

    #[test]
    fn catalog_is_non_empty() {
        assert!(elements().len() >= 60, "the element catalog is populated");
    }

    #[test]
    fn attribute_catalog_distinguishes_global_scoped_and_geometry_attrs() {
        let Some(id) = attribute("id") else {
            panic!("id missing from catalog");
        };
        assert_eq!(id.applicability, AttributeApplicability::Global);

        let Some(href) = attribute("xlink:href") else {
            panic!("href missing from catalog");
        };
        assert_eq!(href.name, "href");
        assert!(matches!(
            href.applicability,
            AttributeApplicability::Elements(elements)
                if elements.contains(&"a") && elements.contains(&"use")
        ));

        let Some(cx) = attribute("cx") else {
            panic!("cx missing from catalog");
        };
        assert_eq!(cx.presentation_attribute, None);
        assert!(matches!(
            cx.applicability,
            AttributeApplicability::Elements(elements)
                if elements.contains(&"circle") && !elements.contains(&"rect")
        ));

        let circle_attrs = attributes_for_with_profile(SpecSnapshotId::LATEST, "circle");
        assert!(
            circle_attrs
                .iter()
                .any(|profiled| profiled.attribute.name == "id")
        );
        assert!(
            circle_attrs
                .iter()
                .any(|profiled| profiled.attribute.name == "cx")
        );

        let rect_attrs = attributes_for_with_profile(SpecSnapshotId::LATEST, "rect");
        assert!(
            !rect_attrs
                .iter()
                .any(|profiled| profiled.attribute.name == "cx")
        );
    }

    #[test]
    fn foreign_object_hosts_foreign_content() {
        let Some(foreign_object) = element("foreignObject") else {
            panic!("foreignObject missing from catalog");
        };
        assert!(
            matches!(foreign_object.content_model, ContentModel::Foreign),
            "the spec's `any` content model maps to Foreign"
        );
        assert!(allows_foreign_children("foreignObject"));
        // A regular element is not a foreign host.
        assert!(!allows_foreign_children("circle"));
    }
}
