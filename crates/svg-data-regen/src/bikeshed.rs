//! Lower SVG 2's Bikeshed chapter sources to the plain HTML the extractors read.
//!
//! Since 2026-10-14 ([w3c/svgwg#1036]) SVG 2 is written in Bikeshed: one `.bs`
//! file per chapter under `master/`, built by `tools/svg2-build/`. A chapter
//! source is ordinary HTML except for three things this module undoes, so the
//! chapter, path-grammar and lifecycle extractors see the same document shape
//! the pre-Bikeshed HTML source gave them:
//!
//! - **Link shorthands** in text: `<{rect}>`, `<{rect/rx}>`, `{{SVGElement}}`,
//!   `{{SVGSVGElement/currentScale}}`, `[=term=]`, each optionally with
//!   `|link text`. Each becomes an `<a data-link-type=…>` holding the text
//!   Bikeshed renders. Property shorthands (`'fill'`) are already plain text.
//! - **Name quoting**: a single name the spec wraps in curly quotes, as plain
//!   text or inside link or name markup (`‘<{rect}>’`,
//!   `‘<a href="#FxAttribute">fx</a>’`), gets the straight quotes property
//!   shorthands use (`'rect'`), and so do the long-form property links Bikeshed
//!   renders unquoted (`<a class="property">fill</a>`). Descriptions then
//!   quote names one way, as the old HTML source wrote them, and the lifecycle
//!   extractor can read declarations such as `'kerning' property`.
//! - **Escapes**: a backslash before `'`, `[`, `{`, `<` or `=` only stops
//!   Bikeshed from reading a shorthand, so it is dropped.
//! - **Single-page ids**: a heading whose single-page id differs from its
//!   chapter-page id carries the chapter-page id in `data-multipage-id`; that
//!   id is the one the editor's-draft chapter pages (and so permalinks) use, so
//!   it replaces `id`. The empty `<span class="bs-old-id">` aliases kept for
//!   the single page are removed.
//!
//! Bikeshed reads no shorthand inside `<pre>`, `<xmp>`, `<script>`, `<style>`
//! or comments, so neither does this. Content generated at build time from the
//! definitions files stays a `data-svg-fill` placeholder; the extractors read
//! those facts from the definitions XML instead.
//!
//! The authoring rules are in [`tools/svg2-build/README.md`][readme].
//!
//! [w3c/svgwg#1036]: https://github.com/w3c/svgwg/issues/1036
//! [readme]: https://github.com/w3c/svgwg/blob/main/tools/svg2-build/README.md

use std::sync::LazyLock;

use regex::Regex;

/// The empty single-page id aliases `convert.py` placed next to chapter ids.
static OLD_ID_SPAN_RE: LazyLock<Regex> = LazyLock::new(|| {
    crate::util::compile_regex(r#"(?s)<span\b[^>]*\bclass="bs-old-id"[^>]*>\s*</span>"#)
});
/// A start tag's `data-multipage-id` value.
static MULTIPAGE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::util::compile_regex(r#"\bdata-multipage-id="([^"]*)""#));
/// A start tag's `id` attribute.
static ID_RE: LazyLock<Regex> = LazyLock::new(|| crate::util::compile_regex(r#"(\s)id="[^"]*""#));

/// A single name in the spec's curly quotes, possibly inside link or name
/// markup: `‘<a href="#FxAttribute">fx</a>’`, `‘<span class="property">cx</span>’`.
static CURLY_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    crate::util::compile_regex(
        r"‘((?:<[A-Za-z][^>]*>)*)([A-Za-z_][A-Za-z0-9_.:-]*)((?:</[A-Za-z]+>)*)’",
    )
});
/// An unquoted property link.
static BARE_PROPERTY_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    crate::util::compile_regex(r#"(<a\b[^>]*\bclass="property"[^>]*>)([^<'‘’]+)(</a>)"#)
});

/// Elements whose content Bikeshed leaves alone.
const OPAQUE_ELEMENTS: &[&str] = &["pre", "xmp", "script", "style"];
/// Characters a backslash escapes in Bikeshed source.
const ESCAPABLE: &[char] = &['\'', '[', '{', '<', '='];

/// The kinds of link shorthand, by how they open and close.
#[derive(Clone, Copy)]
enum Shorthand {
    /// `<{element}>` or `<{element/attribute}>`.
    Markup,
    /// `{{Interface}}` or `{{Interface/member}}`.
    Idl,
    /// `[=term=]` or `[=for/term=]`.
    Dfn,
}

impl Shorthand {
    /// The shorthand starting at the beginning of `rest`, if any.
    fn at(rest: &str) -> Option<Self> {
        if rest.starts_with("<{") {
            Some(Self::Markup)
        } else if rest.starts_with("{{") {
            Some(Self::Idl)
        } else if rest.starts_with("[=") {
            Some(Self::Dfn)
        } else {
            None
        }
    }

    const fn close(self) -> &'static str {
        match self {
            Self::Markup => "}>",
            Self::Idl => "}}",
            Self::Dfn => "=]",
        }
    }

    /// The `data-link-type` the lowered link carries.
    fn link_type(self, target: &str) -> &'static str {
        match self {
            Self::Markup if target.contains('/') => "element-attr",
            Self::Markup => "element",
            Self::Idl => "idl",
            Self::Dfn => "dfn",
        }
    }
}

