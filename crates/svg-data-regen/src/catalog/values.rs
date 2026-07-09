//! Recognize an attribute's value space from its CSS/SVG grammar or prose.
//!
//! Turns a property's raw value expression (`<number> | auto`, `see below`,
//! a coordinate-pair grammar, an entity-escaped production reference, …) into
//! the [`CatalogAttributeValues`] subset the catalog models, applying source
//! overrides and token repairs along the way.

use std::collections::BTreeMap;

use super::{
    CatalogAttribute, CatalogAttributeValues, css_grammar_graph, strip_type_range_annotations,
};
use crate::chapter::PropertyValueDef;
use crate::util::{is_keyword_token, normalize_ws};

/// Convert a CSS property grammar into the runtime value-space subset we know
/// how to represent today.
pub(super) fn values_for_property(
    property: &PropertyValueDef,
    descriptions_by_id: &BTreeMap<&str, &str>,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
) -> CatalogAttributeValues {
    if let Some(values) = source_verified_value_override(property) {
        return values;
    }
    let mut keywords = property.keywords.clone();
    keywords.sort();
    keywords.dedup();
    if !keywords.is_empty()
        && property
            .value
            .as_deref()
            .is_some_and(is_keyword_only_grammar)
    {
        return CatalogAttributeValues::Enum { values: keywords };
    }

    let Some(raw_value) = property.value.as_deref() else {
        return CatalogAttributeValues::FreeText;
    };
    // Typographic quotes (`‘`/`’`) wrap CSS property-references like
    // `<‘'opacity'’>`; fold them to ASCII so the byte-oriented token repair
    // below and the reference check both see a clean `<'opacity'>`.
    let raw_value = fold_property_reference_quotes(raw_value);
    let raw_value = strip_type_range_annotations(&raw_value).into_owned();
    let raw_value = raw_value.as_str();
    let grammar = repair_css_type_tokens(raw_value);
    let value = grammar.as_str();
    let normalized = value.to_ascii_lowercase();
    if is_see_below_value(value) {
        return property
            .id
            .as_deref()
            .and_then(|id| descriptions_by_id.get(id).copied())
            .map_or(
                CatalogAttributeValues::FreeText,
                value_from_see_below_description,
            );
    }
    // `<anything>` is the SVG spec's literal "any value" (e.g. `xlink:title`),
    // not a CSS type production — genuinely unconstrained, so free text rather
    // than a grammar with a meaningless `<anything>` type node.
    if normalized == "<anything>" {
        return CatalogAttributeValues::FreeText;
    }
    if let Some(values) = value_from_property_reference(value) {
        return values;
    }
    if let Some(values) = value_from_referenced_syntax(value) {
        return values;
    }
    if is_path_data_value(&normalized) {
        return CatalogAttributeValues::PathData;
    }
    if is_semicolon_number_list_value(&normalized) {
        return CatalogAttributeValues::SemicolonNumberList;
    }
    if is_coordinate_pair_list_value(&normalized) {
        return CatalogAttributeValues::CoordinatePairList;
    }
    if is_coordinate_pair_value(&normalized) {
        return CatalogAttributeValues::CoordinatePair;
    }
    if is_suggested_file_name_prose_value(value) {
        return CatalogAttributeValues::SuggestedFileName;
    }
    // Semantic projection policy: `<transform-list>` currently becomes a typed
    // transform-function list for downstream consumers instead of remaining raw
    // CSS grammar text.
    if (property.name == "transform" || normalized.contains("<transform-list>"))
        && let Some(inputs) = grammar_inputs
    {
        let functions = inputs.transform_functions();
        if !functions.is_empty() {
            return CatalogAttributeValues::Transform { functions };
        }
    }
    if normalized == "<color>" {
        return CatalogAttributeValues::Color;
    }
    // `<paint>` is strictly richer than `<color>` (adds `none`, `<url>`,
    // `context-fill`, `context-stroke`); keep it distinct so those alternatives
    // are not silently dropped.
    if normalized == "<paint>" {
        return CatalogAttributeValues::Paint;
    }
    if normalized == "<url>" {
        return CatalogAttributeValues::Url;
    }
    // `<iri>`/`<uri>` (SVG IRI references) permit non-ASCII characters a `<url>`
    // does not; keep them distinct so the broader character set is not lost.
    if normalized == "<iri>" || normalized == "<uri>" {
        return CatalogAttributeValues::Iri;
    }
    if normalized == "<number> | <percentage>" || normalized == "<percentage> | <number>" {
        return CatalogAttributeValues::NumberOrPercentage;
    }
    if normalized == "<number>" {
        return CatalogAttributeValues::Number;
    }
    if matches!(
        normalized.as_str(),
        "<length>" | "<length-percentage>" | "<length> | <percentage>"
    ) {
        return CatalogAttributeValues::Length;
    }
    let graph = css_grammar_graph(value);
    CatalogAttributeValues::CssGrammar { grammar, graph }
}

