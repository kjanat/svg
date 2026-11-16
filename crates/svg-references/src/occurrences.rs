use std::fmt;

use svg_tree::{child_of_kind, has_ancestor, is_attribute_node_kind, walk_tree};
use tree_sitter::{Parser, Point, Tree};

use crate::{
    DefinitionTarget, Span, inline_style_fragments,
    text::{
        CssFragment, advance_point, css_escape_identifier, css_unescape, is_xml_name,
        is_xml_whitespace, node_span, rebase_point, span_between, strip_quotes,
        xml_escape_attribute, xml_tokens, xml_unescape, xml_unescape_mapped,
    },
};

const ARIA_IDREF_ATTRIBUTES: &[&str] = &[
    "aria-activedescendant",
    "aria-controls",
    "aria-describedby",
    "aria-details",
    "aria-errormessage",
    "aria-flowto",
    "aria-labelledby",
    "aria-owns",
];

/// Whether an occurrence declares its symbol or refers to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceRole {
    /// `id="x"`, a `.x` class selector, or a `--x:` custom property declaration.
    Declaration,
    /// Any other use of the symbol.
    Reference,
}

/// The syntax surrounding an occurrence, which decides how replacement text is escaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceSyntax {
    /// Raw XML attribute text.
    Attribute,
    /// A CSS identifier, inside a `<style>` element or a `style` or presentation attribute.
    CssIdentifier {
        /// The CSS is XML character data, outside a CDATA section.
        xml_escaped: bool,
    },
    /// The fragment of a CSS `url(#...)`.
    CssUrl {
        /// The CSS is XML character data, outside a CDATA section.
        xml_escaped: bool,
    },
    /// The element id of a syncbase or event value in an animation `begin` or `end` list.
    TimingReference,
}

/// One place where an id, class, or custom property appears in an SVG document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolOccurrence {
    /// The symbol, with XML character references and CSS escapes decoded.
    pub target: DefinitionTarget,
    /// Declaration or reference.
    pub role: OccurrenceRole,
    /// Surrounding syntax.
    pub syntax: OccurrenceSyntax,
    /// Start byte of the bare name, excluding `#`, `.` and `url(` wrappers.
    pub start_byte: usize,
    /// End byte of the bare name.
    pub end_byte: usize,
    /// Source span of the bare name.
    pub span: Span,
}

/// A replacement produced by a rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameEdit {
    /// Span to replace.
    pub span: Span,
    /// Replacement text, escaped for its syntax.
    pub new_text: String,
}

/// Why a rename was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    /// The new id is not an XML name.
    InvalidId(String),
    /// The new class name is empty or contains whitespace.
    InvalidClass(String),
    /// The new custom property name lacks the `--` prefix, a body, or contains whitespace.
    InvalidCustomProperty(String),
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(name) => {
                write!(f, "`{name}` is not a valid id: an id must be an XML name")
            }
            Self::InvalidClass(name) => write!(
                f,
                "`{name}` is not a valid class name: a class name must be non-empty and contain \
                 no whitespace"
            ),
            Self::InvalidCustomProperty(name) => write!(
                f,
                "`{name}` is not a valid custom property name: it must start with `--`, have at \
                 least one more character, and contain no whitespace"
            ),
        }
    }
}

impl std::error::Error for RenameError {}

