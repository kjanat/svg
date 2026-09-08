use std::{fmt::Write as _, sync::LazyLock};

use svg_data::effective_compat::{self, BrowserSupport, BrowserVersion, Facts};
use svg_data::{BaselineQualifier, BaselineTier, ProfileLookup, SpecLifecycle, SpecSnapshotId};
use tower_lsp_server::ls_types::Uri;
use url::Url;

use crate::{
    clipboard::svg_data_uri,
    compat::{CompatOverride, Outcome},
    positions::byte_offset_for_row_col,
    stylesheets::{ClassDefinitionHover, CustomPropertyDefinitionHover},
};

struct HoverSourceLink {
    label: String,
    target: String,
}

fn direct_hover_source_link(uri: &Uri, line: usize) -> HoverSourceLink {
    HoverSourceLink {
        label: format!("{}:{line}", uri.as_str()),
        target: uri.as_str().to_owned(),
    }
}

pub fn format_class_hover(class_name: &str, definitions: &[ClassDefinitionHover]) -> String {
    format_definition_hover(
        definitions.iter().map(|definition| {
            (
                css_rule_snippet(&definition.source, &definition.definition.span),
                hover_source_link(&definition.uri, definition.definition.span.start_row),
            )
        }),
        &format!(".{class_name}"),
    )
}

pub fn format_custom_property_hover(
    property_name: &str,
    definitions: &[CustomPropertyDefinitionHover],
) -> String {
    format_definition_hover(
        definitions.iter().map(|definition| {
            (
                css_declaration_snippet(&definition.source, &definition.definition.span),
                hover_source_link(&definition.uri, definition.definition.span.start_row),
            )
        }),
        property_name,
    )
}

fn format_definition_hover(
    definitions: impl Iterator<Item = (String, HoverSourceLink)>,
    fallback_label: &str,
) -> String {
    let sections: Vec<String> = definitions
        .map(|(snippet, source)| {
            let trimmed = snippet.trim();
            let mut section = String::new();
            if trimmed.is_empty() {
                let _ = write!(section, "`{fallback_label}`");
            } else {
                section.push_str("```css\n");
                section.push_str(trimmed);
                section.push_str("\n```");
            }
            section.push_str("\nDefined in [");
            section.push_str(&source.label);
            section.push_str("](");
            section.push_str(&source.target);
            section.push(')');
            section
        })
        .collect();

    sections.join("\n\n---\n\n")
}

fn hover_source_link(uri: &Uri, start_row: usize) -> HoverSourceLink {
    let line = start_row + 1;
    let Ok(url) = Url::parse(uri.as_str()) else {
        return direct_hover_source_link(uri, line);
    };

    match url.scheme() {
        "file" => {
            let target = format!("{url}#L{line}");

            // Prefer a cwd-relative label when the URL can be resolved to a
            // filesystem path. `url::Url::to_file_path` requires a drive letter
            // on Windows, so this optimization only kicks in for well-formed
            // absolute URIs on the current platform.
            if let Ok(path) = url.to_file_path()
                && let Ok(cwd) = std::env::current_dir()
                && let Ok(relative) = path.strip_prefix(&cwd)
            {
                return HoverSourceLink {
                    label: format!("{}:{line}", relative.display()),
                    target,
                };
            }

            // Otherwise fall back to the URL path's final segment. Works on
            // every platform, including Windows `file:///foo.svg` URIs without
            // a drive letter (where `to_file_path()` returns Err).
            let url_path = url.path();
            let basename = url_path.rsplit('/').find(|seg| !seg.is_empty());
            let label = basename.map_or_else(|| url_path.to_owned(), ToOwned::to_owned);
            HoverSourceLink {
                label: format!("{label}:{line}"),
                target,
            }
        }
        "http" | "https" => {
            let host = url.host_str().unwrap_or_default();
            HoverSourceLink {
                label: format!("{host}{}:{line}", url.path()),
                target: format!("{url}#L{line}"),
            }
        }
        _ => direct_hover_source_link(uri, line),
    }
}

fn css_rule_snippet(source: &str, span: &svg_references::Span) -> String {
    let source_bytes = source.as_bytes();
    let start = byte_offset_for_row_col(source_bytes, span.start_row, span.start_col);
    if start >= source_bytes.len() {
        return String::new();
    }

    if let Some(block_open) = source_bytes[start..]
        .iter()
        .position(|&byte| byte == b'{')
        .map(|offset| start + offset)
    {
        let selector_start = source_bytes[..start]
            .iter()
            .rposition(|&byte| byte == b'}')
            .map_or(0, |idx| idx + 1);

        if let Some(block_end) = matching_brace_end(source_bytes, block_open) {
            return source[selector_start..block_end].trim().to_owned();
        }
    }

    line_text_at(source, span.start_row)
}

fn css_declaration_snippet(source: &str, span: &svg_references::Span) -> String {
    let source_bytes = source.as_bytes();
    let start = byte_offset_for_row_col(source_bytes, span.start_row, span.start_col);
    if start >= source_bytes.len() {
        return String::new();
    }

    let declaration_start = source_bytes[..start]
        .iter()
        .rposition(|&byte| matches!(byte, b';' | b'{'))
        .map_or(0, |idx| idx + 1);
    let declaration_end = source_bytes[start..]
        .iter()
        .position(|&byte| matches!(byte, b';' | b'}'))
        .map_or(source_bytes.len(), |idx| start + idx);

    source[declaration_start..declaration_end].trim().to_owned()
}

// `matching_brace_end` is a cheap brace counter for hover snippets. It does
// not handle braces inside CSS strings/comments, so edge cases may truncate the
// snippet, but that is preferable here to a full CSS reparse on hover.
fn matching_brace_end(source: &[u8], open_index: usize) -> Option<usize> {
    let mut depth = 0usize;

    for (idx, byte) in source.iter().enumerate().skip(open_index) {
        match *byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(idx + 1);
                }
            }
            _ => {}
        }
    }

    None
}