fn is_see_below_value(value: &str) -> bool {
    let trimmed = value.trim();
    let unwrapped = trimmed
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .map_or(trimmed, str::trim);
    unwrapped.eq_ignore_ascii_case("see below")
}

/// Fold the typographic quotes SVGWG uses around CSS property-references
/// (`<‘'opacity'’>`) to ASCII apostrophes, leaving `<'opacity'>`.
fn fold_property_reference_quotes(value: &str) -> String {
    value.replace(['\u{2018}', '\u{2019}'], "'")
}

/// Resolve a CSS property-reference value (`<'name'>`) to the referenced
/// property's value space.
///
/// SVG value grammars cite two properties this way: `<'color'>` (a `<color>`)
/// and `<'opacity'>` (an `<alpha-value>` = `<number> | <percentage>`). These are
/// the CSS-defined value spaces of those properties, used by `stop-color`,
/// `stop-opacity`, `fill-opacity`, and `stroke-opacity`. The bare `<color>`
/// *type* (no quotes) is left to the normal type handling below.
fn value_from_property_reference(value: &str) -> Option<CatalogAttributeValues> {
    let inner = value.trim().strip_prefix('<')?.strip_suffix('>')?;
    if !inner.contains('\'') {
        return None;
    }
    let name: String = inner
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect();
    match name.as_str() {
        "color" => Some(CatalogAttributeValues::Color),
        "opacity" => Some(CatalogAttributeValues::NumberOrPercentage),
        _ => None,
    }
}

fn value_from_referenced_syntax(value: &str) -> Option<CatalogAttributeValues> {
    // Only exact fetched syntax citations are classified here; anything else
    // stays raw rather than being inferred by substring heuristics.
    match normalize_prose_phrase(value).as_str() {
        "boolean attribute [html]" => Some(CatalogAttributeValues::Boolean),
        "space-separated valid non-empty url tokens [html]" => {
            Some(CatalogAttributeValues::UrlTokenList)
        }
        "set of space-separated tokens [html]" | "space-separated keyword tokens [html]" => {
            Some(CatalogAttributeValues::TokenList)
        }
        "set of comma-separated tokens [html]" => Some(CatalogAttributeValues::CommaTokenList),
        "language-tag [abnf]" | "a bcp 47 language tag string [html]" => {
            Some(CatalogAttributeValues::LanguageTag)
        }
        "valid integer [html]" => Some(CatalogAttributeValues::Integer),
        "url [url]" => Some(CatalogAttributeValues::Url),
        "a referrer policy string [referrerpolicy]" => Some(CatalogAttributeValues::ReferrerPolicy),
        "a mime type string [html]" => Some(CatalogAttributeValues::MediaType),
        _ => None,
    }
}

fn is_path_data_value(value: &str) -> bool {
    matches!(value.trim(), "path data" | "svg-path [ebnf]")
}

fn is_semicolon_number_list_value(value: &str) -> bool {
    let value = value.trim();
    value.contains("<number>") && value.contains("[; <number>]*")
}

fn is_coordinate_pair_value(value: &str) -> bool {
    let value = value.trim();
    value == "x, y coordinate pair"
}

fn is_coordinate_pair_list_value(value: &str) -> bool {
    let value = value.trim();
    value == "semicolon-separated x, y coordinate pairs"
}

fn is_suggested_file_name_prose_value(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("any value") && lower.contains("suggested file name")
}

