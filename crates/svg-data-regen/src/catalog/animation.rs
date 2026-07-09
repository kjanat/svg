//! Derive an attribute's animation tri-state (`NotAnimatable` / `Discrete` /
//! `Additive`).
//!
//! Reconciles the propdef's stated `Animatable:` designation (SVG 1.1 and the
//! SVG 2 / Web Animations vocabulary) with the additive signal inferable from
//! the value kind, so a bare `yes` still yields the correct additive-vs-discrete
//! classification.

use super::{CatalogAnimation, CatalogAttributeValues};
use crate::chapter::PropertyValueDef;

/// Map a propdef's stated animation designation to the tri-state. Handles both
/// the SVG 2 / Web Animations vocabulary (`by computed value`, `discrete`, `not
/// animatable`) and the SVG 1.1 form (`yes`, `no`, `yes (non-additive)`).
///
/// Returns `None` for a bare `yes`: SVG 1.1's `Animatable: yes` states only that
/// the value animates, not whether additively, so the additive distinction must
/// come from the value kind (a `<length>` interpolates; a token list does not).
fn animation_from_type_string(animation_type: &str) -> Option<CatalogAnimation> {
    let animation_type = animation_type.trim().to_ascii_lowercase();
    if matches!(
        animation_type.as_str(),
        "" | "no" | "none" | "not animatable"
    ) {
        return Some(CatalogAnimation::NotAnimatable);
    }
    if animation_type == "discrete" || animation_type.contains("non-additive") {
        return Some(CatalogAnimation::Discrete);
    }
    if animation_type == "yes" {
        return None;
    }
    // Interpolable designations: the Web Animations `by computed value` family
    // and the CSS `as <type>` forms (`as color`, `as length`, `as font weight`).
    Some(CatalogAnimation::Additive)
}

/// Classify animation behaviour from the resolved value space, per the SVG
/// Animations §2.17 by-data-type table. Used for attributes whose defining spec
/// states no explicit animation designation — chiefly presentation attributes
/// SVG delegates to CSS (`opacity`, `color`, `font-*`), whose value space is
/// only known after the derived resolution.
pub(super) fn animation_from_value_kind(values: &CatalogAttributeValues) -> CatalogAnimation {
    use CatalogAttributeValues as V;
    match values {
        // SVG Animations additive types: angle, color, integer, length, number,
        // paint, percentage — plus the numeric/geometric lists and path data,
        // which interpolate component-wise.
        V::Color
        | V::Paint
        | V::Length
        | V::Number
        | V::Integer
        | V::NumberOrPercentage
        | V::Transform { .. }
        | V::CoordinatePair
        | V::CoordinatePairList
        | V::SemicolonNumberList
        | V::PathData => CatalogAnimation::Additive,
        // Animatable but non-additive: keyword/enumerated, boolean, reference,
        // list-of-token and URL types animate by discrete swap (SVG Animations:
        // URL is `yes` but non-additive; "all other data types" animate via
        // `set`).
        V::Enum { .. }
        | V::Boolean
        | V::TokenList
        | V::CommaTokenList
        | V::UrlTokenList
        | V::Url
        | V::Iri
        | V::Id
        | V::IdList
        | V::LanguageTag
        | V::MediaType
        | V::MediaQueryList
        | V::ReferrerPolicy
        | V::SuggestedFileName
        | V::CssDeclarationList => CatalogAnimation::Discrete,
        // A CSS grammar that admits a numeric/dimensional/colour value
        // interpolates (SVG Animations additive types), e.g. `font-size`
        // (`<length-percentage>`); a purely keyword grammar animates discretely.
        V::CssGrammar { grammar, .. } => {
            if grammar_admits_interpolable_type(grammar) {
                CatalogAnimation::Additive
            } else {
                CatalogAnimation::Discrete
            }
        }
        // Genuinely unconstrained or not-yet-modelled: no defined animation
        // behaviour.
        V::FreeText | V::Unresolved => CatalogAnimation::NotAnimatable,
    }
}

/// Whether a CSS value grammar references an interpolable numeric/dimensional/
/// colour type (per the SVG Animations §2.17 additive types). `<time>` and
/// `<frequency>` are excluded — those are not animatable.
fn grammar_admits_interpolable_type(grammar: &str) -> bool {
    [
        "<length",
        "<number",
        "<percentage",
        "<integer",
        "<angle",
        "<color",
    ]
    .iter()
    .any(|type_ref| grammar.contains(type_ref))
}

/// Animation for an attribute already known to be animatable (an explicit
/// `Animatable: yes`), classified additive-vs-discrete from its value kind. A
/// value space that carries no additive signal (or is unresolved) is `Discrete`
/// rather than `NotAnimatable` — the explicit designation is authoritative.
fn known_animatable_from_value_kind(values: &CatalogAttributeValues) -> CatalogAnimation {
    match animation_from_value_kind(values) {
        CatalogAnimation::Additive => CatalogAnimation::Additive,
        CatalogAnimation::Discrete | CatalogAnimation::NotAnimatable => CatalogAnimation::Discrete,
    }
}

/// Resolve an attribute's animation behaviour from the strongest available
/// signal: an explicit `Animatable: no` (SVG definitions), then the propdef's
/// stated animation type, then the value-space fallback for value-bearing
/// attributes the spec designates animatable or delegates to CSS.
pub(super) fn resolve_animation(
    explicit: Option<bool>,
    property: Option<&PropertyValueDef>,
    values: &CatalogAttributeValues,
) -> CatalogAnimation {
    if explicit == Some(false) {
        return CatalogAnimation::NotAnimatable;
    }
    if let Some(animation_type) = property.and_then(|property| property.animation_type.as_deref()) {
        // A definite designation wins; a bare `yes` defers to the value kind but,
        // being an explicit "animatable", is never demoted to non-animatable.
        return animation_from_type_string(animation_type)
            .unwrap_or_else(|| known_animatable_from_value_kind(values));
    }
    if explicit == Some(true) {
        return known_animatable_from_value_kind(values);
    }
    // No explicit designation and no local propdef: presentation attributes SVG
    // delegates to CSS are still animatable (their value space, resolved later,
    // drives the additive distinction); everything else stays non-animatable.
    CatalogAnimation::NotAnimatable
}