/// Collect every id, class, and custom property occurrence in an SVG document, sorted by
/// position.
///
/// Covers `id` attributes, local `#id` IRIs in attributes and paint values, ARIA id
/// references, `class` attribute tokens, `var()` in presentation attributes, and the
/// selectors, `var()` and `url(#...)` uses in `<style>` elements and `style` attributes.
#[must_use]
pub fn collect_symbol_occurrences(source: &[u8], tree: &Tree) -> Vec<SymbolOccurrence> {
    let mut out = Vec::new();
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| match node.kind() {
        "id_token" => {
            if let Ok(name) = node.utf8_text(source) {
                out.push(markup_occurrence(
                    DefinitionTarget::Id(xml_unescape(name)),
                    OccurrenceRole::Declaration,
                    node.start_byte(),
                    node.end_byte(),
                    node_span(node),
                ));
            }
        }
        "iri_reference" => {
            if is_structural_iri(node)
                && crate::links::fragment_is_local(node, source)
                && !inside_encoded_presentation_value(node, source)
                && let Some(occurrence) = local_iri_occurrence(node, source, 0, Point::new(0, 0))
            {
                out.push(occurrence);
            }
        }
        "class_name" if has_ancestor(node, "class_attribute_value") => {
            if let Ok(text) = node.utf8_text(source) {
                token_occurrences(
                    text,
                    node.start_byte(),
                    node.start_position(),
                    DefinitionTarget::Class,
                    &mut out,
                );
            }
        }
        "paint_payload" if !inside_encoded_presentation_value(node, source) => {
            paint_occurrences(node, source, &mut out);
        }
        "style_text_double" | "style_text_single" => {
            if let Ok(content) = node.utf8_text(source)
                && let Some(fragment) =
                    CssFragment::declarations(content, node.start_byte(), node.start_position())
            {
                css_occurrences(&fragment, &mut out);
            }
        }
        kind if is_attribute_node_kind(kind) => {
            aria_occurrences(node, source, &mut out);
            presentation_occurrences(node, source, &mut out);
            timing_occurrences(node, source, &mut out);
        }
        _ => {}
    });

    for (_, fragment) in inline_style_fragments(source, tree) {
        css_occurrences(&fragment, &mut out);
    }

    out.sort_by_key(|occurrence| (occurrence.start_byte, occurrence.end_byte));
    out.dedup();
    out
}

/// Return the occurrence whose bare name contains `byte_offset`; a cursor touching the end of a
/// name counts.
#[must_use]
pub fn symbol_occurrence_at(
    occurrences: &[SymbolOccurrence],
    byte_offset: usize,
) -> Option<&SymbolOccurrence> {
    occurrences
        .iter()
        .find(|occurrence| (occurrence.start_byte..occurrence.end_byte).contains(&byte_offset))
        .or_else(|| {
            occurrences
                .iter()
                .find(|occurrence| occurrence.end_byte == byte_offset)
        })
}

/// Check a proposed new name for `target`'s kind of symbol.
///
/// # Errors
///
/// Returns a [`RenameError`] naming the rule `new_name` breaks.
pub fn validate_new_name(target: &DefinitionTarget, new_name: &str) -> Result<(), RenameError> {
    match target {
        DefinitionTarget::Id(_) => {
            if is_xml_name(new_name) {
                Ok(())
            } else {
                Err(RenameError::InvalidId(new_name.to_owned()))
            }
        }
        DefinitionTarget::Class(_) => {
            if !new_name.is_empty() && !new_name.chars().any(is_xml_whitespace) {
                Ok(())
            } else {
                Err(RenameError::InvalidClass(new_name.to_owned()))
            }
        }
        DefinitionTarget::CustomProperty(_) => {
            if new_name.len() > 2
                && new_name.starts_with("--")
                && !new_name.chars().any(char::is_whitespace)
            {
                Ok(())
            } else {
                Err(RenameError::InvalidCustomProperty(new_name.to_owned()))
            }
        }
    }
}

/// Build the edits that rename every occurrence of `target` to `new_name`.
///
/// # Errors
///
/// Returns a [`RenameError`] when `new_name` is not valid for `target`'s kind of symbol.
pub fn rename_edits(
    occurrences: &[SymbolOccurrence],
    target: &DefinitionTarget,
    new_name: &str,
) -> Result<Vec<RenameEdit>, RenameError> {
    validate_new_name(target, new_name)?;
    Ok(occurrences
        .iter()
        .filter(|occurrence| occurrence.target == *target)
        .map(|occurrence| RenameEdit {
            span: occurrence.span.clone(),
            new_text: escape_for(occurrence.syntax, new_name),
        })
        .collect())
}

fn escape_for(syntax: OccurrenceSyntax, name: &str) -> String {
    match syntax {
        OccurrenceSyntax::Attribute => xml_escape_attribute(name),
        OccurrenceSyntax::TimingReference => {
            let mut escaped = String::with_capacity(name.len());
            for ch in name.chars() {
                if matches!(ch, '.' | '-' | '\\') {
                    escaped.push('\\');
                }
                escaped.push(ch);
            }
            xml_escape_attribute(&escaped)
        }
        OccurrenceSyntax::CssIdentifier { xml_escaped } => {
            let escaped = css_escape_identifier(name);
            if xml_escaped {
                xml_escape_attribute(&escaped)
            } else {
                escaped
            }
        }
        OccurrenceSyntax::CssUrl { xml_escaped } => {
            if xml_escaped {
                xml_escape_attribute(name)
            } else {
                name.to_owned()
            }
        }
    }
}