fn value_from_see_below_description(prose: &str) -> CatalogAttributeValues {
    // Semantic projection policy: `(see below)` is resolved from fetched prose
    // here, not from a first-class grammar production.
    let normalized = normalize_prose_phrase(prose);
    if normalized.contains("must not be an empty string")
        && normalized.contains("must not contain any whitespace")
    {
        return CatalogAttributeValues::Id;
    }
    let enum_values = quoted_keyword_values(prose);
    if !enum_values.is_empty()
        && (normalized.contains("possible values")
            || normalized.contains("values are the strings")
            || normalized.contains("values are "))
    {
        return CatalogAttributeValues::Enum {
            values: enum_values,
        };
    }
    if normalized.contains("parsed as a media_query_list") {
        return CatalogAttributeValues::MediaQueryList;
    }
    if normalized.contains("style sheet language as a media type") {
        return CatalogAttributeValues::MediaType;
    }
    if normalized.contains("parsed as a declaration-list") {
        return CatalogAttributeValues::CssDeclarationList;
    }
    CatalogAttributeValues::FreeText
}

fn normalize_prose_phrase(text: &str) -> String {
    normalize_ws(text).to_ascii_lowercase()
}

pub(super) fn quoted_keyword_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    for quoted in text.split('\'').skip(1).step_by(2) {
        if is_keyword_token(quoted) {
            values.push(quoted.to_owned());
        }
    }
    values.sort();
    values.dedup();
    values
}

/// Whether the raw value grammar is just bare keyword alternatives.
fn is_keyword_only_grammar(value: &str) -> bool {
    value.split('|').all(|token| is_keyword_token(token.trim()))
}

/// Source-verified value spaces that override what the raw grammar text would
/// otherwise reduce to, applied before the generic grammar projection.
///
/// * `begin`/`end` cite the SMIL `begin-value-list`/`end-value-list` production
///   (a semicolon-separated list of offset/syncbase/event/repeat/accessKey/
///   wallclock begin-values), not a keyword — we do not model that grammar, so
///   keep it an auditable [`CatalogAttributeValues::Unresolved`] gap rather than
///   a bogus single-keyword enum of the production name.
/// * `width`/`height` accept `auto` in SVG 2 (geometry.html §Sizing), but
///   `definitions.xml` types them only as `<length-percentage>`; restore the
///   full source value space rather than reducing to `Length` and dropping
///   `auto`.
fn source_verified_value_override(property: &PropertyValueDef) -> Option<CatalogAttributeValues> {
    if property
        .value
        .as_deref()
        .is_some_and(is_unmodeled_production_reference)
    {
        return Some(CatalogAttributeValues::Unresolved);
    }
    if let Some(grammar) = geometry_sizing_grammar(&property.name)
        && property.value.as_deref().is_some_and(|value| {
            matches!(
                value.trim(),
                "<length>" | "<length-percentage>" | "<length> | <percentage>"
            )
        })
    {
        return Some(CatalogAttributeValues::CssGrammar {
            graph: css_grammar_graph(grammar),
            grammar: grammar.to_owned(),
        });
    }
    None
}

/// Whether the whole grammar is a single SVG grammar-production reference that
/// looks like a keyword but is not one (`begin-value-list`, `end-value-list`):
/// an unmodeled value space that must stay auditable rather than collapse to a
/// one-element enum of the production name.
fn is_unmodeled_production_reference(value: &str) -> bool {
    let value = value.trim();
    value.ends_with("-value-list") && !value.contains(char::is_whitespace)
}

/// The source-verified SVG 2 value grammar for the `width`/`height` geometry
/// sizing properties, which accept `auto` on top of `<length-percentage>`
/// (geometry.html §Sizing). `None` for any other name.
fn geometry_sizing_grammar(name: &str) -> Option<&'static str> {
    matches!(name, "width" | "height").then_some("auto | <length-percentage>")
}