/// Lower one Bikeshed chapter source to plain HTML (see the module docs).
#[must_use]
pub fn lower(source: &str) -> String {
    let source = OLD_ID_SPAN_RE.replace_all(source, "");
    let source = BARE_PROPERTY_LINK_RE.replace_all(&source, "$1'$2'$3");
    let mut out = String::with_capacity(source.len());
    let mut rest: &str = &source;
    let mut opaque: Option<&'static str> = None;

    while let Some(ch) = rest.chars().next() {
        if let Some(element) = opaque {
            // Copy verbatim up to the closing tag; the tag itself is handled
            // below as an ordinary tag.
            let end = find_closing_tag(rest, element).unwrap_or(rest.len());
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            opaque = None;
            continue;
        }
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map_or(rest.len(), |at| at + 3);
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        if let Some(kind) = Shorthand::at(rest)
            && let Some(len) = lower_shorthand(kind, rest, &mut out)
        {
            rest = &rest[len..];
            continue;
        }
        if ch == '<' && starts_tag(rest) {
            let end = tag_end(rest);
            let tag = &rest[..end];
            out.push_str(&lower_tag(tag));
            opaque = opaque_element(tag);
            rest = &rest[end..];
            continue;
        }
        if ch == '\\'
            && let Some(next) = rest[1..].chars().next()
            && ESCAPABLE.contains(&next)
        {
            out.push(next);
            rest = &rest[1 + next.len_utf8()..];
            continue;
        }
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    CURLY_NAME_RE.replace_all(&out, "$1'$2'$3").into_owned()
}

/// Lower the shorthand of `kind` at the start of `rest` into `out`, returning
/// how many bytes of `rest` it consumed, or `None` when it is unterminated.
fn lower_shorthand(kind: Shorthand, rest: &str, out: &mut String) -> Option<usize> {
    let body_end = rest[2..].find(kind.close())? + 2;
    let body = &rest[2..body_end];
    if body.contains('\n') {
        return None;
    }
    let consumed = body_end + 2;
    let (target, text) = body
        .split_once('|')
        .map_or_else(|| (body, link_text(body)), |(target, text)| (target, text));
    let link_type = kind.link_type(target);
    out.push_str("<a data-link-type=\"");
    out.push_str(link_type);
    out.push_str("\">");
    out.push_str(text);
    out.push_str("</a>");
    Some(consumed)
}

/// The text Bikeshed shows for a shorthand without explicit link text: the
/// name after the last `/` (`rect/rx` shows `rx`), the whole name otherwise.
fn link_text(target: &str) -> &str {
    // Only a `/` before a method's argument list separates the `for` part.
    let head = target.find('(').map_or(target, |at| &target[..at]);
    head.rfind('/').map_or(target, |at| &target[at + 1..])
}

/// Whether `rest` (starting with `<`) opens a tag rather than being text.
fn starts_tag(rest: &str) -> bool {
    rest[1..]
        .chars()
        .next()
        .is_some_and(|next| next.is_ascii_alphabetic() || matches!(next, '/' | '!' | '?'))
}

/// The byte length of the tag at the start of `rest`, quotes respected.
fn tag_end(rest: &str) -> usize {
    let mut quote: Option<char> = None;
    for (at, ch) in rest.char_indices().skip(1) {
        if let Some(open) = quote {
            if ch == open {
                quote = None;
            }
        } else if matches!(ch, '"' | '\'') {
            quote = Some(ch);
        } else if ch == '>' {
            return at + 1;
        }
    }
    rest.len()
}

/// Rewrite a tag's `id` to its chapter-page id when it carries one.
fn lower_tag(tag: &str) -> String {
    let Some(multipage) = MULTIPAGE_ID_RE.captures(tag) else {
        return tag.to_owned();
    };
    let id = multipage[1].to_owned();
    let without = MULTIPAGE_ID_RE.replace(tag, "");
    if ID_RE.is_match(&without) {
        ID_RE
            .replace(&without, |caps: &regex::Captures<'_>| {
                format!("{}id=\"{id}\"", &caps[1])
            })
            .into_owned()
    } else {
        without.into_owned()
    }
}