const fn markup_occurrence(
    target: DefinitionTarget,
    role: OccurrenceRole,
    start_byte: usize,
    end_byte: usize,
    span: Span,
) -> SymbolOccurrence {
    SymbolOccurrence {
        target,
        role,
        syntax: OccurrenceSyntax::Attribute,
        start_byte,
        end_byte,
        span,
    }
}

pub fn is_structural_iri(node: tree_sitter::Node<'_>) -> bool {
    node.parent()
        .is_some_and(|parent| matches!(parent.kind(), "href_reference" | "functional_iri"))
}

fn local_iri_occurrence(
    node: tree_sitter::Node<'_>,
    text: &[u8],
    base_byte: usize,
    base_point: Point,
) -> Option<SymbolOccurrence> {
    let name = node
        .utf8_text(text)
        .ok()?
        .strip_prefix('#')
        .filter(|name| !name.is_empty())?;
    let start = node.start_position();
    Some(markup_occurrence(
        DefinitionTarget::Id(xml_unescape(name)),
        OccurrenceRole::Reference,
        base_byte + node.start_byte() + 1,
        base_byte + node.end_byte(),
        span_between(
            rebase_point(base_point, Point::new(start.row, start.column + 1)),
            rebase_point(base_point, node.end_position()),
        ),
    ))
}

pub fn parse_paint(payload: &[u8]) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg_paint::LANGUAGE.into())
        .ok()?;
    parser.parse(payload, None)
}

fn paint_occurrences(
    payload: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut Vec<SymbolOccurrence>,
) {
    let Some(payload_source) = source.get(payload.byte_range()) else {
        return;
    };
    let Some(tree) = parse_paint(payload_source) else {
        return;
    };
    let mut cursor = tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        if node.kind() == "iri_reference"
            && let Some(occurrence) = local_iri_occurrence(
                node,
                payload_source,
                payload.start_byte(),
                payload.start_position(),
            )
        {
            out.push(occurrence);
        }
    });
}

fn aria_occurrences(
    attribute: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut Vec<SymbolOccurrence>,
) {
    let Some(name) = attribute
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
    else {
        return;
    };
    if !ARIA_IDREF_ATTRIBUTES.contains(&name) {
        return;
    }
    let Some(value) = attribute.child_by_field_name("value") else {
        return;
    };
    let Some(content_node) = value.child_by_field_name("content") else {
        return;
    };
    let Ok(content) = content_node.utf8_text(source) else {
        return;
    };
    token_occurrences(
        content,
        content_node.start_byte(),
        content_node.start_position(),
        DefinitionTarget::Id,
        out,
    );
}

fn token_occurrences(
    text: &str,
    start_byte: usize,
    start_point: Point,
    target: fn(String) -> DefinitionTarget,
    out: &mut Vec<SymbolOccurrence>,
) {
    for (token, start, end) in xml_tokens(text) {
        let start_point_of = |offset: usize| advance_point(start_point, &text.as_bytes()[..offset]);
        out.push(markup_occurrence(
            target(token),
            OccurrenceRole::Reference,
            start_byte + start,
            start_byte + end,
            span_between(start_point_of(start), start_point_of(end)),
        ));
    }
}

fn timing_occurrences(
    attribute: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut Vec<SymbolOccurrence>,
) {
    let is_timing = attribute
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        .is_some_and(|name| matches!(name, "begin" | "end"));
    let Some(value) = attribute.child_by_field_name("value").filter(|_| is_timing) else {
        return;
    };
    let Ok(text) = value.utf8_text(source) else {
        return;
    };
    let (quote, content) = strip_quotes(text);
    let start = value.start_position();
    let content_point = Point::new(start.row, start.column + quote);
    let content_byte = value.start_byte() + quote;
    let (decoded, offsets) = xml_unescape_mapped(content);
    let raw = |index: usize| {
        offsets
            .as_ref()
            .map_or(Some(index), |offsets| offsets.get(index).copied())
    };
    let mut segment_start = 0;
    for segment in decoded.split(';') {
        let lead = segment.len() - segment.trim_start_matches(is_xml_whitespace).len();
        if let Some((length, id)) = syncbase_id(&segment[lead..])
            && let (Some(start), Some(end)) = (
                raw(segment_start + lead),
                raw(segment_start + lead + length),
            )
        {
            let point_at =
                |offset: usize| advance_point(content_point, &content.as_bytes()[..offset]);
            out.push(SymbolOccurrence {
                target: DefinitionTarget::Id(id),
                role: OccurrenceRole::Reference,
                syntax: OccurrenceSyntax::TimingReference,
                start_byte: content_byte + start,
                end_byte: content_byte + end,
                span: span_between(point_at(start), point_at(end)),
            });
        }
        segment_start += segment.len() + 1;
    }
}

