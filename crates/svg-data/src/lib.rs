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
    attribute_by_catalog_name(canonical.as_ref())
}

fn attribute_by_catalog_name(name: &str) -> Option<&'static AttributeDef> {
    ATTRIBUTES.iter().find(|attribute| attribute.name == name)
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
    if let Some(lookup) = href_lookup_for_profile(profile, name) {
        return lookup;
    }
    attribute(name).map_or(ProfileLookup::Unknown, attribute_lookup_present)
}

const fn attribute_lookup_present(value: &'static AttributeDef) -> ProfileLookup<AttributeDef> {
    if matches!(value.applicability, AttributeApplicability::None) {
        return ProfileLookup::Unknown;
    }
    ProfileLookup::Present {
        value,
        lifecycle: SpecLifecycle::Stable,
    }
}

fn href_lookup_for_profile(
    profile: SpecSnapshotId,
    name: &str,
) -> Option<ProfileLookup<AttributeDef>> {
    match (name, is_svg11_profile(profile)) {
        ("href", true) => {
            attribute_by_catalog_name("href").map(|_| ProfileLookup::UnsupportedInProfile {
                known_in: SVG2_HREF_SNAPSHOTS,
            })
        }
        ("xlink:href", true) => attribute_by_catalog_name("href").map(attribute_lookup_present),
        ("xlink:href", false) => {
            attribute_by_catalog_name("href").map(|_| ProfileLookup::UnsupportedInProfile {
                known_in: SVG11_XLINK_HREF_SNAPSHOTS,
            })
        }
        _ => None,
    }
}

/// Attributes that apply to `elem_name` in `profile`.
#[must_use]
pub fn attributes_for_with_profile(
    profile: SpecSnapshotId,
    elem_name: &str,
) -> Vec<ProfiledAttribute> {
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
            name: attribute_name_for_profile(profile, attribute.name),
            attribute,
            lifecycle: SpecLifecycle::Stable,
        })
        .collect()
}

fn attribute_name_for_profile(profile: SpecSnapshotId, name: &'static str) -> &'static str {
    if name == "href" && is_svg11_profile(profile) {
        "xlink:href"
    } else {
        name
    }
}

const SVG11_XLINK_HREF_SNAPSHOTS: &[SpecSnapshotId] = &[
    SpecSnapshotId::Svg11Rec20030114,
    SpecSnapshotId::Svg11Rec20110816,
];

const SVG2_HREF_SNAPSHOTS: &[SpecSnapshotId] = &[
    SpecSnapshotId::Svg2Cr20181004,
    SpecSnapshotId::Svg2EditorsDraft,
];

const fn is_svg11_profile(profile: SpecSnapshotId) -> bool {
    matches!(
        profile,
        SpecSnapshotId::Svg11Rec20030114 | SpecSnapshotId::Svg11Rec20110816
    )
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
        .unwrap_or_else(|| built_in_snapshot_metadata(snapshot))
}

fn built_in_snapshot_metadata(snapshot: SpecSnapshotId) -> SnapshotMetadata {
    let aliases: &'static [&'static str] = match snapshot {
        SpecSnapshotId::Svg11Rec20030114 => {
            &["svg11rec20030114", "svg11-20030114", "svg1.1-20030114"][..]
        }
        SpecSnapshotId::Svg11Rec20110816 => &[
            "svg11",
            "svg1.1",
            "1.1",
            "svg11rec20110816",
            "svg11-20110816",
        ],
        SpecSnapshotId::Svg2Cr20181004 => &["svg2cr", "svg2-cr", "svg2cr20181004", "svg2-20181004"],
        SpecSnapshotId::Svg2EditorsDraft => &[
            "svg2",
            "svg2.0",
            "2",
            "2.0",
            "svg2draft",
            "svg2-draft",
            "latest",
        ],
    };
    SnapshotMetadata { snapshot, aliases }
}

/// Resolve a requested profile string (id or alias) to a snapshot.
#[must_use]
pub fn resolve_profile_id(requested: &str) -> Option<SpecSnapshotId> {
    let requested = requested.trim();
    spec_snapshots().iter().copied().find(|snapshot| {
        snapshot.as_str().eq_ignore_ascii_case(requested)
            || snapshot_metadata(*snapshot)
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(requested))
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
    fn profile_aliases_resolve_without_generated_snapshot_metadata() {
        assert_eq!(
            resolve_profile_id("svg11"),
            Some(SpecSnapshotId::Svg11Rec20110816)
        );
        assert_eq!(
            resolve_profile_id("Svg2Draft"),
            Some(SpecSnapshotId::Svg2EditorsDraft)
        );
        assert_eq!(
            resolve_profile_id("Svg11Rec20030114"),
            Some(SpecSnapshotId::Svg11Rec20030114)
        );
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
    fn href_alias_tracks_svg11_and_svg2_profiles() {
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg11Rec20110816, "href"),
            ProfileLookup::UnsupportedInProfile { known_in }
                if known_in == SVG2_HREF_SNAPSHOTS
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg11Rec20110816, "xlink:href"),
            ProfileLookup::Present { value, lifecycle: SpecLifecycle::Stable }
                if value.name == "href"
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg2EditorsDraft, "xlink:href"),
            ProfileLookup::UnsupportedInProfile { known_in }
                if known_in == SVG11_XLINK_HREF_SNAPSHOTS
        ));
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::Svg2EditorsDraft, "href"),
            ProfileLookup::Present { value, lifecycle: SpecLifecycle::Stable }
                if value.name == "href"
        ));

        let svg11_use_attrs = attributes_for_with_profile(SpecSnapshotId::Svg11Rec20110816, "use");
        assert!(
            svg11_use_attrs
                .iter()
                .any(|profiled| profiled.name == "xlink:href")
        );
        assert!(
            !svg11_use_attrs
                .iter()
                .any(|profiled| profiled.name == "href")
        );
    }

    #[test]
    fn nowhere_supported_attributes_are_not_present_without_element_context() {
        let Some(xlink_title) = attribute("xlink:title") else {
            panic!("xlink:title missing from catalog");
        };
        assert_eq!(xlink_title.applicability, AttributeApplicability::None);
        assert!(matches!(
            attribute_for_profile(SpecSnapshotId::LATEST, "xlink:title"),
            ProfileLookup::Unknown
        ));
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