pub(super) fn repair_css_type_tokens(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut repaired = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'<'
            && bytes
                .get(index + 1)
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'\'')
        {
            let start = index;
            repaired.push('<');
            index += 1;
            // A multiplier/occurrence marker (`#`, `?`, `+`, `*`, `!`) can never
            // be part of a type name, so when the upstream propdef HTML glued one
            // inside the brackets (`<mask-layer#>`, `<length?>`) it is pulled back
            // out as a suffix, yielding the canonical `<mask-layer>#` / `<length>?`.
            let mut suffix = String::new();
            while index < bytes.len()
                && bytes[index] != b'>'
                && !bytes[index].is_ascii_whitespace()
                && !matches!(bytes[index], b'|' | b'[' | b']' | b',')
            {
                if matches!(bytes[index], b'#' | b'?' | b'+' | b'*' | b'!') {
                    while index < bytes.len()
                        && matches!(bytes[index], b'#' | b'?' | b'+' | b'*' | b'!')
                    {
                        suffix.push(char::from(bytes[index]));
                        index += 1;
                    }
                    break;
                }
                repaired.push(char::from(bytes[index]));
                index += 1;
            }
            repaired.push('>');
            if bytes.get(index) == Some(&b'>') {
                index += 1;
            }
            repaired.push_str(&suffix);
            if index == start {
                index += 1;
            }
            continue;
        }
        repaired.push(char::from(bytes[index]));
        index += 1;
    }
    repaired
}

/// Finalize an attribute's value space after primary extraction.
///
/// Resolves value spaces that SVG delegates to external specs and thus arrive
/// without a local grammar: WAI-ARIA attributes from the ARIA index, and CSS
/// properties from the fetched `@webref/css` grammar. Anything still left as
/// [`CatalogAttributeValues::FreeText`] that is not *genuinely* free-form then
/// becomes [`CatalogAttributeValues::Unresolved`], so unparsed value spaces stay
/// auditable instead of hiding among event handlers and `data-*`.
pub(super) fn apply_derived_value_space(
    attribute: &mut CatalogAttribute,
    aria: Option<&crate::aria::AriaValueIndex>,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
    svg11_grammars: &BTreeMap<String, String>,
    descriptions_by_id: &BTreeMap<&str, &str>,
) {
    if !matches!(attribute.values, CatalogAttributeValues::FreeText) {
        return;
    }
    // ARIA resolution counts even when it yields `FreeText` (a `string` type):
    // that is a *derived* free-form space, so it is exempt from the sink split.
    if let Some(values) = aria.and_then(|index| index.get(&attribute.name)) {
        attribute.values = values.clone();
        return;
    }
    if let Some(values) =
        css_property_value_space(&attribute.name, grammar_inputs, descriptions_by_id)
    {
        attribute.values = values;
        return;
    }
    // Properties SVG 2 removed but SVG 1.1 defines (e.g. `kerning`,
    // `glyph-orientation-horizontal`, `color-profile`) resolve from the SVG 1.1
    // property index we already fetch.
    if let Some(grammar) = svg11_grammars.get(&attribute.name)
        && let Some(values) =
            resolve_source_grammar(&attribute.name, grammar, grammar_inputs, descriptions_by_id)
    {
        attribute.values = values;
        return;
    }
    if let Some(values) =
        deprecated_svg_value_space(&attribute.name, grammar_inputs, descriptions_by_id)
    {
        attribute.values = values;
        return;
    }
    // Attributes SVG borrows from HTML (`decoding`, `fetchpriority`,
    // `async`/`defer`) carry no fetched grammar; resolve them from the WHATWG
    // spec sections read directly from their HTML.
    if let Some(values) =
        html_borrowed_value_space(&attribute.name, grammar_inputs, descriptions_by_id)
    {
        attribute.values = values;
        return;
    }
    if let Some(values) = definitional_value_space(&attribute.name) {
        attribute.values = values;
        return;
    }
    if !is_genuinely_free_value(&attribute.name) {
        attribute.values = CatalogAttributeValues::Unresolved;
    }
}

/// Value spaces fixed by definition rather than a fetched grammar.
///
/// * `xml:lang` is a BCP 47 language tag (per the XML/SVG definition).
/// * The SMIL animation value attributes `by`/`from`/`to`/`values` are
///   polymorphic: each is parsed using the rules of the attribute named by
///   `attributeName` (SVG Animations, values-attribute prose), so there is no
///   fixed grammar at the attribute level. `title` is advisory prose. All are
///   genuinely unconstrained statically, so they resolve to `FreeText` rather
///   than an unresolved gap. (Element-scoped overrides — e.g. `animateMotion`'s
///   coordinate-pair `from`/`to` — are applied separately.)
fn definitional_value_space(name: &str) -> Option<CatalogAttributeValues> {
    match name {
        "xml:lang" => Some(CatalogAttributeValues::LanguageTag),
        // Advisory prose, polymorphic SMIL values, the free-form `baseProfile`
        // profile-name (SVG 1.1: `= profile-name`, no enumeration), and the
        // deprecated `xlink:title` (SVG 2 linking.html: value `<anything>`).
        "by" | "from" | "to" | "values" | "title" | "baseProfile" | "xlink:title" => {
            Some(CatalogAttributeValues::FreeText)
        }
        _ => None,
    }
}

