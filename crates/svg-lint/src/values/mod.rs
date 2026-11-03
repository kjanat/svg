//! Attribute value validation against the catalog's value spaces.
//!
//! [`invalid_reason`] maps an attribute's [`AttributeValues`] (for the element
//! it sits on and the active profile) to the check for that kind of value. Each
//! kind lives in its own module and reports `Some(reason)` only for a value the
//! spec makes invalid. A construct a check does not model is accepted: a false
//! positive on valid SVG is worse than a missed error.
//!
//! The value handed to a check is the attribute value as XML delivers it: the
//! surrounding quotes removed and character and entity references decoded.
//! Whitespace is left as written; whether it is allowed is part of each value
//! space's grammar.

mod color;
mod constraints;
mod css_grammar;
mod declarations;
mod geometry;
mod keyword;
mod length;
mod numeric;
mod path;
mod reference;
mod syntax;
mod text_kinds;
mod tokens;

use std::borrow::Cow;

use svg_data::{AttributeApplicability, AttributeDef, AttributeValues, SpecSnapshotId};

/// Why `value` is invalid for `values`, or `None` when it is valid or its
/// value space has no check.
pub fn invalid_reason(values: &AttributeValues, value: &str) -> Option<String> {
    match values {
        AttributeValues::Enum(keywords) => keyword::check_enum(value, keywords),
        AttributeValues::Transform(functions) => geometry::check_transform(value, functions),
        AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => geometry::check_preserve_aspect_ratio(value, alignments, meet_or_slice),
        AttributeValues::Color => color::check_color(value),
        AttributeValues::Paint => color::check_paint(value),
        AttributeValues::Length => length::check_length(value),
        AttributeValues::Url => reference::check_url(value),
        AttributeValues::Iri => reference::check_iri(value),
        AttributeValues::Boolean => tokens::check_boolean(value),
        AttributeValues::TokenList => tokens::check_token_list(value),
        AttributeValues::CommaTokenList => tokens::check_comma_token_list(value),
        AttributeValues::UrlTokenList => tokens::check_url_token_list(value),
        AttributeValues::Id => tokens::check_id(value),
        AttributeValues::IdList => tokens::check_id_list(value),
        AttributeValues::LanguageTag => text_kinds::check_language_tag(value),
        AttributeValues::MediaType => text_kinds::check_media_type(value),
        AttributeValues::MediaQueryList => text_kinds::check_media_query_list(value),
        AttributeValues::ReferrerPolicy => text_kinds::check_referrer_policy(value),
        AttributeValues::SuggestedFileName => text_kinds::check_suggested_file_name(value),
        AttributeValues::Integer => numeric::check_integer(value),
        AttributeValues::Number => numeric::check_number(value),
        AttributeValues::NumberOrPercentage => numeric::check_number_or_percentage(value),
        AttributeValues::PathData => path::check_path_data(value),
        AttributeValues::SemicolonNumberList => path::check_semicolon_number_list(value),
        AttributeValues::CoordinatePair => path::check_coordinate_pair(value),
        AttributeValues::CoordinatePairList => path::check_coordinate_pair_list(value),
        AttributeValues::CssDeclarationList => declarations::check_css_declaration_list(value),
        AttributeValues::CssGrammar { grammar, graph } => {
            css_grammar::check_css_grammar(value, grammar, graph)
        }
        AttributeValues::FreeText | AttributeValues::Unresolved => None,
    }
}

/// The transfer function elements. Filter Effects defines their attributes
/// in the `feComponentTransfer` section, and the catalog keys their
/// element-scoped values on that element rather than on the children that
/// carry them.
const TRANSFER_FUNCTION_ELEMENTS: &[&str] = &["feFuncR", "feFuncG", "feFuncB", "feFuncA"];

/// The value space of `def` on `element` under `profile`, or `None` when the
/// attribute does not apply there.
///
/// The catalog's value space describes the attribute where it applies. On
/// any other element (`path` on a mesh-gradient `<stop>`, a removed feature)
/// the same name has no spec-defined value to check against.
pub fn values_for<'a>(
    def: &'a AttributeDef,
    element: &str,
    profile: SpecSnapshotId,
) -> Option<&'a AttributeValues> {
    let scoped_on = |name: &str| {
        def.element_values
            .iter()
            .find(|values| values.element == name)
            .map(|values| &values.values)
    };
    let scoped = scoped_on(element).or_else(|| {
        TRANSFER_FUNCTION_ELEMENTS
            .contains(&element)
            .then(|| scoped_on("feComponentTransfer"))
            .flatten()
    });
    let applies = match def.applicability {
        AttributeApplicability::Global => true,
        AttributeApplicability::Elements(elements) => elements.contains(&element),
        AttributeApplicability::None => false,
    };
    scoped.or_else(|| applies.then(|| def.values_for_profile(profile)))
}