fn line_text_at(source: &str, row: usize) -> String {
    source
        .lines()
        .nth(row)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Typed sections of a compatibility hover payload. Each variant renders
/// to its own self-contained block of markdown; [`CompatMarkdownBuilder`]
/// joins them with exactly one blank line between each, so sections never
/// need to push their own leading/trailing whitespace.
///
/// The enum replaces the loose `parts.push(String::new())` pattern: the
/// blank-line discipline is encoded once in `build()` rather than repeated
/// at every call site, and the set of legal sections is closed, so future
/// additions must land here (visible to every reviewer) rather than as
/// ad-hoc string pushes.
enum HoverSection {
    /// Blockquote-quoted verdict headline. Rendered markdown looks
    /// like `> ✗ baseProfile — removed from the current SVG profile`.
    Headline(String),
    /// Plain-text MDN-style description. First prose block.
    Description(String),
    /// Consolidated `**Status:** reason · reason` line, or legacy profile-lifecycle fallback.
    Status(String),
    /// Attribute value constraints (`Values: ...`, `Functions: ...`, or paired `Alignments:`/`Scaling:`).
    /// Rendered as consecutive lines with NO blank between them.
    ValueConstraints(Vec<String>),
    /// Pre-rendered baseline row with icon data-URI.
    Baseline(String),
    /// Single-line `Chrome ≤80 · Edge ≤80 · Firefox ✗ · Safari ≤13.1` chip row.
    BrowserChips(String),
    /// Per-browser sub-bullets for partial/prefix/flags/notes caveats.
    /// Rendered as consecutive lines with NO blank between them.
    BrowserNotes(Vec<String>),
    /// Footer links (MDN · Spec), joined with ` · ` into a single line.
    Links(Vec<String>),
}

/// Structured builder for compatibility-hover markdown. Call sites push
/// [`HoverSection`]s in display order; [`Self::build`] renders each
/// section and joins with `\n\n`.
///
/// The builder is deliberately thin — it owns *ordering* and *spacing*,
/// not section content. Each `push_*` helper only accepts a pre-formatted
/// payload so that the heavy formatting (verdict glyphs, baseline icons,
/// per-browser notes) stays in its dedicated function and can be unit-
/// tested in isolation.
struct CompatMarkdownBuilder {
    sections: Vec<HoverSection>,
}

impl CompatMarkdownBuilder {
    const fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    fn headline(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::Headline(line));
        self
    }

    fn description(&mut self, line: String) -> &mut Self {
        if !line.is_empty() {
            self.sections.push(HoverSection::Description(line));
        }
        self
    }

    fn status(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::Status(line));
        self
    }

    fn value_constraints(&mut self, lines: Vec<String>) -> &mut Self {
        if !lines.is_empty() {
            self.sections.push(HoverSection::ValueConstraints(lines));
        }
        self
    }

    fn baseline(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::Baseline(line));
        self
    }

    fn browser_chips(&mut self, line: String) -> &mut Self {
        self.sections.push(HoverSection::BrowserChips(line));
        self
    }

    fn browser_notes(&mut self, lines: Vec<String>) -> &mut Self {
        if !lines.is_empty() {
            self.sections.push(HoverSection::BrowserNotes(lines));
        }
        self
    }

    fn links(&mut self, lines: Vec<String>) -> &mut Self {
        if !lines.is_empty() {
            self.sections.push(HoverSection::Links(lines));
        }
        self
    }

    fn build(self) -> String {
        let rendered: Vec<String> = self
            .sections
            .into_iter()
            .map(|section| match section {
                HoverSection::Headline(line)
                | HoverSection::Description(line)
                | HoverSection::Status(line)
                | HoverSection::Baseline(line)
                | HoverSection::BrowserChips(line) => line,
                HoverSection::ValueConstraints(lines) | HoverSection::BrowserNotes(lines) => {
                    lines.join("\n")
                }
                HoverSection::Links(lines) => lines.join(" · "),
            })
            .collect();
        rendered.join("\n\n")
    }
}

/// Build the `[MDN Reference](…) · [Spec](…)` link list. Keeps both call
/// sites from duplicating the tiny `spec_url` fallback.
fn hover_link_list(mdn_url: &str, spec_url: Option<&str>) -> Vec<String> {
    let mut links = vec![format!("[MDN Reference]({mdn_url})")];
    if let Some(spec_url) = spec_url {
        links.push(format!("[Spec]({spec_url})"));
    }
    links
}

/// Render the attribute value-constraint block as zero, one, or two lines
/// depending on which [`AttributeValues`] variant is present.
fn value_constraints_lines(values: &svg_data::AttributeValues) -> Vec<String> {
    match values {
        svg_data::AttributeValues::Enum(vals) => {
            vec![format!("Values: `{}`", vals.join("` | `"))]
        }
        svg_data::AttributeValues::Transform(funcs) => {
            vec![format!("Functions: `{}`", funcs.join("` | `"))]
        }
        svg_data::AttributeValues::PreserveAspectRatio {
            alignments,
            meet_or_slice,
        } => vec![
            format!("Alignments: `{}`", alignments.join("` | `")),
            format!("Scaling: `{}`", meet_or_slice.join("` | `")),
        ],
        svg_data::AttributeValues::Boolean => vec!["Value: HTML boolean attribute".to_owned()],
        svg_data::AttributeValues::TokenList => vec!["Value: space-separated tokens".to_owned()],
        svg_data::AttributeValues::CommaTokenList => {
            vec!["Value: comma-separated tokens".to_owned()]
        }
        svg_data::AttributeValues::UrlTokenList => {
            vec!["Value: space-separated URL tokens".to_owned()]
        }
        svg_data::AttributeValues::LanguageTag => vec!["Value: BCP 47 language tag".to_owned()],
        svg_data::AttributeValues::Integer => vec!["Value: integer".to_owned()],
        svg_data::AttributeValues::Number => vec!["Value: number".to_owned()],
        svg_data::AttributeValues::IdList => {
            vec!["Value: space-separated ID references".to_owned()]
        }
        svg_data::AttributeValues::MediaType => vec!["Value: media type".to_owned()],
        svg_data::AttributeValues::MediaQueryList => vec!["Value: CSS media query list".to_owned()],
        svg_data::AttributeValues::CssDeclarationList => {
            vec!["Value: CSS declaration list".to_owned()]
        }
        svg_data::AttributeValues::Id => vec!["Value: non-empty ID without whitespace".to_owned()],
        svg_data::AttributeValues::ReferrerPolicy => vec!["Value: referrer policy".to_owned()],
        svg_data::AttributeValues::SuggestedFileName => {
            vec!["Value: suggested download file name".to_owned()]
        }
        svg_data::AttributeValues::PathData => vec!["Value: SVG path data".to_owned()],
        svg_data::AttributeValues::SemicolonNumberList => {
            vec!["Value: semicolon-separated number list".to_owned()]
        }
        svg_data::AttributeValues::CoordinatePair => {
            vec!["Value: coordinate pair".to_owned()]
        }
        svg_data::AttributeValues::CoordinatePairList => {
            vec!["Value: semicolon-separated coordinate pairs".to_owned()]
        }
        svg_data::AttributeValues::CssGrammar { grammar, graph } => {
            let mut lines = vec![format!("Grammar: `{grammar}`")];
            let keywords = css_graph_node_text(graph, svg_data::CssGrammarNodeKind::Keyword);
            if !keywords.is_empty() {
                lines.push(format!("Keywords: `{}`", keywords.join("` | `")));
            }
            let types = css_graph_node_text(graph, svg_data::CssGrammarNodeKind::Type);
            if !types.is_empty() {
                lines.push(format!("Types: `{}`", types.join("` | `")));
            }
            let functions = css_graph_node_text(graph, svg_data::CssGrammarNodeKind::Function);
            if !functions.is_empty() {
                lines.push(format!("Functions: `{}`", functions.join("` | `")));
            }
            lines
        }
        _ => Vec::new(),
    }
}