/// Source-verified value grammars for attributes SVG 2 removed but that browser
/// compat data still tracks, so they have no machine-readable SVG source. Each
/// grammar is quoted verbatim from the defining spec (read directly from its
/// HTML), with the universal `inherit` keyword dropped; it is resolved through
/// the same grammar projection as native properties.
fn deprecated_attribute_grammar(name: &str) -> Option<&'static str> {
    Some(match name {
        "zoomAndPan" => "disable | magnify", // SVG 2 §15.7 zoomAndPan attribute
        "externalResourcesRequired" => "false | true", // SVG 1.1 §5.8.1
        "attributeType" => "CSS | XML | auto", // SVG 1.1 §19.2.5 / SMIL
        "version" => "<number>",             // SVG 1.1 §5.1.3 version attribute
        "xlink:show" => "new | replace | embed | other | none", // XLink 1.1 §5.6.1
        "xlink:actuate" => "onLoad | onRequest | other | none", // XLink 1.1 §5.6.2
        _ => return None,
    })
}

/// Resolve a deprecated attribute's source-verified grammar to a value space.
fn deprecated_svg_value_space(
    name: &str,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
    descriptions_by_id: &BTreeMap<&str, &str>,
) -> Option<CatalogAttributeValues> {
    let grammar = deprecated_attribute_grammar(name)?;
    resolve_source_grammar(name, grammar, grammar_inputs, descriptions_by_id)
}

/// Value spaces for attributes SVG borrows from HTML, whose grammars are
/// defined by the WHATWG HTML spec rather than any SVG or CSS source. They
/// arrive from browser-compat data with no fetched grammar, so each keyword set
/// is quoted from the spec section read directly from its HTML.
///
/// # Sources
///
/// - `async`/`defer` boolean attributes — [`scripting.html`][script]
/// - `decoding` image decoding hint — [`images.html`][decoding]
/// - `fetchpriority` fetch priority attribute — [`urls-and-fetching.html`][fetchpriority]
///
/// `interestfor` (interest invokers) is not yet in the WHATWG spec — an
/// experimental attribute — but MDN's [interest invokers guide][interestfor]
/// states its value "is the id of the target element": a single element ID
/// reference, exactly like `aria-activedescendant`. Whether that id resolves to
/// an element is a document-level (LSP) check, not a value-space concern.
///
/// [script]: https://html.spec.whatwg.org/multipage/scripting.html#attr-script-async
/// [decoding]: https://html.spec.whatwg.org/multipage/images.html#image-decoding-hint
/// [fetchpriority]: https://html.spec.whatwg.org/multipage/urls-and-fetching.html#fetch-priority-attribute
/// [interestfor]: https://developer.mozilla.org/en-US/docs/Web/API/Popover_API/Using_interest_invokers
fn html_borrowed_value_space(
    name: &str,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
    descriptions_by_id: &BTreeMap<&str, &str>,
) -> Option<CatalogAttributeValues> {
    match name {
        // Presence (boolean) attributes: the script element's `async`/`defer`.
        "async" | "defer" => Some(CatalogAttributeValues::Boolean),
        // Single element ID reference to the interest target.
        "interestfor" => Some(CatalogAttributeValues::Id),
        // Enumerated attributes, keywords verbatim from the spec keyword tables.
        "decoding" => resolve_source_grammar(
            name,
            "sync | async | auto",
            grammar_inputs,
            descriptions_by_id,
        ),
        "fetchpriority" => resolve_source_grammar(
            name,
            "high | low | auto",
            grammar_inputs,
            descriptions_by_id,
        ),
        _ => None,
    }
}