/// Why `value` is invalid for the attribute `attr` with value space `values`:
/// the value space's check, then the rules the spec adds for `attr` alone
/// (see [`constraints`]).
pub fn invalid_reason_for(attr: &str, values: &AttributeValues, value: &str) -> Option<String> {
    if attr == "d"
        && matches!(values, AttributeValues::CssGrammar { grammar, .. } if *grammar == D_PROPERTY_GRAMMAR)
    {
        return d_attribute(value);
    }
    invalid_reason(values, value).or_else(|| constraints::check(attr, values, value))
}

/// The catalog's grammar for the `d` property.
const D_PROPERTY_GRAMMAR: &str = "none | <string>";

/// The `d` presentation attribute holds path data, not the CSS `<string>` of
/// the `d` property: "d will become a presentation attribute (no name change)
/// with path data string as value", and "The contents of the \<string> value
/// must match the svg-path EBNF grammar"
/// (<https://svgwg.org/svg2-draft/paths.html#TheDProperty>). `none` and the
/// CSS-wide keywords are the property's own values and are accepted.
fn d_attribute(value: &str) -> Option<String> {
    let keyword = syntax::microsyntax::strip_ascii_whitespace(value);
    if keyword.eq_ignore_ascii_case("none") || syntax::cascade::is_css_wide_keyword_value(value) {
        return None;
    }
    path::check_path_data(value)
}

/// The attribute value as XML delivers it: `raw` is the value node's source
/// text, quotes included. `None` when the value references an entity the
/// document declares, whose replacement text this crate does not resolve, or
/// when `raw` is not a whole quoted value (a fragment left by parse error
/// recovery).
///
/// This follows XML 1.0 attribute-value normalization
/// (<https://www.w3.org/TR/xml/#AVNormalize>): line breaks are first
/// normalized (<https://www.w3.org/TR/xml/#sec-line-ends>), each literal
/// white space character becomes a space, and character references and the
/// predefined entities are then replaced. A character reference to white
/// space (`&#10;`) is kept as that character.
pub fn attribute_value_text(raw: &str) -> Option<Cow<'_, str>> {
    let unquoted = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            raw.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })?;
    let normalized = normalize_white_space(unquoted);
    match decode_references(&normalized)? {
        Cow::Borrowed(_) => Some(normalized),
        Cow::Owned(decoded) => Some(Cow::Owned(decoded)),
    }
}

/// Each literal `#x20 | #x9 | #xD | #xA` as one space, with `#xD #xA` taken
/// as a single line break.
fn normalize_white_space(text: &str) -> Cow<'_, str> {
    if !text.contains(['\t', '\n', '\r']) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.replace("\r\n", " ").replace(['\t', '\n', '\r'], " "))
}

/// Decode XML character references (`&#65;`, `&#x41;`) and the five
/// predefined entities. `None` for a reference to any other entity, which
/// only the document's DTD defines. A `&` that starts no well-formed
/// reference is kept as written.
fn decode_references(text: &str) -> Option<Cow<'_, str>> {
    if !text.contains('&') {
        return Some(Cow::Borrowed(text));
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let reference = after.find(';').map(|end| (&after[..end], end));
        let decoded = match reference {
            Some((name, end)) => match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ if name.starts_with('#') => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                    .and_then(char::from_u32),
                _ if is_xml_name(name) => return None,
                _ => None,
            }
            .map(|ch| (ch, end)),
            None => None,
        };
        if let Some((ch, end)) = decoded {
            out.push(ch);
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    Some(Cow::Owned(out))
}