fn css_graph_node_text(
    graph: &svg_data::CssGrammarGraph,
    kind: svg_data::CssGrammarNodeKind,
) -> Vec<&'static str> {
    let mut values: Vec<&'static str> = graph
        .nodes
        .iter()
        .filter(|node| node.kind == kind)
        .filter_map(|node| node.text)
        .collect();
    values.sort_unstable();
    values.dedup();
    values
}

pub fn format_element_hover_with_profile(
    el: &svg_data::ElementDef,
    profile: SpecSnapshotId,
    profile_lifecycle: Option<String>,
    rt: Option<&CompatOverride>,
    native: Option<&svg_data::profile::SvgNative>,
) -> String {
    let baked = Facts::from(svg_data::CompatFacts {
        deprecated: el.deprecated,
        experimental: el.experimental,
        standard_track: el.standard_track,
        baseline: el.baseline,
        discouraged: el.discouraged,
        browser_support: el.browser_support,
    });
    let facts = rt.map_or(&baked, |r| &r.facts);
    let baseline = facts
        .baseline
        .as_ref()
        .map(svg_data::compat_model::Baseline::as_ref);
    let verdict = effective_compat::verdict(facts);
    let _ = profile;

    let mut builder = CompatMarkdownBuilder::new();

    if let Some(v) = verdict.as_ref() {
        builder.headline(format_verdict_headline(v, el.name));
    }
    builder.description(el.description.to_owned());

    if native.is_some_and(|n| n.is_unsupported(svg_data::profile::ConstraintKind::Element, el.name))
    {
        builder.status("⚠ Not supported by the SVG Native profile".to_owned());
    }

    if let Some(status) = reconciled_status(verdict.as_ref(), profile_lifecycle) {
        builder.status(status);
    }

    if let Some(advice) = format_discouraged(&facts.discouraged) {
        builder.baseline(advice);
    } else if let Some(baseline) = baseline {
        builder.baseline(format_baseline(baseline));
    }
    if let Some(line) = format_browser_support_line(facts.browser_support.as_ref()) {
        builder.browser_chips(line);
    }
    if let Some(notes) = format_browser_notes_list(facts.browser_support.as_ref()) {
        builder.browser_notes(notes);
    }
    append_provenance(&mut builder, rt);

    builder.links(hover_link_list(el.mdn_url, el.spec_url));

    builder.build()
}

/// Render the attribute hover markdown for the active profile.
///
/// Resolves the attribute's value constraints through
/// [`svg_data::AttributeDef::values_for_profile`] so per-snapshot value
/// overrides surface in the hover panel, then layers profile-lifecycle text,
/// baseline status, and browser-support chips (optionally overridden by the
/// runtime compat overlay `rt`) on top.
pub fn format_attribute_hover_with_profile_name(
    attr: &svg_data::AttributeDef,
    display_name: &str,
    element_name: Option<&str>,
    profile: SpecSnapshotId,
    profile_lifecycle: Option<String>,
    rt: Option<&CompatOverride>,
    native: Option<&svg_data::profile::SvgNative>,
) -> String {
    let verdict = svg_data::compat_verdict_for_attribute_on_element(attr, element_name, profile);
    format_attribute_hover_with_verdict(
        attr,
        display_name,
        element_name,
        profile,
        profile_lifecycle,
        rt,
        native,
        verdict.as_ref(),
    )
}

pub struct UnsupportedAttributeHoverProfile<'a> {
    pub profile: SpecSnapshotId,
    pub known_in: &'static [SpecSnapshotId],
    pub profile_lifecycle: Option<String>,
    pub rt: Option<&'a CompatOverride>,
    pub native: Option<&'a svg_data::profile::SvgNative>,
}

pub fn format_unsupported_attribute_hover_with_profile_name(
    attr: &svg_data::AttributeDef,
    display_name: &str,
    element_name: Option<&str>,
    profile: UnsupportedAttributeHoverProfile<'_>,
) -> String {
    let verdict = profile_unsupported_attribute_verdict(
        attr,
        element_name,
        profile.profile,
        profile.known_in,
    );
    format_attribute_hover_with_verdict(
        attr,
        display_name,
        element_name,
        profile.profile,
        profile.profile_lifecycle,
        profile.rt,
        profile.native,
        Some(&verdict),
    )
}