fn syncbase_id(value: &str) -> Option<(usize, String)> {
    let mut id = String::new();
    let mut chars = value.char_indices();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '\\' => id.push(chars.next()?.1),
            '.' => {
                let follows_event =
                    value[index + 1..].starts_with(|next: char| next.is_ascii_alphabetic());
                return (follows_event && is_xml_name(&id)).then_some((index, id));
            }
            _ => id.push(ch),
        }
    }
    None
}

fn presentation_content<'a>(
    attribute: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<(tree_sitter::Node<'a>, usize, &'a str)> {
    let is_presentation = attribute
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        .and_then(svg_data::attribute)
        .is_some_and(|definition| definition.presentation_attribute.is_some());
    let value = attribute
        .child_by_field_name("value")
        .filter(|_| is_presentation)?;
    let (quote, content) = strip_quotes(value.utf8_text(source).ok()?);
    Some((value, quote, content))
}

pub fn presentation_fragment(
    attribute: tree_sitter::Node<'_>,
    source: &[u8],
) -> Option<CssFragment> {
    let (value, quote, content) = presentation_content(attribute, source)?;
    let start = value.start_position();
    CssFragment::value(
        content,
        value.start_byte() + quote,
        Point::new(start.row, start.column + quote),
    )
}

pub fn encoded_presentation_value(attribute: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    presentation_content(attribute, source).is_some_and(|(_, _, content)| content.contains('&'))
}

pub fn inside_encoded_presentation_value(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if is_attribute_node_kind(ancestor.kind()) {
            return encoded_presentation_value(ancestor, source);
        }
        current = ancestor.parent();
    }
    false
}

fn presentation_occurrences(
    attribute: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut Vec<SymbolOccurrence>,
) {
    let Some((_, _, content)) = presentation_content(attribute, source) else {
        return;
    };
    let encoded = content.contains('&');
    let decoded = xml_unescape(content).to_ascii_lowercase();
    if !(decoded.contains("var(") || (encoded && decoded.contains("url("))) {
        return;
    }
    if let Some(fragment) = presentation_fragment(attribute, source) {
        let mut cursor = fragment.tree.root_node().walk();
        walk_tree(&mut cursor, &mut |node| {
            if node.kind() == "call_expression" {
                call_occurrence(&fragment, node, encoded, out);
            }
        });
    }
}

fn var_occurrence(
    fragment: &CssFragment,
    call: tree_sitter::Node<'_>,
    out: &mut Vec<SymbolOccurrence>,
) {
    let is_var = child_of_kind(call, "function_name")
        .and_then(|function| fragment.node_text(function))
        .is_some_and(|function| function.eq_ignore_ascii_case("var"));
    let Some(arguments) = child_of_kind(call, "arguments").filter(|_| is_var) else {
        return;
    };
    let mut args = arguments.walk();
    if let Some(first) = arguments
        .named_children(&mut args)
        .find(|argument| argument.kind() != "comment")
        && first.kind() == "plain_value"
        && fragment
            .node_text(first)
            .is_some_and(|text| text.starts_with("--"))
    {
        push_css_identifier(
            fragment,
            first,
            OccurrenceRole::Reference,
            DefinitionTarget::CustomProperty,
            out,
        );
    }
}

pub fn css_occurrences(fragment: &CssFragment, out: &mut Vec<SymbolOccurrence>) {
    let mut cursor = fragment.tree.root_node().walk();
    walk_tree(&mut cursor, &mut |node| {
        let parent_kind = node.parent().map(|parent| parent.kind());
        match node.kind() {
            "class_name" if parent_kind == Some("class_selector") => {
                push_css_identifier(
                    fragment,
                    node,
                    OccurrenceRole::Declaration,
                    DefinitionTarget::Class,
                    out,
                );
            }
            "id_name" if parent_kind == Some("id_selector") => {
                push_css_identifier(
                    fragment,
                    node,
                    OccurrenceRole::Reference,
                    DefinitionTarget::Id,
                    out,
                );
            }
            "property_name"
                if fragment
                    .node_text(node)
                    .is_some_and(|text| text.starts_with("--")) =>
            {
                push_css_identifier(
                    fragment,
                    node,
                    OccurrenceRole::Declaration,
                    DefinitionTarget::CustomProperty,
                    out,
                );
            }
            "call_expression" => call_occurrence(fragment, node, true, out),
            _ => {}
        }
    });
}

