//! Resolve `see [name]` / `see below` cross-references between property
//! definitions.
//!
//! Some SVG property definitions state their value space by pointing at another
//! definition (`see 'fill'`) rather than restating the grammar. This resolves
//! those pointers to the referenced definition's keywords so downstream value
//! derivation sees a concrete value space.

use std::collections::BTreeMap;

use crate::chapter::PropertyValueDef;

pub(super) fn attribute_href_fragment(href: &str) -> Option<&str> {
    href.rsplit_once('#')
        .map_or(Some(href), |(_, fragment)| Some(fragment))
}

/// Parse a spec "see-reference" value (`(see <attr> attribute)`) into the
/// referenced attribute name.
///
/// Some `<dfn>`s define a value space by deferring to another attribute's
/// grammar in prose, e.g. `in2 = "(see in attribute)"`. The extraction in
/// [`chapter.rs`](crate::chapter) faithfully captures that prose, leaving the
/// cross-reference to be resolved here against the full attribute set.
///
/// The match is shape-based (case- and whitespace-insensitive), never keyed on
/// a specific attribute name: any value of the form `(see NAME attribute)`,
/// where `NAME` is an attribute identifier (`[A-Za-z][A-Za-z0-9_:-]*`),
/// resolves to `NAME`. Returns `None` for anything else (real grammars,
/// alternations, malformed prose).
pub(super) fn parse_see_reference(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix('(')?.strip_suffix(')')?.trim();
    let (prefix, rest) = inner.split_once(char::is_whitespace)?;
    if !prefix.eq_ignore_ascii_case("see") {
        return None;
    }
    let mut tokens = rest.split_whitespace();
    let name = tokens.next()?;
    // The closing keyword must be exactly `attribute` and nothing may follow.
    match tokens.next() {
        Some(tail) if tail.eq_ignore_ascii_case("attribute") && tokens.next().is_none() => {}
        _ => return None,
    }
    let mut chars = name.chars();
    let first_is_alpha = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic());
    let rest_is_ident = name
        .chars()
        .skip(1)
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | ':' | '-'));
    if first_is_alpha && rest_is_ident {
        Some(name)
    } else {
        None
    }
}

/// Select the canonical `(value, keywords)` of a grouped attribute definition,
/// mirroring the canonical-def selection in [`resolve_property_values`]: prefer
/// the first unscoped definition, else the bearer-scoped definition whose
/// element name sorts first.
fn canonical_def_value_keywords(
    defs: &[PropertyValueDef],
) -> Option<(Option<String>, Vec<String>)> {
    let unscoped = defs.iter().find(|def| def.dfn_for.is_none());
    let canonical = unscoped.or_else(|| {
        defs.iter()
            .filter(|def| def.dfn_for.is_some())
            .min_by(|left, right| left.dfn_for.cmp(&right.dfn_for))
    })?;
    Some((canonical.value.clone(), canonical.keywords.clone()))
}

/// Resolve spec "see-references" (`(see <attr> attribute)`) by inheriting the
/// referenced attribute's value space, returning an owned, rewritten copy of
/// the input definitions.
///
/// Extraction keeps such cross-references as faithful prose; resolution needs
/// the whole attribute set (the referenced grammar), so it runs here once the
/// definitions are grouped by name. A bounded fixpoint resolves reference
/// chains (`a -> b -> concrete`) while a visited set keeps cycles safe: a
/// reference is only rewritten when its target's value is itself concrete (not
/// another unresolved see-reference), and iteration stops once no definition
/// changes. Dangling or cyclic references are left as their original prose,
/// never fabricated.
pub(super) fn resolve_see_references(properties: &[PropertyValueDef]) -> Vec<PropertyValueDef> {
    let mut resolved: Vec<PropertyValueDef> = properties.to_vec();
    // Bound the fixpoint by the definition count: each pass resolves at least
    // one more link of any acyclic chain, so chains can be at most this long.
    for _ in 0..resolved.len() {
        // Snapshot each name's canonical `(value, keywords)` into an owned map so
        // the rewrite below can mutate `resolved` without aliasing the lookup.
        let mut groups: BTreeMap<&str, Vec<PropertyValueDef>> = BTreeMap::new();
        for def in &resolved {
            groups
                .entry(def.name.as_str())
                .or_default()
                .push(def.clone());
        }
        let canonical: BTreeMap<String, (Option<String>, Vec<String>)> = groups
            .into_iter()
            .filter_map(|(name, defs)| {
                canonical_def_value_keywords(&defs).map(|values| (name.to_owned(), values))
            })
            .collect();
        let mut changed = false;
        for def in &mut resolved {
            let Some(target) = def.value.as_deref().and_then(parse_see_reference) else {
                continue;
            };
            let Some((target_value, target_keywords)) = canonical.get(target) else {
                continue;
            };
            // Only inherit from a concrete target; deferring to another
            // see-reference (or to nothing) is left for a later pass or, if it
            // never concretizes (cycle/dangling), left as prose.
            let target_is_reference = target_value
                .as_deref()
                .is_some_and(|value| parse_see_reference(value).is_some());
            if target_value.is_none() || target_is_reference {
                continue;
            }
            def.value = target_value.clone();
            def.keywords = target_keywords.clone();
            changed = true;
        }
        if !changed {
            break;
        }
    }
    resolved
}