#[allow(clippy::too_many_arguments)]
fn format_attribute_hover_with_verdict(
    attr: &svg_data::AttributeDef,
    display_name: &str,
    element_name: Option<&str>,
    profile: SpecSnapshotId,
    profile_lifecycle: Option<String>,
    rt: Option<&CompatOverride>,
    native: Option<&svg_data::profile::SvgNative>,
    verdict: Option<&svg_data::CompatVerdict>,
) -> String {
    let baked = Facts::from(attr.compat_facts_for_element(element_name));
    let facts = rt.map_or(&baked, |r| &r.facts);
    let baseline = facts
        .baseline
        .as_ref()
        .map(svg_data::compat_model::Baseline::as_ref);
    let effective_verdict = reconcile_verdict(verdict, facts);
    let verdict = effective_verdict.as_ref();

    let mut builder = CompatMarkdownBuilder::new();

    if let Some(v) = verdict {
        builder.headline(format_verdict_headline(v, display_name));
    } else {
        builder.headline(format!("`{display_name}`"));
    }
    builder.description(attr.description.to_owned());

    if native.is_some_and(|n| {
        n.is_unsupported(svg_data::profile::ConstraintKind::Attribute, attr.name)
            || n.is_unsupported(svg_data::profile::ConstraintKind::Property, attr.name)
    }) {
        builder.status("⚠ Not supported by the SVG Native profile".to_owned());
    }

    if let Some(status) = reconciled_status(verdict, profile_lifecycle) {
        builder.status(status);
    }

    builder.value_constraints(value_constraints_lines(attr.values_for_profile(profile)));

    if let Some(advice) = format_discouraged(&facts.discouraged) {
        builder.baseline(advice);
    } else if let Some(baseline) = baseline {
        builder.baseline(format_baseline(baseline));
    }
    if let Some(line) = format_browser_support_line(facts.browser_support.as_ref()) {
        builder.browser_chips(line);
    }
    if let Some(notes) = format_browser_notes_list(facts.browser_support.as_ref()) {
        builder.browser_notes(notes);
    }
    append_provenance(&mut builder, rt);

    builder.links(hover_link_list(attr.mdn_url, attr.spec_url));

    builder.build()
}

fn profile_unsupported_attribute_verdict(
    attr: &svg_data::AttributeDef,
    element_name: Option<&str>,
    profile: SpecSnapshotId,
    known_in: &'static [SpecSnapshotId],
) -> svg_data::CompatVerdict {
    let mut reasons =
        svg_data::compat_verdict_for_attribute_on_element(attr, element_name, profile)
            .map(|verdict| verdict.reasons)
            .unwrap_or_default();

    if let Some(last_seen) = last_known_before_profile(profile, known_in)
        && !reasons
            .iter()
            .any(|reason| matches!(reason, svg_data::VerdictReason::ProfileObsolete { .. }))
    {
        reasons.push(svg_data::VerdictReason::ProfileObsolete { last_seen });
    }

    let headline_template = if reasons
        .iter()
        .any(|reason| matches!(reason, svg_data::VerdictReason::ProfileObsolete { .. }))
    {
        "removed from the current SVG profile"
    } else {
        "not in the current SVG profile"
    };

    svg_data::CompatVerdict {
        recommendation: svg_data::VerdictRecommendation::Forbid,
        headline_template,
        reasons,
    }
}

fn last_known_before_profile(
    profile: SpecSnapshotId,
    known_in: &'static [SpecSnapshotId],
) -> Option<SpecSnapshotId> {
    let selected_index = snapshot_index(profile)?;
    known_in.iter().rev().copied().find(|known| {
        snapshot_index(*known).is_some_and(|known_index| known_index < selected_index)
    })
}

pub fn profile_lifecycle_hover_line<T>(
    profile: SpecSnapshotId,
    lookup: &ProfileLookup<T>,
) -> Option<String> {
    match lookup {
        ProfileLookup::Present { lifecycle, .. } => {
            Some(format_profile_lifecycle_line(profile, *lifecycle))
        }
        ProfileLookup::UnsupportedInProfile { known_in } => {
            Some(format_unsupported_profile_lifecycle_line(profile, known_in))
        }
        ProfileLookup::Unknown => None,
    }
}

fn format_profile_lifecycle_line(profile: SpecSnapshotId, lifecycle: SpecLifecycle) -> String {
    let label = match lifecycle {
        SpecLifecycle::Stable => "Stable",
        SpecLifecycle::Experimental => "Experimental",
        SpecLifecycle::Deprecated => "Deprecated",
        SpecLifecycle::Obsolete => "Obsolete",
    };
    format!("**{label} in {}**", profile.as_str())
}

fn format_unsupported_profile_lifecycle_line(
    profile: SpecSnapshotId,
    known_in: &'static [SpecSnapshotId],
) -> String {
    let Some(selected_index) = snapshot_index(profile) else {
        return format!("**Not in {}**", profile.as_str());
    };
    let first_known = known_in.first().copied();
    let last_known = known_in.last().copied();

    if let Some(last_known) = last_known
        && snapshot_index(last_known).is_some_and(|known_index| known_index < selected_index)
    {
        return format!("**Obsolete after {}**", last_known.as_str());
    }

    if let Some(first_known) = first_known
        && snapshot_index(first_known).is_some_and(|known_index| known_index > selected_index)
    {
        return format!("**Experimental in {}**", first_known.as_str());
    }

    format!("**Not in {}**", profile.as_str())
}

fn snapshot_index(snapshot: SpecSnapshotId) -> Option<usize> {
    svg_data::spec_snapshots()
        .iter()
        .position(|candidate| *candidate == snapshot)
}

fn format_external_attribute_hover(
    description: impl AsRef<str>,
    reference_label: &str,
    reference_url: &str,
) -> String {
    format!(
        "{}\n\n[{}]({})",
        description.as_ref(),
        reference_label,
        reference_url
    )
}

fn format_deprecated_external_attribute_hover(
    description: impl AsRef<str>,
    replacement: Option<&str>,
    reference_label: &str,
    reference_url: &str,
) -> String {
    let mut parts = vec![format!("~~{}~~", description.as_ref())];
    parts.push(String::new());
    parts.push("**Deprecated**".to_owned());
    if let Some(r) = replacement {
        parts.push(String::new());
        parts.push(format!("Use `{r}` instead."));
    }
    parts.push(String::new());
    parts.push(format!("[{reference_label}]({reference_url})"));
    parts.join("\n")
}