fn call_occurrence(
    fragment: &CssFragment,
    call: tree_sitter::Node<'_>,
    urls: bool,
    out: &mut Vec<SymbolOccurrence>,
) {
    let Some(function) =
        child_of_kind(call, "function_name").and_then(|function| fragment.node_text(function))
    else {
        return;
    };
    let Some(arguments) = child_of_kind(call, "arguments") else {
        return;
    };
    if function.eq_ignore_ascii_case("var") {
        var_occurrence(fragment, call, out);
    } else if urls
        && function.eq_ignore_ascii_case("url")
        && let Some((start, end)) = url_argument_range(fragment, arguments)
        && let Some(fragment_text) = fragment.text.get(start..end)
        && let Some(name) = fragment_text.strip_prefix('#').filter(|n| !n.is_empty())
        && let Some((source_start, source_end, span)) = fragment.source_range(start + 1, end)
    {
        out.push(SymbolOccurrence {
            target: DefinitionTarget::Id(css_unescape(name)),
            role: OccurrenceRole::Reference,
            syntax: OccurrenceSyntax::CssUrl {
                xml_escaped: fragment.xml_escaped,
            },
            start_byte: source_start,
            end_byte: source_end,
            span,
        });
    }
}

pub fn url_argument_range(
    fragment: &CssFragment,
    arguments: tree_sitter::Node<'_>,
) -> Option<(usize, usize)> {
    let text = fragment.node_text(arguments)?;
    let inner = text.strip_prefix('(')?;
    let inner = inner.strip_suffix(')').unwrap_or(inner);
    let leading = inner.len() - inner.trim_start().len();
    let trimmed = inner.trim();
    let base = arguments.start_byte() + 1 + leading;
    let (quote_offset, content) = strip_quotes(trimmed);
    if quote_offset == 0 && trimmed.starts_with(['"', '\'']) {
        return None;
    }
    if content.is_empty() {
        return None;
    }
    let start = base + quote_offset;
    Some((start, start + content.len()))
}

