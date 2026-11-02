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
mod css_grammar;
mod declarations;
mod geometry;
mod keyword;
mod length;
mod numeric;
mod path;
mod reference;
mod text_kinds;
mod tokens;

use std::borrow::Cow;

use svg_data::AttributeValues;

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

/// The attribute value as XML delivers it: `raw` is the value node's source
/// text, quotes included.
pub fn attribute_value_text(raw: &str) -> Cow<'_, str> {
    let unquoted = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            raw.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(raw);
    decode_references(unquoted)
}

/// Decode XML character references (`&#65;`, `&#x41;`) and the five predefined
/// entities. An unrecognized reference is kept as written.
fn decode_references(text: &str) -> Cow<'_, str> {
    if !text.contains('&') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let decoded = after.find(';').and_then(|end| {
            let name = &after[..end];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                    .and_then(char::from_u32),
            };
            ch.map(|ch| (ch, end))
        });
        if let Some((ch, end)) = decoded {
            out.push(ch);
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_either_quote_and_decodes_references() {
        assert_eq!(attribute_value_text("\"a b\""), "a b");
        assert_eq!(attribute_value_text("'a'"), "a");
        assert_eq!(attribute_value_text("\"&lt;&amp;&#65;&#x42;\""), "<&AB");
        assert_eq!(attribute_value_text("\"&bogus; &\""), "&bogus; &");
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
