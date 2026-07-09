//! Small shared helpers for the regeneration pipeline.

use std::borrow::Cow;

use quick_xml::events::BytesStart;
use regex::Regex;
use tl::{HTMLTag, NodeHandle, Parser};

use crate::Fallible;

/// Compile a regex pattern that is fixed at compile time.
///
/// # Panics
/// Panics when `pattern` is not a valid regex.
pub fn compile_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => panic!("invalid regex {pattern:?}: {error}"),
    }
}

/// Wrap a message as a boxed error.
pub fn boxed(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::<dyn std::error::Error>::from(message.into())
}

/// Collapse whitespace runs into single ASCII spaces.
pub fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Decode HTML entities, then collapse whitespace runs into single spaces.
pub fn normalize_html_ws(text: &str) -> String {
    normalize_ws(decode_html_entities(text).as_ref())
}

/// The unescaped value of quick-xml attribute `key` (matched by local name) on
/// `element`, if present.
pub fn xml_attribute(element: &BytesStart, key: &[u8]) -> Fallible<Option<String>> {
    for attr in element.attributes() {
        let attr = attr?;
        if attr.key.local_name().as_ref() == key {
            return Ok(Some(
                attr.normalized_value(quick_xml::XmlVersion::default())?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

/// Whether `url` is an absolute `http(s)` URL (as opposed to a spec-relative
/// href that must be resolved against a base).
pub fn is_absolute_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Resolve `href` against `base`: absolute hrefs pass through, relative ones are
/// appended to `base`.
pub fn resolve_url(base: &str, href: &str) -> String {
    if is_absolute_url(href) {
        href.to_owned()
    } else {
        format!("{base}{href}")
    }
}

/// The page portion of a `url`, dropping any `#fragment`.
pub fn page_url(url: &str) -> String {
    url.split_once('#')
        .map_or(url, |(page, _fragment)| page)
        .to_owned()
}

/// The byte offset just past the `>` that closes the tag opening at `start`,
/// skipping `>` inside single- or double-quoted attribute values.
pub fn tag_open_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, ch) in html[start..].char_indices() {
        match (quote, ch) {
            (Some(current), found) if found == current => quote = None,
            (None, '"' | '\'') => quote = Some(ch),
            (None, '>') => return Some(start + offset + ch.len_utf8()),
            _ => {}
        }
    }
    None
}

/// Parse an HTML fragment with the pipeline's standard parser options.
///
/// Centralizes `ParserOptions` so a future parsing-config change (id tracking,
/// full DOM, …) is a single edit rather than one per call site.
pub fn parse_html(html: &str) -> Fallible<tl::VDom<'_>> {
    Ok(tl::parse(html, tl::ParserOptions::default())?)
}

/// The raw (un-normalized) inner text of the tag `handle` points at, if any.
///
/// Callers apply their own normalizer ([`normalize_ws`] or [`normalize_html_ws`])
/// so entity-decoding policy stays a caller decision, not baked in here.
pub fn handle_inner_text<'p>(handle: NodeHandle, parser: &'p Parser) -> Option<Cow<'p, str>> {
    Some(handle.get(parser)?.as_tag()?.inner_text(parser))
}

/// The raw inner text of the first descendant of `tag` matching `selector`.
pub fn selector_inner_text<'p>(
    tag: &HTMLTag,
    parser: &'p Parser,
    selector: &str,
) -> Option<Cow<'p, str>> {
    handle_inner_text(tag.query_selector(parser, selector)?.next()?, parser)
}

/// Whether `tag` carries `class` among its space-separated class list.
pub fn has_class(tag: &HTMLTag, class: &str) -> bool {
    tag.attributes().class().is_some_and(|classes| {
        classes
            .as_utf8_str()
            .split_whitespace()
            .any(|each| each == class)
    })
}

/// Decode the HTML entities the upstream spec and MDN prose use.
///
/// Named basics plus numeric references are decoded in a single pass, while
/// unrecognized `&...;` runs are left verbatim.
pub fn decode_html_entities(input: &str) -> Cow<'_, str> {
    if !input.contains('&') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(semi) = after.find(';')
            && let Some(decoded) = decode_html_entity(&after[1..semi])
        {
            out.push(decoded);
            rest = &after[semi + 1..];
            continue;
        }
        out.push('&');
        rest = &after[1..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{00A0}'),
        _ => {
            let code = entity.strip_prefix('#')?;
            let value = match code.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(value)
        }
    }
}

/// Whether a value grammar token is a bare keyword.
pub fn is_keyword_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_html_entities_handles_named_and_numeric() {
        assert_eq!(decode_html_entities("a&lt;b&gt;c").as_ref(), "a<b>c");
        assert_eq!(decode_html_entities("&amp;&quot;").as_ref(), "&\"");
        assert_eq!(decode_html_entities("x&#65;y").as_ref(), "xAy");
        assert_eq!(decode_html_entities("x&#x41;y").as_ref(), "xAy");
        assert_eq!(decode_html_entities("x&#39;y").as_ref(), "x'y");
        assert_eq!(decode_html_entities("plain text").as_ref(), "plain text");
        assert_eq!(decode_html_entities("a&bogus;b").as_ref(), "a&bogus;b");
    }

    #[test]
    fn normalize_html_ws_decodes_then_collapses() {
        assert_eq!(
            normalize_html_ws(" auto | &lt;length&gt;\n"),
            "auto | <length>"
        );
    }
}