fn push_css_identifier(
    fragment: &CssFragment,
    node: tree_sitter::Node<'_>,
    role: OccurrenceRole,
    target: fn(String) -> DefinitionTarget,
    out: &mut Vec<SymbolOccurrence>,
) {
    let Some(text) = fragment.node_text(node) else {
        return;
    };
    let (Some(start_byte), Some(end_byte), Some(start), Some(end)) = (
        fragment.source_byte(node.start_byte()),
        fragment.source_byte(node.end_byte()),
        fragment.source_point(node.start_byte(), node.start_position()),
        fragment.source_point(node.end_byte(), node.end_position()),
    ) else {
        return;
    };
    out.push(SymbolOccurrence {
        target: target(css_unescape(text)),
        role,
        syntax: OccurrenceSyntax::CssIdentifier {
            xml_escaped: fragment.xml_escaped,
        },
        start_byte,
        end_byte,
        span: span_between(start, end),
    });
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    fn parse_svg(source: &str) -> Result<Tree, Box<dyn Error>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
        parser
            .parse(source, None)
            .ok_or_else(|| "parse returned None".into())
    }

    fn texts<'a>(source: &'a str, occurrences: &[SymbolOccurrence]) -> Vec<&'a str> {
        occurrences
            .iter()
            .map(|occurrence| &source[occurrence.start_byte..occurrence.end_byte])
            .collect()
    }

    fn of_target<'a>(
        occurrences: &'a [SymbolOccurrence],
        target: &DefinitionTarget,
    ) -> Vec<&'a SymbolOccurrence> {
        occurrences
            .iter()
            .filter(|occurrence| occurrence.target == *target)
            .collect()
    }

    fn span_text<'a>(source: &'a str, span: &Span) -> &'a str {
        let lines: Vec<&str> = source.split('\n').collect();
        assert_eq!(span.start_row, span.end_row, "single-line span expected");
        &lines[span.start_row][span.start_col..span.end_col]
    }

    #[test]
    fn collects_id_declaration_and_all_reference_forms() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg>\n",
            "  <linearGradient id=\"g\"/>\n",
            "  <style>#g { fill: url(#g); } .x { stroke: url( \"#g\" ); }</style>\n",
            "  <rect fill=\"url(#g) red\" clip-path=\"url(#g)\" style=\"fill: url(#g)\"/>\n",
            "  <use href=\"#g\" aria-labelledby=\"a g\"/>\n",
            "</svg>\n",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let ids = of_target(&occurrences, &DefinitionTarget::Id("g".into()));
        assert_eq!(ids.len(), 9, "{ids:#?}");
        assert_eq!(
            ids.iter()
                .filter(|o| o.role == OccurrenceRole::Declaration)
                .count(),
            1
        );
        for occurrence in &ids {
            assert_eq!(&source[occurrence.start_byte..occurrence.end_byte], "g");
            assert_eq!(span_text(source, &occurrence.span), "g", "{occurrence:?}");
        }
        Ok(())
    }

    #[test]
    fn collects_classes_from_attributes_and_selectors() -> Result<(), Box<dyn Error>> {
        let source = "<svg><style>.a, .b:hover, .a\\.b {}</style><g class=\"a b a.b\"/></svg>";
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let classes: Vec<_> = occurrences
            .iter()
            .filter_map(|o| match &o.target {
                DefinitionTarget::Class(name) => Some((name.as_str(), o.role)),
                _ => None,
            })
            .collect();
        assert_eq!(
            classes,
            vec![
                ("a", OccurrenceRole::Declaration),
                ("b", OccurrenceRole::Declaration),
                ("a.b", OccurrenceRole::Declaration),
                ("a", OccurrenceRole::Reference),
                ("b", OccurrenceRole::Reference),
                ("a.b", OccurrenceRole::Reference),
            ]
        );
        Ok(())
    }

    #[test]
    fn collects_custom_properties_from_stylesheet_and_style_attribute() -> Result<(), Box<dyn Error>>
    {
        let source = concat!(
            "<svg><style>:root { --c: red; } .a { fill: var(--c, blue); }</style>",
            "<rect style=\"--c: blue; stroke: var(--c)\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let props = of_target(
            &occurrences,
            &DefinitionTarget::CustomProperty("--c".into()),
        );
        let roles: Vec<_> = props.iter().map(|o| o.role).collect();
        assert_eq!(
            roles,
            vec![
                OccurrenceRole::Declaration,
                OccurrenceRole::Reference,
                OccurrenceRole::Declaration,
                OccurrenceRole::Reference,
            ]
        );
        assert!(
            texts(source, &occurrences)
                .iter()
                .all(|text| !text.is_empty())
        );
        Ok(())
    }

    #[test]
    fn decodes_xml_references_before_matching() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><style>.a\\&b { fill: red }</style>",
            "<g class=\"a&amp;b\" id=\"iA\" aria-owns=\"i&#65;\"/>",
            "<use href=\"#iA\"/><rect style=\"fill: url(#i&#65;)\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let class = of_target(&occurrences, &DefinitionTarget::Class("a&b".into()));
        assert_eq!(
            texts(source, &class.iter().copied().cloned().collect::<Vec<_>>()),
            vec!["a\\&b", "a&amp;b"]
        );
        let id = of_target(&occurrences, &DefinitionTarget::Id("iA".into()));
        assert_eq!(
            texts(source, &id.iter().copied().cloned().collect::<Vec<_>>()),
            vec!["iA", "i&#65;", "iA", "i&#65;"]
        );
        let edits = rename_edits(&occurrences, &DefinitionTarget::Class("a&b".into()), "c&d")?;
        let texts: Vec<_> = edits.iter().map(|edit| edit.new_text.as_str()).collect();
        assert_eq!(texts, vec!["c\\&amp;d", "c&amp;d"]);
        Ok(())
    }

    #[test]
    fn css_urls_decode_xml_then_css_escapes() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><linearGradient id=\"grad\"/>",
            "<style>rect { fill: url('#gr\\61 d') }</style>",
            "<rect style=\"content: &quot;x&quot;; fill: url(&quot;#grad&quot;)\"/>",
            "<rect style=\"stroke: url(#gr&#x61;d)\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let grad = of_target(&occurrences, &DefinitionTarget::Id("grad".into()));
        let spans: Vec<_> = grad.iter().map(|o| span_text(source, &o.span)).collect();
        assert_eq!(spans, vec!["grad", "gr\\61 d", "grad", "gr&#x61;d"]);
        let edits = rename_edits(&occurrences, &DefinitionTarget::Id("grad".into()), "sky")?;
        assert_eq!(edits.len(), 4);
        Ok(())
    }

    #[test]
    fn class_and_aria_tokens_split_after_decoding() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><style>.a {fill:red}</style>",
            "<g id=\"b\" class=\"a&#32;b\" aria-owns=\"b&#x9;c\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let named = |target: DefinitionTarget| -> Vec<&str> {
            of_target(&occurrences, &target)
                .iter()
                .map(|o| span_text(source, &o.span))
                .collect()
        };
        assert_eq!(named(DefinitionTarget::Class("a".into())), vec!["a", "a"]);
        assert_eq!(named(DefinitionTarget::Class("b".into())), vec!["b"]);
        assert_eq!(named(DefinitionTarget::Id("b".into())), vec!["b", "b"]);
        assert_eq!(named(DefinitionTarget::Id("c".into())), vec!["c"]);
        Ok(())
    }

    #[test]
    fn var_skips_leading_comments() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><style>:root { --c: red } rect { fill: var(/* theme */ --c) }</style>",
            "<rect fill=\"var(/**/--c)\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let props = of_target(
            &occurrences,
            &DefinitionTarget::CustomProperty("--c".into()),
        );
        let roles: Vec<_> = props
            .iter()
            .map(|o| (o.role, span_text(source, &o.span)))
            .collect();
        assert_eq!(
            roles,
            vec![
                (OccurrenceRole::Declaration, "--c"),
                (OccurrenceRole::Reference, "--c"),
                (OccurrenceRole::Reference, "--c"),
            ]
        );
        Ok(())
    }

    #[test]
    fn collects_ids_from_animation_timing() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><rect id=\"btn\"/><rect id=\"a-b\"/>",
            "<animate begin=\"btn.click; 2s; a\\-b.end+1s; wallclock(2020-01-01T00:00:00.5Z); \
             indefinite\" end=\"btn&#46;begin\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let named = |name: &str| -> Vec<&str> {
            of_target(&occurrences, &DefinitionTarget::Id(name.into()))
                .iter()
                .map(|o| span_text(source, &o.span))
                .collect()
        };
        assert_eq!(named("btn"), vec!["btn", "btn", "btn"]);
        assert_eq!(named("a-b"), vec!["a-b", "a\\-b"]);
        let edits = rename_edits(&occurrences, &DefinitionTarget::Id("a-b".into()), "c.d")?;
        let texts: Vec<_> = edits.iter().map(|edit| edit.new_text.as_str()).collect();
        assert_eq!(texts, vec!["c.d", "c\\.d"]);
        Ok(())
    }

    #[test]
    fn presentation_vars_are_found_after_decoding() -> Result<(), Box<dyn Error>> {
        let source = "<svg><style>:root { --c: red }</style><rect fill=\"v&#97;r(--c)\"/></svg>";
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let props = of_target(
            &occurrences,
            &DefinitionTarget::CustomProperty("--c".into()),
        );
        assert_eq!(props.len(), 2, "{occurrences:#?}");
        Ok(())
    }

    #[test]
    fn encoded_presentation_urls_are_parsed_after_decoding() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><linearGradient id=\"g\"/>",
            "<rect fill=\"url(&quot;#g&quot;) red\" clip-path=\"url(#g)\" mask=\"url(#&#x67;)\"/>",
            "<rect stroke=\"url('#g')\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let spans: Vec<_> = of_target(&occurrences, &DefinitionTarget::Id("g".into()))
            .iter()
            .map(|o| span_text(source, &o.span))
            .collect();
        assert_eq!(
            spans,
            vec!["g", "g", "g", "&#x67;", "g"],
            "{occurrences:#?}"
        );
        let edits = rename_edits(&occurrences, &DefinitionTarget::Id("g".into()), "h")?;
        assert_eq!(edits.len(), 5);
        Ok(())
    }

    #[test]
    fn cdata_stylesheets_are_not_xml_escaped() -> Result<(), Box<dyn Error>> {
        let source = "<svg><style><![CDATA[.a\\&b {}]]></style><g class=\"a&amp;b\"/></svg>";
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let class = of_target(&occurrences, &DefinitionTarget::Class("a&b".into()));
        assert_eq!(class.len(), 2, "{occurrences:#?}");
        assert_eq!(
            class[0].syntax,
            OccurrenceSyntax::CssIdentifier { xml_escaped: false }
        );
        let edits = rename_edits(&occurrences, &DefinitionTarget::Class("a&b".into()), "c&d")?;
        let texts: Vec<_> = edits.iter().map(|edit| edit.new_text.as_str()).collect();
        assert_eq!(texts, vec!["c\\&d", "c&amp;d"]);
        Ok(())
    }

    #[test]
    fn collects_custom_properties_from_presentation_attributes() -> Result<(), Box<dyn Error>> {
        let source = concat!(
            "<svg><style>:root { --c: red }</style>",
            "<rect fill=\"var(--c)\" stroke-width='var(--c, 2)' title=\"var(--c)\"/>",
            "<path d=\"M0 0\" opacity=\"var(--c-x)\"/></svg>",
        );
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let props = of_target(
            &occurrences,
            &DefinitionTarget::CustomProperty("--c".into()),
        );
        let found: Vec<_> = props
            .iter()
            .map(|o| (o.role, span_text(source, &o.span)))
            .collect();
        assert_eq!(
            found,
            vec![
                (OccurrenceRole::Declaration, "--c"),
                (OccurrenceRole::Reference, "--c"),
                (OccurrenceRole::Reference, "--c"),
            ]
        );
        assert_eq!(
            of_target(
                &occurrences,
                &DefinitionTarget::CustomProperty("--c-x".into())
            )
            .len(),
            1
        );
        let edits = rename_edits(
            &occurrences,
            &DefinitionTarget::CustomProperty("--c".into()),
            "--d",
        )?;
        assert_eq!(edits.len(), 3);
        Ok(())
    }

    #[test]
    fn ignores_external_iris() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><use href="other.svg#g"/><rect fill="url(other.svg#g)"/></svg>"#;
        let tree = parse_svg(source)?;
        assert_eq!(
            collect_symbol_occurrences(source.as_bytes(), &tree),
            Vec::<SymbolOccurrence>::new()
        );
        Ok(())
    }

    #[test]
    fn multiline_stylesheet_spans_rebase_rows() -> Result<(), Box<dyn Error>> {
        let source = "<svg>\n<style>\n  .a {}\n  #b {}\n</style>\n</svg>";
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let spans: Vec<_> = occurrences.iter().map(|o| o.span.clone()).collect();
        assert_eq!(
            spans,
            vec![
                Span {
                    start_row: 2,
                    start_col: 3,
                    end_row: 2,
                    end_col: 4
                },
                Span {
                    start_row: 3,
                    start_col: 3,
                    end_row: 3,
                    end_col: 4
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn occurrence_at_accepts_cursor_at_name_end() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><g id="abc"/></svg>"#;
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let start = source.find("abc").ok_or("abc")?;
        for offset in [start, start + 1, start + 3] {
            assert!(
                symbol_occurrence_at(&occurrences, offset).is_some(),
                "{offset}"
            );
        }
        assert!(symbol_occurrence_at(&occurrences, start - 1).is_none());
        Ok(())
    }

    #[test]
    fn validates_new_names_per_symbol_kind() {
        let id = DefinitionTarget::Id(String::new());
        let class = DefinitionTarget::Class(String::new());
        let prop = DefinitionTarget::CustomProperty(String::new());
        assert!(validate_new_name(&id, "ok-1").is_ok());
        assert_eq!(
            validate_new_name(&id, "1bad"),
            Err(RenameError::InvalidId("1bad".into()))
        );
        assert!(validate_new_name(&class, "a.b").is_ok());
        assert!(validate_new_name(&class, "a b").is_err());
        assert!(validate_new_name(&class, "").is_err());
        assert!(validate_new_name(&prop, "--x").is_ok());
        assert!(validate_new_name(&prop, "--").is_err());
        assert!(validate_new_name(&prop, "x").is_err());
        assert!(
            RenameError::InvalidId("1bad".into())
                .to_string()
                .contains("XML name")
        );
    }

    #[test]
    fn rename_escapes_per_syntax() -> Result<(), Box<dyn Error>> {
        let source = r#"<svg><style>.a {}</style><g class="a" style="--p: 1"/></svg>"#;
        let tree = parse_svg(source)?;
        let occurrences = collect_symbol_occurrences(source.as_bytes(), &tree);
        let edits = rename_edits(&occurrences, &DefinitionTarget::Class("a".into()), "x.\"y")?;
        let texts: Vec<_> = edits.iter().map(|edit| edit.new_text.as_str()).collect();
        assert_eq!(texts, vec!["x\\.\\&quot;y", "x.&quot;y"]);
        assert_eq!(
            rename_edits(&occurrences, &DefinitionTarget::Class("a".into()), "x y"),
            Err(RenameError::InvalidClass("x y".into()))
        );
        Ok(())
    }
}