/// The opaque element a start tag opens, if it opens one.
fn opaque_element(tag: &str) -> Option<&'static str> {
    if tag.starts_with("</") || tag.ends_with("/>") {
        return None;
    }
    let name: String = tag[1..]
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    OPAQUE_ELEMENTS
        .iter()
        .copied()
        .find(|element| *element == name)
}

/// Where `</element` next starts in `rest`, ignoring ASCII case.
fn find_closing_tag(rest: &str, element: &str) -> Option<usize> {
    let needle = format!("</{element}");
    let lower = rest.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(&needle) {
        let at = from + found;
        let after = lower[at + needle.len()..].chars().next();
        if after.is_none_or(|ch| ch == '>' || ch.is_ascii_whitespace()) {
            return Some(at);
        }
        from = at + needle.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_markup_shorthands_and_requotes_names() {
        let lowered = lower(
            "<p>The <span class=\"element-name\">‘<{rect}>’</span> takes <span \
             class=\"attr-name\">‘<{rect/rx}>’</span>, see <{circle}>.</p>",
        );
        assert_eq!(
            lowered,
            "<p>The <span class=\"element-name\"><a data-link-type=\"element\">'rect'</a></span> \
             takes <span class=\"attr-name\"><a data-link-type=\"element-attr\">'rx'</a></span>, \
             see <a data-link-type=\"element\">circle</a>.</p>"
        );
    }

    #[test]
    fn lowers_idl_and_term_shorthands_with_and_without_text() {
        assert_eq!(
            lower("{{SVGElement}} {{SVGSVGElement/currentScale}} {{A/b(c)|b}}"),
            "<a data-link-type=\"idl\">SVGElement</a> <a data-link-type=\"idl\">currentScale</a> \
             <a data-link-type=\"idl\">b</a>"
        );
        assert_eq!(
            lower("[=user agent=] and [=invalid value|invalid=]"),
            "<a data-link-type=\"dfn\">user agent</a> and <a data-link-type=\"dfn\">invalid</a>"
        );
    }

    #[test]
    fn drops_escapes_and_keeps_property_shorthands_as_text() {
        assert_eq!(
            lower(r"\'none' and 'fill' \[=x=] \{{y}}"),
            "'none' and 'fill' [=x=] {{y}}"
        );
        assert_eq!(lower(r"a \rm b"), r"a \rm b");
    }

    #[test]
    fn leaves_opaque_elements_and_comments_alone() {
        let source = "<pre class=idl>{{X}} <{y}></pre><!-- [=z=] --><xmp>[=q=]</xmp>[=t=]";
        assert_eq!(
            lower(source),
            "<pre class=idl>{{X}} <{y}></pre><!-- [=z=] --><xmp>[=q=]</xmp><a \
             data-link-type=\"dfn\">t</a>"
        );
    }

    #[test]
    fn uses_chapter_page_ids_and_drops_single_page_aliases() {
        assert_eq!(
            lower(
                "<h3 id=\"shapes-Introduction\" data-multipage-id=\"Introduction\">I</h3><p \
                 id=\"x\"><span id=\"geometry-x\" class=\"bs-old-id\">\n</span>T</p>"
            ),
            "<h3 id=\"Introduction\" >I</h3><p id=\"x\">T</p>"
        );
    }

    #[test]
    fn quotes_property_links_like_property_shorthands() {
        assert_eq!(
            lower(
                "‘<span class=\"propdef-title property\">stop-color</span>’ and <a \
                 href=\"#StopColorProperty\" class=\"property\">stop-color</a> and <span \
                 class=\"property\">'fill'</span>"
            ),
            "<span class=\"propdef-title property\">'stop-color'</span> and <a \
             href=\"#StopColorProperty\" class=\"property\">'stop-color'</a> and <span \
             class=\"property\">'fill'</span>"
        );
    }

    #[test]
    fn ignores_unterminated_or_multiline_shorthands() {
        assert_eq!(lower("a [= b\n=] c"), "a [= b\n=] c");
        assert_eq!(lower("x {{ y"), "x {{ y");
    }

    #[test]
    fn keeps_quoted_angle_brackets_inside_tags() {
        assert_eq!(
            lower("<a title=\"a > b\" href=\"#x\">[=t=]</a>"),
            "<a title=\"a > b\" href=\"#x\"><a data-link-type=\"dfn\">t</a></a>"
        );
    }
}
