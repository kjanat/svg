//! Derive per-profile lifecycle for the entities in a snapshot.
//!
//! Given a profile and the cross-profile presence inventories, classify each
//! element/attribute as added, removed, or carried-over relative to the
//! neighbouring specification editions, so consumers can reason about when a
//! feature entered or left the platform.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    CatalogAttribute, CatalogInventory, CatalogLifecycleEntry, CatalogLifecycleStatus,
    CatalogSnapshotLifecycle, CatalogSpecSnapshotId, canonical_attribute_name,
};

pub(super) fn derive_snapshot_lifecycle(
    profile: CatalogSpecSnapshotId,
    inventories: &[CatalogInventory],
    attributes: &[CatalogAttribute],
) -> CatalogSnapshotLifecycle {
    let catalog_attribute_names: BTreeSet<&str> = attributes
        .iter()
        .map(|attribute| attribute.name.as_str())
        .collect();
    let element_presence = collect_element_presence(inventories);
    let attribute_presence = collect_attribute_presence(inventories, &catalog_attribute_names);
    CatalogSnapshotLifecycle {
        elements: lifecycle_entries_for_profile(profile, &element_presence, false),
        attributes: lifecycle_entries_for_profile(profile, &attribute_presence, true),
    }
}

fn collect_element_presence(
    inventories: &[CatalogInventory],
) -> BTreeMap<String, Vec<CatalogSpecSnapshotId>> {
    let mut presence: BTreeMap<String, BTreeSet<CatalogSpecSnapshotId>> = BTreeMap::new();
    for inventory in inventories {
        for element in &inventory.elements {
            presence
                .entry(element.name.clone())
                .or_default()
                .insert(inventory.profile);
        }
    }
    presence
        .into_iter()
        .map(|(name, profiles)| (name, profiles.into_iter().collect()))
        .collect()
}

fn collect_attribute_presence(
    inventories: &[CatalogInventory],
    catalog_attribute_names: &BTreeSet<&str>,
) -> BTreeMap<String, Vec<CatalogSpecSnapshotId>> {
    let mut presence: BTreeMap<String, BTreeSet<CatalogSpecSnapshotId>> = BTreeMap::new();
    for inventory in inventories {
        for attribute in &inventory.attributes {
            let Some(attribute) =
                lifecycle_attribute_name(inventory.profile, attribute, catalog_attribute_names)
            else {
                continue;
            };
            presence
                .entry(attribute)
                .or_default()
                .insert(inventory.profile);
        }
    }
    presence
        .into_iter()
        .map(|(name, profiles)| (name, profiles.into_iter().collect()))
        .collect()
}

fn lifecycle_entries_for_profile(
    profile: CatalogSpecSnapshotId,
    presence: &BTreeMap<String, Vec<CatalogSpecSnapshotId>>,
    attributes: bool,
) -> Vec<CatalogLifecycleEntry> {
    let mut entries = Vec::new();
    for (name, known_in) in presence {
        let present = known_in.contains(&profile);
        let catalog_name = attributes
            .then(|| canonical_attribute_name(name))
            .and_then(|canonical| (canonical.as_ref() != name).then(|| canonical.into_owned()));
        let lifecycle = if present {
            if is_draft_only(profile, known_in) {
                Some(CatalogLifecycleStatus::Experimental)
            } else if catalog_name.is_some() {
                Some(CatalogLifecycleStatus::Stable)
            } else {
                None
            }
        } else if known_before(profile, known_in) {
            Some(CatalogLifecycleStatus::Obsolete)
        } else if known_after(profile, known_in) {
            Some(CatalogLifecycleStatus::NotYetIntroduced)
        } else {
            None
        };
        let Some(lifecycle) = lifecycle else {
            continue;
        };
        entries.push(CatalogLifecycleEntry {
            name: name.clone(),
            catalog_name,
            present,
            lifecycle,
            known_in: known_in.clone(),
        });
    }
    entries
}

fn lifecycle_attribute_name(
    profile: CatalogSpecSnapshotId,
    attribute: &str,
    catalog_attribute_names: &BTreeSet<&str>,
) -> Option<String> {
    if attribute == "xlink:href" && !is_svg11_profile(profile) {
        return None;
    }
    let canonical = canonical_attribute_name(attribute);
    (attribute == "xlink:href" || catalog_attribute_names.contains(canonical.as_ref()))
        .then(|| attribute.to_owned())
}

fn is_draft_only(profile: CatalogSpecSnapshotId, known_in: &[CatalogSpecSnapshotId]) -> bool {
    profile == CatalogSpecSnapshotId::Svg2EditorsDraft
        && known_in == [CatalogSpecSnapshotId::Svg2EditorsDraft]
}

fn known_before(profile: CatalogSpecSnapshotId, known_in: &[CatalogSpecSnapshotId]) -> bool {
    known_in.iter().any(|known| *known < profile)
}

fn known_after(profile: CatalogSpecSnapshotId, known_in: &[CatalogSpecSnapshotId]) -> bool {
    known_in.iter().any(|known| *known > profile)
}

const fn is_svg11_profile(profile: CatalogSpecSnapshotId) -> bool {
    matches!(
        profile,
        CatalogSpecSnapshotId::Svg11Rec20030114 | CatalogSpecSnapshotId::Svg11Rec20110816
    )
}