/// Whether `name` is an XML `Name`, restricted to the ASCII subset plus any
/// non-ASCII character (a conservative superset for entity names).
fn is_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| {
        first.is_ascii_alphabetic() || matches!(first, '_' | ':') || !first.is_ascii()
    }) && chars.all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '-' | '.') || !ch.is_ascii()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_either_quote_and_decodes_references() {
        let text = |raw| attribute_value_text(raw).map(Cow::into_owned);
        assert_eq!(text("\"a b\"").as_deref(), Some("a b"));
        assert_eq!(text("'a'").as_deref(), Some("a"));
        assert_eq!(text("\"&lt;&amp;&#65;&#x42;\"").as_deref(), Some("<&AB"));
        assert_eq!(text("\"& &#; &1;\"").as_deref(), Some("& &#; &1;"));
    }

    // Claim: "For a white space character (#x20, #xD, #xA, #x9), append a
    // space character (#x20) to the normalized value." and "For a character
    // reference, append the referenced character to the normalized value."
    // https://www.w3.org/TR/xml/#AVNormalize
    #[test]
    fn literal_white_space_is_normalized_but_references_are_not() {
        let text = |raw| attribute_value_text(raw).map(Cow::into_owned);
        assert_eq!(text("\"a\r\nb\tc\rd\ne\"").as_deref(), Some("a b c d e"));
        assert_eq!(text("\"a&#10;b&#9;c\"").as_deref(), Some("a\nb\tc"));
    }

    // Claim: "For an entity reference, recursively apply step 3 of this
    // algorithm to the replacement text of the entity."
    // https://www.w3.org/TR/xml/#AVNormalize
    #[test]
    fn declared_entity_references_make_the_value_unknown() {
        assert_eq!(attribute_value_text("\"&overflow;\""), None);
        assert_eq!(attribute_value_text("\"1 &st0; 2\""), None);
    }

    #[test]
    fn unquoted_fragments_are_not_values() {
        for raw in ["", "\"", "a", "\"a", "'a\""] {
            assert_eq!(attribute_value_text(raw), None, "{raw:?}");
        }
    }

    fn d_values() -> AttributeValues {
        let Some(def) = svg_data::attribute("d") else {
            panic!("d is not in the catalog");
        };
        def.values_for_element(Some("path")).clone()
    }

    // Claim: "The contents of the <string> value must match the svg-path EBNF
    // grammar defined below"
    // https://svgwg.org/svg2-draft/paths.html#TheDProperty
    #[test]
    fn d_attribute_is_checked_as_path_data() {
        let values = d_values();
        for value in ["M0 0L10 10", " m 1,2 3,4z ", "", "none", "inherit"] {
            assert_eq!(invalid_reason_for("d", &values, value), None, "{value:?}");
        }
        for value in ["c 1 1", "M0 0 L", "M 0 0 X 1"] {
            assert!(
                invalid_reason_for("d", &values, value).is_some(),
                "{value:?}"
            );
        }
    }

    /// The value space of `attr` on `element` in the latest profile.
    fn latest(attr: &str, element: &str) -> Option<&'static AttributeValues> {
        let Some(def) = svg_data::attribute(attr) else {
            panic!("{attr} is not in the catalog");
        };
        values_for(def, element, SpecSnapshotId::LATEST)
    }

    #[test]
    fn attributes_have_no_value_space_off_their_elements() {
        assert!(matches!(
            latest("path", "animateMotion"),
            Some(AttributeValues::PathData)
        ));
        assert!(latest("path", "stop").is_none());
    }

    // Claim: "type = "identity | table | discrete | linear | gamma""
    // https://drafts.csswg.org/filter-effects-1/#element-attrdef-fecomponenttransfer-type
    // Claim: "When any other attribute defined using the CSS Value
    // Definition Syntax is parsed, this is done by parsing the attribute's
    // value according to the grammar given in attribute definition table."
    // https://svgwg.org/svg2-draft/types.html#attribute-css-value
    // Claim: "Keywords are identifiers and are interpreted ASCII
    // case-insensitively (i.e., [a-z] and [A-Z] are equivalent)."
    // https://drafts.csswg.org/css-values-4/#keywords
    #[test]
    fn transfer_function_attributes_use_the_component_transfer_values() {
        for element in TRANSFER_FUNCTION_ELEMENTS {
            let Some(values) = latest("type", element) else {
                panic!("type has no value space on <{element}>");
            };
            for value in ["linear", "GAMMA", " table ", "/* c */ discrete", "var(--t)"] {
                assert_eq!(invalid_reason_for("type", values, value), None, "{value:?}");
            }
            for value in ["lineer", "text/html", "linear gamma", "'linear'"] {
                assert!(
                    invalid_reason_for("type", values, value).is_some(),
                    "{value:?}"
                );
            }
            assert!(matches!(
                latest("slope", element),
                Some(AttributeValues::Number)
            ));
        }
    }

    #[test]
    fn free_text_and_unresolved_have_no_check() {
        assert_eq!(invalid_reason(&AttributeValues::FreeText, "anything"), None);
        assert_eq!(
            invalid_reason(&AttributeValues::Unresolved, "anything"),
            None
        );
    }
}