pub fn external_attribute_hover(kind: &str, attr_name: &str) -> Option<String> {
    const XML_NAMES_URL: &str = "https://www.w3.org/TR/REC-xml-names/";
    const XML_DECL_URL: &str = "https://www.w3.org/TR/xml/";

    if let Some(markdown) = xml_declaration_attribute_hover(kind, XML_DECL_URL) {
        return Some(markdown);
    }

    if let Some(markdown) = namespace_attribute_hover(attr_name, XML_NAMES_URL) {
        return Some(markdown);
    }

    let mdn_reference_url = |name: &str| {
        format!("https://developer.mozilla.org/docs/Web/SVG/Reference/Attribute/{name}")
    };

    legacy_svg_attribute_hover(attr_name, &mdn_reference_url)
}

fn xml_declaration_attribute_hover(kind: &str, reference_url: &str) -> Option<String> {
    let description = match kind {
        "xml_version_attribute_name" => {
            "Specifies the XML version used by the document declaration."
        }
        "xml_encoding_attribute_name" => {
            "Specifies the character encoding declared for the XML document."
        }
        "xml_standalone_attribute_name" => {
            "Declares whether the XML document relies on external markup declarations."
        }
        _ => return None,
    };

    Some(format_external_attribute_hover(
        description,
        "W3C XML Reference",
        reference_url,
    ))
}

fn namespace_attribute_hover(attr_name: &str, reference_url: &str) -> Option<String> {
    if attr_name == "xmlns" {
        return Some(format_external_attribute_hover(
            "Declares the default XML namespace for this element and its descendants.",
            "W3C Namespaces in XML",
            reference_url,
        ));
    }

    attr_name.strip_prefix("xmlns:").map(|prefix| {
        format_external_attribute_hover(
            format!(
                "Declares the `{prefix}` XML namespace prefix for this element and its \
                 descendants."
            ),
            "W3C Namespaces in XML",
            reference_url,
        )
    })
}