/// Project a CSS-style value grammar string — from any spec source (SVG 1.1
/// property index, `@webref/css`, a source-verified deprecated grammar) — into
/// a catalog value space, reusing the same grammar projection as native SVG
/// properties so keyword sets become `Enum`, `<color>` becomes `Color`, and so
/// on. A grammar that is one bare type reference (`opacity = <opacity-value>`)
/// is expanded one level via its webref definition when that yields a more
/// specialized variant. Returns `None` when the grammar does not reduce past
/// free text, leaving the attribute for the sink split.
fn resolve_source_grammar(
    name: &str,
    grammar: &str,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
    descriptions_by_id: &BTreeMap<&str, &str>,
) -> Option<CatalogAttributeValues> {
    let resolve = |grammar: &str| {
        let property = PropertyValueDef {
            name: name.to_owned(),
            dfn_for: None,
            id: None,
            keywords: crate::chapter::value_keywords(grammar),
            value: Some(grammar.to_owned()),
            initial: None,
            applies_to: None,
            inherited: None,
            computed_value: None,
            animation_type: None,
        };
        values_for_property(&property, descriptions_by_id, grammar_inputs)
    };

    let mut values = resolve(grammar);
    // A property whose whole grammar is one type reference (`opacity =
    // <opacity-value>`) stays a coarse `CssGrammar`. Expand that type one level
    // via its webref definition (`<opacity-value>` = `<number> | <percentage>`)
    // and keep the expansion only when it resolves to a specialized variant, so
    // e.g. `opacity` becomes `NumberOrPercentage` while `color` (already
    // `Color`) is left untouched. Expansion needs the webref type index; when
    // absent the bare-type grammar is kept as-is.
    if matches!(values, CatalogAttributeValues::CssGrammar { .. })
        && let Some(inputs) = grammar_inputs
        && let Some(expanded) = expand_single_type_reference(grammar, inputs)
    {
        let expanded_values = resolve(expanded);
        if !matches!(
            expanded_values,
            CatalogAttributeValues::CssGrammar { .. } | CatalogAttributeValues::FreeText
        ) {
            values = expanded_values;
        }
    }
    // Accept the derived value space (a specialized variant where the grammar
    // reduces cleanly, otherwise the CSS grammar itself); never downgrade to
    // free text here — leave that to the sink split.
    match values {
        CatalogAttributeValues::FreeText | CatalogAttributeValues::Unresolved => None,
        resolved => Some(resolved),
    }
}

/// Resolve a CSS property's value space from the published `@webref/css` value
/// grammar, for presentation/CSS attributes SVG delegates to CSS with no local
/// definition (`writing-mode`, `text-align`, `object-fit`, …). Reuses the same
/// grammar projection as native SVG properties so keyword sets become `Enum`,
/// `<color>` becomes `Color`, and so on. Returns `None` when the name is not a
/// CSS property or webref yields no usable grammar, leaving it for the sink
/// split.
fn css_property_value_space(
    name: &str,
    grammar_inputs: Option<&crate::treesitter::GrammarProjectionInputs>,
    descriptions_by_id: &BTreeMap<&str, &str>,
) -> Option<CatalogAttributeValues> {
    let inputs = grammar_inputs?;
    let syntax = inputs.property_syntax(name)?;
    resolve_source_grammar(name, syntax, Some(inputs), descriptions_by_id)
}

/// If `syntax` is exactly one bare type reference (`<opacity-value>`), return
/// that type's webref grammar; otherwise `None`. Bracketed groups, functional
/// notation, and multi-token grammars are left as-is.
fn expand_single_type_reference<'a>(
    syntax: &str,
    inputs: &'a crate::treesitter::GrammarProjectionInputs,
) -> Option<&'a str> {
    let inner = syntax.trim().strip_prefix('<')?.strip_suffix('>')?;
    if inner.is_empty()
        || !inner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return None;
    }
    inputs.type_syntax(inner)
}

/// Whether an attribute's value is genuinely unconstrained rather than merely
/// not-yet-derived: event-handler attributes (arbitrary ECMAScript) and the
/// `data-*` wildcard. WAI-ARIA `string` attributes are resolved to `FreeText`
/// upstream of the sink split, so they are intentionally excluded here.
fn is_genuinely_free_value(name: &str) -> bool {
    name == "data-*"
        || (name.starts_with("on") && name[2..].starts_with(|ch: char| ch.is_ascii_lowercase()))
}