fn legacy_svg_attribute_hover(
    attr_name: &str,
    mdn_reference_url: &impl Fn(&str) -> String,
) -> Option<String> {
    match attr_name {
        "xml:lang" => Some(format_external_attribute_hover(
            "Specifies the natural language used by the element's text content and attribute \
             values.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xml:space" => Some(format_external_attribute_hover(
            "Controls how XML whitespace is handled for the element's character data.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xml:base" => Some(format_external_attribute_hover(
            "Specifies the base URI used to resolve relative URLs within the element.",
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:href" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink form of `href` used to point at linked resources in SVG.",
            Some("href"),
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:arcrole" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that identifies the semantic role of the link arc.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:role" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that identifies the semantic role of the linked resource.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:show" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that hints how the linked resource should be presented.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:title" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that provides a human-readable title for the link.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:type" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that declares the XLink link type.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        "xlink:actuate" => Some(format_deprecated_external_attribute_hover(
            "Legacy XLink attribute that hints when the linked resource should be traversed.",
            None,
            "MDN Reference",
            &mdn_reference_url(attr_name),
        )),
        _ => None,
    }
}

static BASELINE_HIGH: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-high.svg")));
static BASELINE_LOW: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-low.svg")));
static BASELINE_LIMITED: LazyLock<String> =
    LazyLock::new(|| svg_data_uri(include_str!("../assets/baseline-limited.svg")));

/// Glyph to render before a baseline year when the upstream date
/// carried a qualifier prefix — mirrors the worker's `BaselineBadge`
/// so the LSP hover surfaces `≤2021` instead of silently lying.
const fn format_baseline_qualifier(qualifier: Option<BaselineQualifier>) -> &'static str {
    match qualifier {
        Some(BaselineQualifier::Before) => "≤",
        Some(BaselineQualifier::After) => "≥",
        Some(BaselineQualifier::Approximately) => "~",
        None => "",
    }
}

fn format_baseline(baseline: svg_data::compat_model::Baseline<&str>) -> String {
    let since = baseline
        .milestone()
        .and_then(|date| {
            date.year()
                .map(|year| format!(" since {}{year}", format_baseline_qualifier(date.qualifier)))
        })
        .unwrap_or_default();
    let mut line = match baseline.status {
        Some(BaselineTier::Widely) => format!(
            "![Baseline icon]({}) _Widely Available{since}_",
            *BASELINE_HIGH
        ),
        Some(BaselineTier::Newly) => format!(
            "![Baseline icon]({}) _Newly Available{since}_",
            *BASELINE_LOW
        ),
        Some(BaselineTier::Limited) => format!(
            "![Baseline icon]({}) _Limited availability_",
            *BASELINE_LIMITED
        ),
        None => "_Baseline status unknown_".to_owned(),
    };
    if baseline.status.is_none() {
        if let Some(raw) = baseline.raw_status {
            let _ = write!(
                line,
                " (unrecognized upstream status: {})",
                escape_metadata(raw)
            );
        } else {
            line.push_str(" (upstream status missing)");
        }
    }
    for (label, date) in [
        ("Newly Available", baseline.low_date),
        ("Widely Available", baseline.high_date),
    ] {
        if let Some(date) = date {
            let text = date.date.map_or_else(
                || format!("date not recognized (raw: {})", escape_metadata(date.raw)),
                |parsed| format!("{}{parsed}", format_baseline_qualifier(date.qualifier)),
            );
            let _ = write!(line, "\n\n{label} date: {text}");
        }
    }
    line
}

fn format_discouraged<S: AsRef<str>, L: AsRef<[S]>>(
    advice: &[svg_data::compat_model::Discouraged<S, L>],
) -> Option<String> {
    if advice.is_empty() {
        return None;
    }
    Some(
        advice
            .iter()
            .map(|item| {
                let name = item
                    .feature_name
                    .as_ref()
                    .map_or_else(|| item.feature_id.as_ref(), AsRef::as_ref);
                let mut text = format!(
                    "**WebDX discourages {}**\n\n{}\n\nFeature scope: {} ({}).",
                    escape_metadata(name),
                    escape_metadata(item.reason.as_ref()),
                    escape_metadata(item.feature_id.as_ref()),
                    escape_metadata(item.compat_key.as_ref())
                );
                if !item.alternatives.as_ref().is_empty() {
                    let _ = write!(
                        text,
                        "\n\nAlternatives: {}",
                        item.alternatives
                            .as_ref()
                            .iter()
                            .map(|id| escape_metadata(id.as_ref()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                for reference in item.according_to.as_ref() {
                    let url = reference.as_ref();
                    let reference = if (url.starts_with("https://") || url.starts_with("http://"))
                        && !url
                            .chars()
                            .any(|c| c.is_whitespace() || matches!(c, '<' | '>'))
                    {
                        format!("<{url}>")
                    } else {
                        escape_metadata(url)
                    };
                    let _ = write!(text, "\n\nSupporting reference: {reference}");
                }
                if let Some(date) = &item.removal_date {
                    let _ = write!(
                        text,
                        "\n\nUpstream removal date: {}",
                        escape_metadata(date.as_ref())
                    );
                }
                text
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

/// Upstream prose is text, never trusted Markdown or HTML.
fn escape_metadata(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_ascii_punctuation() {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

/// Sub-bullet lines describing per-browser caveats the `format_browser_support_line`
/// chip row can't express: partial implementations, vendor prefixes, version
/// removals, alternative names, runtime flags, and upstream notes.
///
/// Returns `None` when no browser has any caveat to display — callers omit
/// the whole section in that case.
fn format_browser_notes_list(baked: Option<&BrowserSupport>) -> Option<Vec<String>> {
    let support = baked?;
    let mut lines = Vec::new();
    for (name, version) in [
        ("Chrome", support.chrome.as_ref()),
        ("Edge", support.edge.as_ref()),
        ("Firefox", support.firefox.as_ref()),
        ("Safari", support.safari.as_ref()),
    ] {
        let Some(v) = version else { continue };
        // Explicit-false is already covered by the chip row's `✗`.
        if matches!(v.supported, Some(false)) {
            continue;
        }
        let mut segments: Vec<String> = Vec::new();
        if v.partial_implementation {
            // First note, if any, carries the "why" for the partial impl.
            let detail = v.notes.first().map_or("", String::as_str);
            if detail.is_empty() {
                segments.push("partial implementation".to_string());
            } else {
                segments.push(format!("partial: {}", escape_metadata(detail)));
            }
        } else if !v.notes.is_empty() {
            segments.push(
                v.notes
                    .iter()
                    .map(|n| escape_metadata(n))
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
        }
        if let Some(prefix) = &v.prefix {
            segments.push(format!("requires `{prefix}` prefix"));
        }
        if let Some(alt) = &v.alternative_name {
            segments.push(format!("ships as `{alt}`"));
        }
        if !v.flags.is_empty() {
            let names: Vec<String> = v
                .flags
                .iter()
                .map(|f| {
                    let setting = f
                        .value_to_set
                        .as_ref()
                        .map_or_else(|| f.name.clone(), |value| format!("{}={value}", f.name));
                    let kind = f
                        .kind
                        .as_ref()
                        .map_or(String::new(), |kind| format!(" ({kind})"));
                    format!("`{setting}`{kind}")
                })
                .collect();
            segments.push(format!("behind flag {}", names.join(", ")));
        }
        if let Some(removed) = &v.version_removed {
            let glyph = format_baseline_qualifier(v.version_removed_qualifier);
            segments.push(format!("removed in {glyph}{removed}"));
        }
        if !segments.is_empty() {
            lines.push(format!("- {name}: {}", segments.join("; ")));
        }
    }
    if lines.is_empty() { None } else { Some(lines) }
}

/// Render the verdict headline as a markdown blockquote.
///
/// Glyph choice maps directly to [`VerdictRecommendation`]:
///
/// | Recommendation | Glyph | Semantics |
/// |---|---|---|
/// | `Safe`    | `✓` | Use it |
/// | `Caution` | `⚠` | Use with care |
/// | `Avoid`   | `⊘` | Avoid in new work |
/// | `Forbid`  | `✗` | Do not use |
///
/// The blockquote is rendered markdown — LSP clients that support it show
/// a left border + muted background, a clean attention-grabber. Clients
/// that strip quoting still get the glyph + feature name + template text
/// on the first line.
fn format_verdict_headline(verdict: &svg_data::CompatVerdict, feature_name: &str) -> String {
    let glyph = match verdict.recommendation {
        svg_data::VerdictRecommendation::Safe => "\u{2713}", // ✓
        svg_data::VerdictRecommendation::Caution => "\u{26A0}", // ⚠
        svg_data::VerdictRecommendation::Avoid => "\u{2298}", // ⊘
        svg_data::VerdictRecommendation::Forbid => "\u{2717}", // ✗
    };
    let template = if verdict.headline_template.is_empty() {
        match verdict.recommendation {
            svg_data::VerdictRecommendation::Safe => "safe to use",
            svg_data::VerdictRecommendation::Caution => "use with care",
            svg_data::VerdictRecommendation::Avoid => "avoid in new work",
            svg_data::VerdictRecommendation::Forbid => "do not use",
        }
    } else {
        verdict.headline_template
    };
    format!("> {glyph} `{feature_name}` — {template}")
}

/// Preserve independent profile restrictions while replacing all compatibility reasons.
fn reconcile_verdict(
    previous: Option<&svg_data::CompatVerdict>,
    facts: &Facts,
) -> Option<svg_data::CompatVerdict> {
    let current = effective_compat::verdict(facts);
    if let Some(profile) =
        previous.filter(|v| v.recommendation == svg_data::VerdictRecommendation::Forbid)
    {
        let mut result = profile.clone();
        result.reasons.retain(|r| {
            matches!(
                r,
                svg_data::VerdictReason::ProfileObsolete { .. }
                    | svg_data::VerdictReason::ProfileExperimental
            )
        });
        result
            .reasons
            .extend(current.into_iter().flat_map(|v| v.reasons));
        Some(result)
    } else {
        current
    }
}
fn reconciled_status(
    verdict: Option<&svg_data::CompatVerdict>,
    profile: Option<String>,
) -> Option<String> {
    let status = verdict.and_then(format_verdict_status);
    match (status, profile) {
        (Some(status), Some(profile)) if !profile.starts_with("**Stable") => {
            Some(format!("{status} · {profile}"))
        }
        (Some(status), _) => Some(status),
        (None, profile) => profile,
    }
}
fn append_provenance(builder: &mut CompatMarkdownBuilder, runtime: Option<&CompatOverride>) {
    let Some(runtime) = runtime else { return };
    for source in &runtime.sources {
        let state = match source.outcome {
            Outcome::Loaded => "loaded",
            Outcome::Absent => "no data",
            Outcome::Unknown => "unknown status",
            Outcome::Failed => "refresh failed; bundled facts retained (stale)",
            Outcome::Disabled => "refresh disabled; bundled facts",
        };
        builder.status(format!(
            "{} {}: {state}. Context: `{}`. Source: <{}>",
            source.source,
            source.version.as_deref().unwrap_or("(version unavailable)"),
            source.key,
            source.url
        ));
    }
}

/// Render the verdict status line — one or more reason tags joined by
/// ` · `. This consolidates the old split between `**Deprecated**` and
/// `**Stable in Svg2EditorsDraft**` into a single non-contradictory
/// phrase sourced from the pre-reconciled verdict.
fn format_verdict_status(verdict: &svg_data::CompatVerdict) -> Option<String> {
    if verdict.reasons.is_empty() {
        return None;
    }
    let parts: Vec<String> = verdict.reasons.iter().map(format_verdict_reason).collect();
    Some(format!("**Status:** {}", parts.join(" · ")))
}

fn format_verdict_reason(reason: &svg_data::VerdictReason) -> String {
    match reason {
        svg_data::VerdictReason::BcdDeprecated => "deprecated".to_string(),
        svg_data::VerdictReason::BcdExperimental => "experimental".to_string(),
        svg_data::VerdictReason::BcdNonStandard => "non-standard".to_string(),
        svg_data::VerdictReason::ProfileObsolete { last_seen } => {
            format!("removed after `{}`", last_seen.as_str())
        }
        svg_data::VerdictReason::ProfileExperimental => "draft-only in profile".to_string(),
        svg_data::VerdictReason::BaselineLimited => "limited baseline".to_string(),
        svg_data::VerdictReason::BaselineNewly { since, qualifier } => {
            let glyph = format_baseline_qualifier(*qualifier);
            since.map_or_else(
                || "newly available".to_owned(),
                |year| format!("newly available since {glyph}{year}"),
            )
        }
        svg_data::VerdictReason::PartialImplementationIn(browser) => {
            format!("partial in {browser}")
        }
        svg_data::VerdictReason::PrefixRequiredIn { browser, prefix } => {
            format!("`{prefix}` prefix in {browser}")
        }
        svg_data::VerdictReason::BehindFlagIn(browser) => {
            format!("flagged in {browser}")
        }
        svg_data::VerdictReason::UnsupportedIn(browser) => {
            format!("no support in {browser}")
        }
        svg_data::VerdictReason::RemovedIn {
            browser,
            version,
            qualifier,
        } => {
            let glyph = format_baseline_qualifier(*qualifier);
            format!("removed in {browser} {glyph}{version}")
        }
    }
}

fn format_browser_support_line(support: Option<&BrowserSupport>) -> Option<String> {
    let support = support?;
    let fmt = |name: &str, version: Option<&BrowserVersion>| {
        let Some(v) = version else {
            return format!("{name} unknown");
        };
        if v.supported == Some(false) {
            format!("{name} ✗")
        } else if let Some(version) = &v.version_added {
            format!(
                "{name} {}",
                format_version_with_qualifier(version, v.version_qualifier)
            )
        } else if v.supported == Some(true) {
            format!("{name} supported")
        } else {
            format!("{name} unknown")
        }
    };
    Some(
        [
            fmt("Chrome", support.chrome.as_ref()),
            fmt("Edge", support.edge.as_ref()),
            fmt("Firefox", support.firefox.as_ref()),
            fmt("Safari", support.safari.as_ref()),
        ]
        .join(" · "),
    )
}

fn format_version_with_qualifier(version: &str, qualifier: Option<BaselineQualifier>) -> String {
    let glyph = format_baseline_qualifier(qualifier);
    if glyph.is_empty() || version.starts_with(glyph) {
        version.to_owned()
    } else {
        format!("{glyph}{version}")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fixture_milestones_and_unknown_status_survive_runtime_hover()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../svg-data/src/fixtures/web-features.json"
        ))?;
        let element = svg_data::element("rect").ok_or("rect in catalog")?;
        for case in fixture["cases"].as_array().ok_or("fixture cases")? {
            let baseline = svg_data::compat_model::parse_baseline(&case["input"]);
            let Some(baseline) = baseline else {
                continue;
            };
            let baseline = serde_json::from_value(serde_json::to_value(baseline)?)?;
            let runtime = test_override(Facts {
                deprecated: false,
                experimental: false,
                standard_track: None,
                baseline: Some(baseline),
                discouraged: Vec::new(),
                browser_support: None,
            });
            let hover = super::format_element_hover_with_profile(
                element,
                svg_data::SpecSnapshotId::LATEST,
                None,
                Some(&runtime),
                None,
            );
            let baseline = runtime.facts.baseline.as_ref().ok_or("runtime baseline")?;
            if baseline.status.is_none() {
                assert!(
                    hover.contains("Baseline status unknown"),
                    "{}",
                    case["name"]
                );
                assert!(!hover.contains("![Baseline icon]"), "{}", case["name"]);
                assert!(!hover.contains("Limited availability"), "{}", case["name"]);
            }
            if case["name"] == "both-milestones" {
                assert!(hover.contains("Widely Available since 2022"));
                assert!(hover.contains("Newly Available date: 2020-01-15"));
                assert!(hover.contains("Widely Available date: 2022-07-15"));
                assert!(!hover.contains("Widely Available since 2020"));
            }
            if case["name"] == "newly-undated" {
                assert!(hover.contains("_Newly Available_"));
            }
            if case["name"] == "malformed-dates" {
                assert!(hover.contains("_Widely Available_"));
                assert!(hover.contains("date not recognized"));
            }
        }
        Ok(())
    }

    #[test]
    fn runtime_discouragement_surfaces_scope_references_and_alternatives()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../svg-data/src/fixtures/web-features.json"
        ))?;
        let features = Some(&fixture["features"]);
        let runtime = test_override(Facts {
            deprecated: false,
            experimental: false,
            standard_track: None,
            baseline: svg_data::compat_model::resolve_baseline(features, "svg.elements.legacy"),
            discouraged: svg_data::compat_model::resolve_discouraged(
                features,
                "svg.elements.legacy",
            ),
            browser_support: None,
        });
        let element = svg_data::element("rect").ok_or("rect in catalog")?;
        let hover = super::format_element_hover_with_profile(
            element,
            svg_data::SpecSnapshotId::LATEST,
            None,
            Some(&runtime),
            None,
        );
        assert!(hover.contains("WebDX discourages Legacy SVG"));
        assert!(hover.contains("Use a modern SVG feature"));
        assert!(hover.contains("Feature scope:"));
        assert!(hover.contains("https://example.com/retirement"));
        assert!(hover.contains("Alternatives: svg"));
        assert!(!hover.contains("![Baseline icon]"));
        assert!(!hover.contains("deprecated"));
        Ok(())
    }
    use svg_data::ProfileLookup;

    use super::*;

    fn bv_unknown() -> BrowserVersion {
        BrowserVersion {
            supported: Some(true),
            ..BrowserVersion::default()
        }
    }

    #[test]
    fn unknown_browser_version_is_shown_as_supported() {
        let baked = BrowserSupport {
            chrome: Some(bv_unknown()),
            edge: None,
            firefox: None,
            safari: None,
        };

        // New separator is ` · ` (prose bullet) instead of ` | `.
        assert_eq!(
            format_browser_support_line(Some(&baked)),
            Some("Chrome supported · Edge unknown · Firefox unknown · Safari unknown".to_owned())
        );
    }

    #[test]
    fn absent_browser_support_omits_chip_row() {
        assert_eq!(format_browser_support_line(None), None);
    }

    #[test]
    fn semantic_value_shapes_render_without_fake_grammar() {
        let cases = [
            ("id", "Value: non-empty ID without whitespace"),
            ("class", "Value: space-separated tokens"),
            ("lang", "Value: BCP 47 language tag"),
            ("tabindex", "Value: integer"),
            ("style", "Value: CSS declaration list"),
            ("referrerpolicy", "Value: referrer policy"),
        ];
        for (name, label) in cases {
            let Some(attribute) = svg_data::attribute(name) else {
                panic!("missing {name} attribute");
            };
            let hover = format_attribute_hover_with_profile_name(
                attribute,
                name,
                None,
                SpecSnapshotId::LATEST,
                None,
                None,
                None,
            );

            assert!(hover.contains(label), "{name}: {hover}");
            assert!(!hover.contains("Grammar:"), "{name}: {hover}");
            assert!(!hover.contains("Keywords:"), "{name}: {hover}");
        }
    }

    #[test]
    fn qualified_browser_versions_are_not_double_prefixed() {
        assert_eq!(
            format_version_with_qualifier("≤80", Some(BaselineQualifier::Before)),
            "≤80"
        );
        assert_eq!(
            format_version_with_qualifier("80", Some(BaselineQualifier::Before)),
            "≤80"
        );
    }

    #[test]
    fn missing_and_unknown_browser_data_stays_unknown() {
        let support = BrowserSupport {
            chrome: Some(BrowserVersion::default()),
            ..BrowserSupport::default()
        };
        assert_eq!(
            format_browser_support_line(Some(&support)).as_deref(),
            Some("Chrome unknown · Edge unknown · Firefox unknown · Safari unknown")
        );
    }

    #[test]
    fn unsupported_profile_hover_line_marks_obsolete_after_last_known_snapshot() {
        assert_eq!(
            profile_lifecycle_hover_line(
                SpecSnapshotId::Svg2EditorsDraft,
                &ProfileLookup::<()>::UnsupportedInProfile {
                    known_in: &[
                        SpecSnapshotId::Svg11Rec20030114,
                        SpecSnapshotId::Svg11Rec20110816,
                    ],
                },
            ),
            Some("**Obsolete after Svg11Rec20110816**".to_owned())
        );
    }

    #[test]
    fn present_profile_hover_line_uses_selected_profile_lifecycle() {
        assert_eq!(
            profile_lifecycle_hover_line(
                SpecSnapshotId::Svg2EditorsDraft,
                &ProfileLookup::Present {
                    value: &(),
                    lifecycle: SpecLifecycle::Experimental,
                },
            ),
            Some("**Experimental in Svg2EditorsDraft**".to_owned())
        );
    }

    fn test_override(facts: Facts) -> CompatOverride {
        CompatOverride {
            facts,
            sources: std::array::from_fn(|_| crate::compat::Provenance {
                source: "fixture",
                version: Some("1".to_owned()),
                url: "https://example.com/data".to_owned(),
                key: "fixture".to_owned(),
                outcome: Outcome::Loaded,
            }),
        }
    }
    #[test]
    fn successful_refresh_clears_old_compat_reasons_and_keeps_profile_reasons() {
        let previous = svg_data::CompatVerdict {
            recommendation: svg_data::VerdictRecommendation::Forbid,
            headline_template: "removed from the current SVG profile",
            reasons: vec![
                svg_data::VerdictReason::BaselineLimited,
                svg_data::VerdictReason::BcdDeprecated,
                svg_data::VerdictReason::ProfileObsolete {
                    last_seen: SpecSnapshotId::Svg11Rec20110816,
                },
            ],
        };
        let verdict = reconcile_verdict(Some(&previous), &Facts::default());
        assert_eq!(
            verdict.map(|v| v.reasons),
            Some(vec![svg_data::VerdictReason::ProfileObsolete {
                last_seen: SpecSnapshotId::Svg11Rec20110816
            }])
        );
    }
}
