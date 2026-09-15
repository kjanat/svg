use std::{fmt::Write as _, sync::LazyLock};

use svg_data::browser_compat::{BrowserSupport, BrowserVersion};
use svg_data::effective_compat::{self, Facts};
use svg_data::{BaselineQualifier, BaselineTier, ProfileLookup, SpecLifecycle, SpecSnapshotId};
use tower_lsp_server::ls_types::Uri;
use url::Url;

use crate::{
    clipboard::svg_data_uri,
    compat::{CompatOverride, Outcome},
    hover_settings::{BrowserDetail, HoverSettings, Section, browser_label},
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

/// Length of the longest run of backticks in `text`.
fn longest_backtick_run(text: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for byte in text.bytes() {
        if byte == b'`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// Make a link label safe to write between brackets.
///
/// A path may contain either bracket, and one of them ends the label early —
/// leaving the whole construct visible in a Markdown client as much as here.
fn escape_link_label(label: &str) -> String {
    if label.contains(['[', ']', '\\']) {
        let mut escaped = String::with_capacity(label.len() + 2);
        for character in label.chars() {
            if matches!(character, '[' | ']' | '\\') {
                escaped.push('\\');
            }
            escaped.push(character);
        }
        escaped
    } else {
        label.to_owned()
    }
}

/// Make a link destination safe to write between parentheses.
///
/// Parentheses are legal in a path and in a URI, and an unpaired one closes
/// the destination early — for a Markdown client as much as a plain-text one,
/// which is why this belongs here rather than in the conversion.
fn escape_link_target(target: &str) -> String {
    if target.contains(['(', ')']) {
        target.replace('(', "%28").replace(')', "%29")
    } else {
        target.to_owned()
    }
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
                // The snippet is CSS the user wrote, and a comment in it may
                // contain a run of backticks. A fence has to be longer than
                // anything it wraps or the preview ends early, in any client.
                let fence = "`".repeat(longest_backtick_run(trimmed).max(2) + 1);
                section.push_str(&fence);
                section.push_str("css\n");
                section.push_str(trimmed);
                section.push('\n');
                section.push_str(&fence);
            }
            section.push_str("\nDefined in [");
            section.push_str(&escape_link_label(&source.label));
            section.push_str("](");
            section.push_str(&escape_link_target(&source.target));
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
    /// Consolidated svg assessment line, or profile-lifecycle fallback.
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
    settings: &HoverSettings,
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
    let verdict =
        effective_compat::verdict_for_browsers(facts, settings.browsers.iter().map(String::as_str));

    let mut builder = CompatMarkdownBuilder::new();

    if settings.shows(Section::Status)
        && let Some(v) = verdict.as_ref()
    {
        builder.headline(format_verdict_headline(v, el.name));
    } else {
        builder.headline(format!("`<{}>`", el.name));
    }
    if settings.shows(Section::Description) {
        builder.description(el.description.to_owned());
    }

    if settings.shows(Section::Status)
        && native
            .is_some_and(|n| n.is_unsupported(svg_data::profile::ConstraintKind::Element, el.name))
    {
        builder.status("⚠ Not supported by the SVG Native profile".to_owned());
    }

    if settings.shows(Section::Status)
        && let Some(status) = reconciled_status(verdict.as_ref(), profile_lifecycle)
    {
        builder.status(status);
    }

    append_compat_details(&mut builder, facts, rt, settings);
    append_spec_declaration(
        &mut builder,
        svg_data::element_lifecycle_for_profile(profile, el.name),
        settings,
    );
    if settings.shows(Section::Links) {
        builder.links(hover_link_list(el.mdn_url, el.spec_url));
    }

    builder.build()
}

/// Profile, source context and presentation preferences for one attribute.
pub struct AttributeHoverContext<'a> {
    pub element_name: Option<&'a str>,
    pub profile: SpecSnapshotId,
    pub profile_lifecycle: Option<String>,
    pub rt: Option<&'a CompatOverride>,
    pub native: Option<&'a svg_data::profile::SvgNative>,
    pub settings: &'a HoverSettings,
}

pub fn format_attribute_hover_with_profile_name(
    attr: &svg_data::AttributeDef,
    display_name: &str,
    context: AttributeHoverContext<'_>,
) -> String {
    let verdict = svg_data::compat_verdict_for_attribute_on_element(
        attr,
        context.element_name,
        context.profile,
    );
    format_attribute_hover_with_verdict(attr, display_name, context, verdict.as_ref())
}

pub struct UnsupportedAttributeHoverProfile<'a> {
    pub profile: SpecSnapshotId,
    pub known_in: &'static [SpecSnapshotId],
    pub profile_lifecycle: Option<String>,
    pub rt: Option<&'a CompatOverride>,
    pub native: Option<&'a svg_data::profile::SvgNative>,
    pub settings: &'a HoverSettings,
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
        AttributeHoverContext {
            element_name,
            profile: profile.profile,
            profile_lifecycle: profile.profile_lifecycle,
            rt: profile.rt,
            native: profile.native,
            settings: profile.settings,
        },
        Some(&verdict),
    )
}

fn format_attribute_hover_with_verdict(
    attr: &svg_data::AttributeDef,
    display_name: &str,
    context: AttributeHoverContext<'_>,
    verdict: Option<&svg_data::CompatVerdict>,
) -> String {
    let AttributeHoverContext {
        element_name,
        profile,
        profile_lifecycle,
        rt,
        native,
        settings,
    } = context;
    let baked = Facts::from(attr.compat_facts_for_element(element_name));
    let facts = rt.map_or(&baked, |r| &r.facts);
    let effective_verdict = reconcile_verdict(verdict, facts, settings);
    let verdict = effective_verdict.as_ref();

    let mut builder = CompatMarkdownBuilder::new();

    if settings.shows(Section::Status)
        && let Some(v) = verdict
    {
        builder.headline(format_verdict_headline(v, display_name));
    } else {
        builder.headline(format!("`{display_name}`"));
    }
    if settings.shows(Section::Description) {
        builder.description(attr.description.to_owned());
    }

    if settings.shows(Section::Status)
        && native.is_some_and(|n| {
            n.is_unsupported(svg_data::profile::ConstraintKind::Attribute, attr.name)
                || n.is_unsupported(svg_data::profile::ConstraintKind::Property, attr.name)
        })
    {
        builder.status("⚠ Not supported by the SVG Native profile".to_owned());
    }

    if settings.shows(Section::Status)
        && let Some(status) = reconciled_status(verdict, profile_lifecycle)
    {
        builder.status(status);
    }

    if settings.shows(Section::Values) {
        builder.value_constraints(value_constraints_lines(attr.values_for_profile(profile)));
    }

    append_compat_details(&mut builder, facts, rt, settings);
    append_spec_declaration(
        &mut builder,
        svg_data::attribute_lifecycle_on_element(profile, display_name, element_name),
        settings,
    );
    if settings.shows(Section::Links) {
        builder.links(hover_link_list(attr.mdn_url, attr.spec_url));
    }

    builder.build()
}

fn append_spec_declaration(
    builder: &mut CompatMarkdownBuilder,
    lifecycle: Option<&svg_data::FeatureLifecycle>,
    settings: &HoverSettings,
) {
    if !settings.shows(Section::Status) {
        return;
    }
    let Some(declaration) = lifecycle.and_then(|l| l.declaration) else {
        return;
    };
    let status = match declaration.status {
        svg_data::DeclaredStatus::Deprecated => "Deprecated",
        svg_data::DeclaredStatus::Obsolete => "Obsoleted (retained for legacy content)",
        svg_data::DeclaredStatus::Removed => "Removed",
    };
    builder.status(format!(
        "**SVG specification:** {status}. [Source]({})",
        declaration.source
    ));
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

static BASELINE_HIGH: LazyLock<String> = LazyLock::new(|| {
    baseline_icon(
        include_str!("../assets/baseline-widely-icon.svg"),
        include_str!("../assets/baseline-widely-icon-dark.svg"),
    )
});
static BASELINE_LOW: LazyLock<String> = LazyLock::new(|| {
    baseline_icon(
        include_str!("../assets/baseline-newly-icon.svg"),
        include_str!("../assets/baseline-newly-icon-dark.svg"),
    )
});
static BASELINE_LIMITED: LazyLock<String> = LazyLock::new(|| {
    baseline_icon(
        include_str!("../assets/baseline-limited-icon.svg"),
        include_str!("../assets/baseline-limited-icon-dark.svg"),
    )
});

/// Scale and select immutable official artwork in a separate image container.
/// Markdown has no portable image sizing or theme-selection syntax.
fn baseline_icon(light: &str, dark: &str) -> String {
    svg_data_uri(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="10" viewBox="0 0 18 10">
<style>.dark{{display:none}}@media(prefers-color-scheme:dark){{.light{{display:none}}.dark{{display:inline}}}}</style>
<image class="light" width="18" height="10" href="{}"/>
<image class="dark" width="18" height="10" href="{}"/>
</svg>"#,
        svg_data_uri(light),
        svg_data_uri(dark),
    ))
}

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

fn format_baseline<T>(baseline: &svg_data::compat_model::Baseline<&str, T>) -> String {
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

fn append_compat_details(
    builder: &mut CompatMarkdownBuilder,
    facts: &Facts,
    rt: Option<&CompatOverride>,
    settings: &HoverSettings,
) {
    let advice = settings
        .shows(Section::Discouraged)
        .then(|| format_discouraged(&facts.discouraged))
        .flatten();
    if let Some(advice) = advice {
        builder.baseline(advice);
    } else if settings.shows(Section::Baseline)
        && let Some(baseline) = &facts.baseline
    {
        builder.baseline(format_baseline(&baseline.as_ref()));
    }
    if settings.shows(Section::Browsers)
        && let Some(line) = format_browser_support_line(facts.browser_support.as_ref(), settings)
    {
        builder.browser_chips(line);
    }
    if settings.shows(Section::BrowserDetails)
        && let Some(lines) = format_browser_notes_list(facts.browser_support.as_ref(), settings)
    {
        builder.browser_notes(lines);
    }
    if settings.shows(Section::WebFeaturesSupport)
        && let Some(support) = facts.baseline.as_ref().and_then(|b| b.support.as_ref())
    {
        let lines = settings
            .browsers
            .iter()
            .filter_map(|id| {
                support.get(id).map(|version| {
                    format!(
                        "{} {}",
                        escape_metadata(browser_label(id)),
                        escape_metadata(version)
                    )
                })
            })
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            builder.baseline(format!("Web Features support: {}", lines.join(" · ")));
        }
    }
    if settings.shows(Section::Sources) {
        append_provenance(builder, rt);
    }
}

fn metadata_code(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    format!("{fence} {text} {fence}")
}

fn metadata_link(text: &str) -> String {
    if let Ok(url) = Url::parse(text)
        && matches!(url.scheme(), "http" | "https")
    {
        let target = url.as_str().replace('<', "%3C").replace('>', "%3E");
        format!("<{target}>")
    } else {
        escape_metadata(text)
    }
}

/// Upstream HTML remains intact in storage; presentation extracts readable text and escapes Markdown.
fn format_browser_note(note: &str) -> String {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(note);
    reader.config_mut().check_end_names = false;
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(value)) => match value.decode() {
                Ok(value) => text.push_str(&value),
                Err(_) => return escape_metadata(note),
            },
            Ok(Event::GeneralRef(reference)) => {
                if let Ok(Some(c)) = reference.resolve_char_ref() {
                    text.push(c);
                } else if let Ok(name) = reference.decode() {
                    match name.as_ref() {
                        "amp" => text.push('&'),
                        "lt" => text.push('<'),
                        "gt" => text.push('>'),
                        "quot" => text.push('"'),
                        "apos" => text.push('\''),
                        "nbsp" => text.push(' '),
                        name => {
                            let _ = write!(text, "&{name};");
                        }
                    }
                }
            }
            Ok(Event::Start(tag) | Event::Empty(tag))
                if matches!(tag.name().as_ref(), b"br" | b"p" | b"li") =>
            {
                text.push(' ');
            }
            Ok(Event::Eof) => break,
            Err(_) => return escape_metadata(note),
            _ => {}
        }
    }
    escape_metadata(&text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn format_browser_notes_list(
    support: Option<&BrowserSupport>,
    settings: &HoverSettings,
) -> Option<Vec<String>> {
    let support = support?;
    let mut lines = Vec::new();
    let shows = |detail| settings.browser_details.contains(&detail);
    for id in &settings.browsers {
        let Some(history) = support.get(id) else {
            continue;
        };
        let versions = if settings.browser_history {
            history.iter().collect::<Vec<_>>()
        } else {
            svg_data::browser_compat::select_statement(history)
                .into_iter()
                .collect()
        };
        for v in versions {
            let mut segments = Vec::new();
            if settings.browser_history {
                segments.push(browser_version_label(Some(v)));
            }
            if shows(BrowserDetail::PartialImplementation) && v.partial_implementation {
                segments.push("partial implementation".to_owned());
            }
            if shows(BrowserDetail::Prefix)
                && let Some(prefix) = &v.prefix
            {
                segments.push(format!("requires {} prefix", escape_metadata(prefix)));
            }
            if shows(BrowserDetail::AlternativeName)
                && let Some(alt) = &v.alternative_name
            {
                segments.push(format!("ships as {}", escape_metadata(alt)));
            }
            for flag in v.flags.iter().filter(|_| shows(BrowserDetail::Flags)) {
                let setting = flag.value_to_set.as_ref().map_or_else(
                    || flag.name.clone(),
                    |value| format!("{}={value}", flag.name),
                );
                segments.push(format!(
                    "behind flag {} ({})",
                    metadata_code(&setting),
                    flag.kind.as_str()
                ));
            }
            if shows(BrowserDetail::VersionRemoved)
                && let Some(removed) = &v.version_removed
            {
                segments.push(format!("removed in {}", escape_metadata(removed)));
            }
            if shows(BrowserDetail::VersionLast)
                && let Some(last) = &v.version_last
            {
                segments.push(format!("last supported in {}", escape_metadata(last)));
            }
            segments.extend(
                v.notes
                    .iter()
                    .filter(|_| shows(BrowserDetail::Notes))
                    .map(|n| format_browser_note(n)),
            );
            segments.extend(
                v.impl_url
                    .iter()
                    .filter(|_| shows(BrowserDetail::ImplementationLinks))
                    .map(|url| format!("implementation: {}", metadata_link(url))),
            );
            if !segments.is_empty() {
                lines.push(format!(
                    "- {}: {}",
                    escape_metadata(browser_label(id)),
                    segments.join("; ")
                ));
            }
        }
    }
    (!lines.is_empty()).then_some(lines)
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
    settings: &HoverSettings,
) -> Option<svg_data::CompatVerdict> {
    let current =
        effective_compat::verdict_for_browsers(facts, settings.browsers.iter().map(String::as_str));
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
    Some(format!(
        "**Status (svg assessment):** {}",
        parts.join(" · ")
    ))
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
        svg_data::VerdictReason::RemovedIn { browser, version } => {
            format!("removed in {browser} {}", format_browser_version(version))
        }
    }
}

fn format_browser_version(version: &str) -> String {
    if version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '≤' | '≥' | '~' | '≈'))
    {
        version.to_owned()
    } else {
        escape_metadata(version)
    }
}

fn browser_version_label(version: Option<&BrowserVersion>) -> String {
    let Some(v) = version else {
        return "unknown".to_owned();
    };
    if v.supported() == Some(false) {
        "✗".to_owned()
    } else if let Some(version) = v.version() {
        let removed = if v.version_removed.is_some() {
            " (removed)"
        } else {
            ""
        };
        format!("{}{removed}", format_browser_version(version))
    } else {
        "unknown".to_owned()
    }
}

fn format_browser_support_line(
    support: Option<&BrowserSupport>,
    settings: &HoverSettings,
) -> Option<String> {
    let support = support?;
    let parts = settings
        .browsers
        .iter()
        .map(|id| {
            let v = support
                .get(id)
                .and_then(|v| svg_data::browser_compat::select_statement(v));
            format!(
                "{} {}",
                escape_metadata(browser_label(id)),
                browser_version_label(v)
            )
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Render hover Markdown as readable plain text.
///
/// A client that does not advertise `markdown` in `textDocument.hover.
/// contentFormat` is only promised plain text, and handing it Markdown leaves
/// the syntax on screen. This is not a Markdown renderer; it handles the
/// constructs these hovers actually emit, which were read off generated hover
/// output rather than guessed at.
///
/// Two rules keep it from rewriting the thing the hover is about. A code span
/// or fenced block carries its content through literally, as Markdown itself
/// does, so a CSS rule the user wrote is never reinterpreted as markup. And a
/// delimiter is only markup when it is actually paired: `--_accent` keeps its
/// underscore because nothing closes it, while `_Widely Available_` loses both
/// of its.
pub fn to_plain_text(markdown: &str) -> String {
    let bytes = markdown.as_bytes();
    let mut out = String::with_capacity(markdown.len());
    let mut at = 0;
    // Each of these records that a delimiter can no longer close anywhere
    // ahead. Without them the scan restarts at every candidate, which is
    // quadratic in a CSS definition whose size and shape the user controls.
    let mut brackets_can_close = true;
    let mut autolinks_can_close = true;
    let mut code_can_close = true;
    let mut emphasis_dead_until = 0;
    let mut emphasis_close = None;
    while at < bytes.len() {
        match bytes[at] {
            b'<' if autolinks_can_close => {
                // `metadata_link` writes a URL as an autolink, which is the
                // right thing and which this has to undo rather than read
                // through: a destination is literal, so `…/_draft_` keeps its
                // underscores instead of losing them to emphasis.
                match autolink_end(bytes, at) {
                    Some(close) => {
                        out.push_str(&markdown[at + 1..close]);
                        at = close + 1;
                        continue;
                    }
                    None if !bytes[at..].contains(&b'>') => autolinks_can_close = false,
                    None => {}
                }
            }
            b'>' if at_line_start(bytes, at) => {
                // Every verdict headline is generated as a block quote, so a
                // plain-text client would read the marker as part of the
                // sentence rather than as the shape it is.
                at += 1;
                if bytes.get(at) == Some(&b' ') {
                    at += 1;
                }
                continue;
            }
            b'-' if at_line_start(bytes, at) => {
                // `format_definition_hover` separates multiple definitions with
                // a thematic break. The blank lines around it already do that
                // job in plain text; the rule itself is only markup.
                if let Some(after) = thematic_break_end(bytes, at) {
                    at = after;
                    continue;
                }
            }
            b'\\' if bytes.get(at + 1).is_some_and(u8::is_ascii_punctuation) => {
                // Metadata is escaped for Markdown before it ever gets here,
                // so the backslash is markup and the character after it is not.
                out.push(char::from(bytes[at + 1]));
                at += 2;
                continue;
            }
            // A fence is a block: three or more backticks starting a line.
            // The same run in the middle of one opens a code span instead,
            // which is how a flag value containing backticks is written.
            b'`' if opens_fence(bytes, at) => {
                at = copy_fenced_block(markdown, at, &mut out);
                continue;
            }
            b'`' if code_can_close => {
                let width = backtick_run(bytes, at);
                if let Some(close) = closing_run(bytes, at + width, width) {
                    // A code span has no inline markup inside it, and carries
                    // one space in from each end when it has both.
                    out.push_str(trim_code_span(&markdown[at + width..close]));
                    at = close + width;
                    continue;
                }
                code_can_close = false;
            }
            b'~' if bytes[at..].starts_with(b"~~") => {
                at += 2;
                continue;
            }
            b'*' if bytes[at..].starts_with(b"**") => {
                at += 2;
                continue;
            }
            b'_' => {
                if emphasis_close == Some(at) {
                    emphasis_close = None;
                    at += 1;
                    continue;
                }
                // Only a real opener is worth scanning for a partner, and
                // only a scan that ran and found nothing says anything about
                // the openers after it — one rejected for sitting inside a
                // word tells us nothing at all.
                if emphasis_close.is_none()
                    && opens_emphasis(bytes, at)
                    && at >= emphasis_dead_until
                {
                    if let Some(close) = emphasis_span(bytes, at) {
                        emphasis_close = Some(close);
                        at += 1;
                        continue;
                    }
                    // The candidates ahead of any later opener are a subset of
                    // the ones just rejected, so stop looking until the next
                    // line begins.
                    emphasis_dead_until = line_end(bytes, at);
                }
            }
            b'!' | b'[' if brackets_can_close => match copy_link(markdown, at, &mut out) {
                LinkScan::Found { after, .. } => {
                    at = after;
                    continue;
                }
                LinkScan::Unclosed => brackets_can_close = false,
                LinkScan::NotALink => {}
            },
            _ => {}
        }
        // `at` only ever lands on a character boundary: every branch above
        // matches ASCII, and the fallback advances by one whole character.
        let next = markdown[at..].chars().next().unwrap_or('\u{fffd}');
        out.push(next);
        at += next.len_utf8();
    }
    out
}

/// Copy a fenced block's content verbatim, dropping the fences and the
/// language tag that rides on the opening one. Returns where to resume.
fn copy_fenced_block(source: &str, open: usize, out: &mut String) -> usize {
    let bytes = source.as_bytes();
    let width = backtick_run(bytes, open);
    // The opening fence owns the rest of its line: `css` is a tag, not content.
    let mut at = line_end(bytes, open + width);
    at += usize::from(at < bytes.len());
    let mut line = at;
    while line < bytes.len() {
        // Only a line that is nothing but backticks, at least as many as
        // opened the block, closes it. A CSS comment carrying a run of them
        // is content — which is the whole promise of a verbatim preview.
        let run = backtick_run(bytes, line);
        let ends = line_end(bytes, line);
        if run >= width && source[line + run..ends].trim().is_empty() {
            // The content already ends with the newline before the fence, so
            // the one after the fence would be a second blank line.
            out.push_str(&source[at..line]);
            return ends + usize::from(ends < bytes.len());
        }
        line = ends + 1;
    }
    // Unterminated: the rest of the text is content.
    out.push_str(&source[at..]);
    bytes.len()
}

/// Whether the backticks at `at` open a fenced block rather than a code span.
///
/// A fence begins a line with three or more, and `CommonMark` forbids its info
/// string from carrying backticks — which is exactly what separates it from a
/// long inline delimiter. `metadata_code` reaches for one of those whenever a
/// browser flag value contains a backtick of its own, and on a line that
/// starts with it the two are otherwise indistinguishable.
fn opens_fence(bytes: &[u8], at: usize) -> bool {
    let width = backtick_run(bytes, at);
    at_line_start(bytes, at)
        && width >= 3
        && !bytes[at + width..line_end(bytes, at)].contains(&b'`')
}

/// Where the next run of exactly `width` backticks begins at or after `from`.
///
/// A code span closes on a run of its own length and no other, so a longer run
/// inside it is content — which is the point of `metadata_code` choosing a
/// delimiter longer than anything it wraps.
fn closing_run(bytes: &[u8], from: usize, width: usize) -> Option<usize> {
    let mut at = from;
    while at < bytes.len() {
        if bytes[at] == b'`' {
            let run = backtick_run(bytes, at);
            if run == width {
                return Some(at);
            }
            at += run;
        } else {
            at += 1;
        }
    }
    None
}

/// Drop the one space a code span carries in from each end, as `CommonMark`
/// does, so `` ` a ` `` reads as `a` rather than as padded text.
fn trim_code_span(content: &str) -> &str {
    content
        .strip_prefix(' ')
        .and_then(|rest| rest.strip_suffix(' '))
        .filter(|inner| !inner.trim().is_empty())
        .unwrap_or(content)
}

/// How many backticks run from `at`.
fn backtick_run(bytes: &[u8], at: usize) -> usize {
    bytes[at..].iter().take_while(|&&byte| byte == b'`').count()
}

/// Index of the newline ending the line containing `from`, or the end.
fn line_end(bytes: &[u8], from: usize) -> usize {
    (from..bytes.len())
        .find(|&at| bytes[at] == b'\n')
        .unwrap_or(bytes.len())
}

/// Where the emphasis opened by the `_` at `open` closes, if it closes at all
/// on this line.
///
/// `CommonMark` asks that an opener be followed by something other than space
/// and a closer preceded by the same, which is what keeps `--_accent` and
/// `._icon` intact: nothing there closes what they appear to open.
fn emphasis_span(bytes: &[u8], open: usize) -> Option<usize> {
    let solid = |byte: Option<&u8>| byte.is_some_and(|byte| !byte.is_ascii_whitespace());
    // Walk to the closer or the end of the line, whichever comes first.
    // Computing the line end up front would scan the whole line for every
    // underscore on it, even one whose partner is the very next byte.
    (open + 2..bytes.len())
        .take_while(|&at| bytes[at] != b'\n')
        // An escaped underscore is a character, so it cannot close emphasis
        // any more than it can open it.
        .find(|&at| {
            bytes[at] == b'_'
                && bytes[at - 1] != b'\\'
                && solid(Some(&bytes[at - 1]))
                && !intraword(bytes, at)
        })
}

/// Where the autolink opened at `at` closes, if it is one.
///
/// `CommonMark` asks for a scheme, then anything but a space or another angle
/// bracket, then `>`. Anything else beginning with `<` is just a character —
/// `a < b` is arithmetic, not markup.
fn autolink_end(bytes: &[u8], open: usize) -> Option<usize> {
    let scheme_start = open + 1;
    if !bytes.get(scheme_start).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let mut at = scheme_start + 1;
    while bytes
        .get(at)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    {
        at += 1;
    }
    if bytes.get(at) != Some(&b':') || at == scheme_start + 1 {
        return None;
    }
    while at < bytes.len() {
        match bytes[at] {
            b'>' => return Some(at),
            byte if byte.is_ascii_whitespace() || byte == b'<' => return None,
            _ => at += 1,
        }
    }
    None
}

/// Whether the `_` at `at` can open emphasis at all: it must be followed by
/// something other than space, and must not sit inside a word.
fn opens_emphasis(bytes: &[u8], at: usize) -> bool {
    bytes
        .get(at + 1)
        .is_some_and(|byte| !byte.is_ascii_whitespace())
        && !intraword(bytes, at)
}

/// Whether the delimiter at `at` sits inside a word, where `CommonMark` says
/// an underscore is neither an opener nor a closer.
///
/// This is what keeps prose intact: the catalog describes `media_query_list`,
/// and treating its underscores as emphasis leaves a plain-text reader with
/// `mediaquerylist`.
fn intraword(bytes: &[u8], at: usize) -> bool {
    let word = |byte: Option<&u8>| byte.is_some_and(u8::is_ascii_alphanumeric);
    word(at.checked_sub(1).map(|before| &bytes[before])) && word(bytes.get(at + 1))
}

/// Drop backslashes that escape ASCII punctuation.
fn unescape_punctuation(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('\\') {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\\'
            && let Some(escaped) = characters.clone().next()
            && escaped.is_ascii_punctuation()
        {
            out.push(escaped);
            characters.next();
        } else {
            out.push(character);
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Whether `at` begins a line.
/// Where a thematic break starting at `at` ends, if that whole line is one.
///
/// Only a run of `-` counts. `*` and `_` runs would collide with the emphasis
/// delimiters, and the one thematic break these hovers write is the `---` that
/// joins definitions. A dash in prose cannot reach here: `escape_metadata`
/// backslashes it, and CSS rides through its fence literally.
fn thematic_break_end(bytes: &[u8], at: usize) -> Option<usize> {
    let end = line_end(bytes, at);
    let mut dashes = 0usize;
    for &byte in &bytes[at..end] {
        match byte {
            b'-' => dashes += 1,
            b' ' | b'\t' => {}
            _ => return None,
        }
    }
    if dashes < 3 {
        return None;
    }
    // The break came with a blank line on each side. One of them is already
    // written, and the other would leave a gap where the rule used to be.
    let mut after = end + usize::from(end < bytes.len());
    after += usize::from(bytes.get(after) == Some(&b'\n'));
    Some(after)
}

const fn at_line_start(bytes: &[u8], at: usize) -> bool {
    at == 0 || bytes[at - 1] == b'\n'
}

/// What a scan for `[text](target)` found.
enum LinkScan<'a> {
    Found {
        text: &'a str,
        target: &'a str,
        after: usize,
    },
    /// A `]` closed the text but no `(` followed, so this is not a link.
    NotALink,
    /// No `]` appears anywhere after the `[`, so nothing later can be one.
    Unclosed,
}

/// Copy the link or image beginning at `at`, if there is one, and report how
/// the scan went so the caller can stop looking when nothing ahead can close.
///
/// An image becomes the alt text that describes it — the Baseline badges carry
/// a two-kilobyte data URI — while a link keeps its text and gains its target.
fn copy_link<'a>(source: &'a str, at: usize, out: &mut String) -> LinkScan<'a> {
    let bytes = source.as_bytes();
    let image = bytes[at] == b'!';
    if image && bytes.get(at + 1) != Some(&b'[') {
        return LinkScan::NotALink;
    }
    let scan = read_link(source, at + usize::from(image));
    if let LinkScan::Found { text, target, .. } = scan {
        // The label is escaped where it is written, so the escapes come back
        // off here rather than reaching a reader who never saw the brackets.
        out.push_str(&unescape_punctuation(text));
        if !image && !target.is_empty() {
            out.push_str(" (");
            out.push_str(target);
            out.push(')');
        }
    }
    scan
}

/// Split `[text](target)` starting at the `[`. Nested parentheses in the
/// target are counted, since a `data:` URI can carry them.
fn read_link(source: &str, open: usize) -> LinkScan<'_> {
    let bytes = source.as_bytes();
    let Some(text_end) =
        (open + 1..bytes.len()).find(|&at| bytes[at] == b']' && bytes[at - 1] != b'\\')
    else {
        return LinkScan::Unclosed;
    };
    if bytes.get(text_end + 1) != Some(&b'(') {
        return LinkScan::NotALink;
    }
    let mut depth = 1usize;
    let mut at = text_end + 2;
    while at < bytes.len() {
        match bytes[at] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return LinkScan::Found {
                        text: &source[open + 1..text_end],
                        target: &source[text_end + 2..at],
                        after: at + 1,
                    };
                }
            }
            _ => {}
        }
        at += 1;
    }
    LinkScan::NotALink
}

#[cfg(test)]
mod tests {

    #[test]
    fn plain_text_drops_markdown_syntax_but_keeps_the_words() {
        assert_eq!(
            super::to_plain_text("**Baseline** `stroke-width` is fine"),
            "Baseline stroke-width is fine"
        );
    }

    #[test]
    fn plain_text_keeps_a_link_and_where_it_goes() {
        assert_eq!(
            super::to_plain_text("See [the spec](https://www.w3.org/TR/SVG2/) for more"),
            "See the spec (https://www.w3.org/TR/SVG2/) for more"
        );
    }

    #[test]
    fn plain_text_reduces_an_image_to_what_it_describes() {
        // The Baseline badges are images, and their target is a long data or
        // shields URI that says nothing to a reader. The alt text is the part
        // that carries the meaning.
        assert_eq!(
            super::to_plain_text("![Baseline Widely available](https://example.invalid/b.svg)"),
            "Baseline Widely available"
        );
    }

    #[test]
    fn plain_text_drops_italics_and_strikethrough() {
        // Baseline lines are italic and deprecated descriptions are struck
        // through; both were left on screen by the first version of this.
        assert_eq!(
            super::to_plain_text("![Baseline icon](data:x) _Widely Available since 2018_"),
            "Baseline icon Widely Available since 2018"
        );
        assert_eq!(
            super::to_plain_text("~~Use `stroke-width` instead~~"),
            "Use stroke-width instead"
        );
    }

    #[test]
    fn plain_text_never_renames_a_css_identifier() {
        // Class and custom-property hovers quote CSS the user wrote. An
        // identifier that opens with an underscore closes nothing, and
        // dropping it would name a different symbol than the one hovered.
        assert_eq!(
            super::to_plain_text("`--_accent` is defined here"),
            "--_accent is defined here"
        );
        assert_eq!(
            super::to_plain_text("Defined as:\n```css\n._icon { fill: red }\n```\n"),
            "Defined as:\n._icon { fill: red }\n"
        );
        assert_eq!(
            super::to_plain_text("`.trailing_` and `._leading`"),
            ".trailing_ and ._leading"
        );
        // A code span carries its content through as written, so markup
        // characters inside one are content rather than syntax.
        assert_eq!(
            super::to_plain_text("`a **b** _c_ ~~d~~`"),
            "a **b** _c_ ~~d~~"
        );
    }

    #[test]
    fn plain_text_unwraps_markdown_escapes() {
        // Metadata is escaped for Markdown before it reaches the converter, so
        // a plain-text client would otherwise read the backslashes as prose.
        assert_eq!(
            super::to_plain_text("Chrome 1\\. See note\\_1\\."),
            "Chrome 1. See note_1."
        );
    }

    #[test]
    fn an_escaped_delimiter_neither_opens_nor_closes() {
        // The escape is unwrapped before anything reads the character, and the
        // scan for a partner skips escaped candidates, so emphasis cannot
        // close on one and leave the real closer stranded.
        assert_eq!(super::to_plain_text(r"_foo\_bar_"), "foo_bar");
        assert_eq!(super::to_plain_text(r"\_notemphasis\_"), "_notemphasis_");
        assert_eq!(super::to_plain_text(r"\*not bold\*"), "*not bold*");
    }

    #[test]
    fn plain_text_keeps_words_that_contain_underscores() {
        // The catalog describes `media_query_list`, and an underscore inside a
        // word is neither an opener nor a closer in CommonMark. Reading them
        // as emphasis leaves a plain-text reader with `mediaquerylist`.
        assert_eq!(
            super::to_plain_text("A media_query_list value"),
            "A media_query_list value"
        );
        assert_eq!(
            super::to_plain_text("_emphasis_ around media_query_list text"),
            "emphasis around media_query_list text"
        );
        // This one needs the opener check specifically: without it the word's
        // first underscore opens, and the closer of the real emphasis later in
        // the line closes it, swallowing everything between.
        assert_eq!(
            super::to_plain_text("media_query_list and _real_ emphasis"),
            "media_query_list and real emphasis"
        );
    }

    #[test]
    fn plain_text_drops_the_blockquote_marker() {
        // Every verdict headline is generated as a block quote, so this is on
        // the common path rather than an edge of it.
        assert_eq!(
            super::to_plain_text("> \u{2713} `rect` — safe to use"),
            "\u{2713} rect — safe to use"
        );
        // A `>` that is not a marker is a character like any other.
        assert_eq!(super::to_plain_text("a > b"), "a > b");
    }

    #[test]
    fn a_fence_is_closed_only_by_a_line_of_backticks() {
        // A CSS comment may carry a run of backticks. It is content, and the
        // preview promises it verbatim.
        let hover = super::format_definition_hover(
            std::iter::once((
                "/*\n``` a line of backticks inside a comment\n*/\n.a { fill: red }".to_owned(),
                super::HoverSourceLink {
                    label: "sheet.css".to_owned(),
                    target: "file:///sheet.css".to_owned(),
                },
            )),
            ".a",
        );
        let plain = super::to_plain_text(&hover);
        assert!(
            plain.contains("``` a line of backticks inside a comment")
                && plain.contains(".a { fill: red }"),
            "the whole rule should survive: {plain}"
        );
    }

    #[test]
    fn two_definitions_are_separated_without_a_visible_rule() {
        // `format_definition_hover` joins definitions with a thematic break,
        // so any class defined in two sheets carries one.
        let link = |name: &str| super::HoverSourceLink {
            label: format!("{name}.css"),
            target: format!("file:///{name}.css"),
        };
        let hover = super::format_definition_hover(
            [
                (".a { fill: red }".to_owned(), link("base")),
                (".a { fill: blue }".to_owned(), link("theme")),
            ]
            .into_iter(),
            ".a",
        );
        assert!(
            hover.contains("\n---\n"),
            "the Markdown carries the rule: {hover}"
        );
        let plain = super::to_plain_text(&hover);
        assert!(
            !plain.contains("---"),
            "the rule is markup and should not survive: {plain}"
        );
        // Both definitions do, still told apart by a blank line.
        assert!(
            plain.contains(".a { fill: red }")
                && plain.contains(".a { fill: blue }")
                && plain.contains("base.css")
                && plain.contains("theme.css"),
            "both definitions should survive: {plain}"
        );
        assert!(
            !plain.contains("\n\n\n"),
            "dropping the rule should not leave a gap: {plain}"
        );
    }

    #[test]
    fn an_autolink_gives_up_its_brackets_and_keeps_its_url() {
        // `metadata_link` writes http and https targets this way, and a
        // discouraged feature carries one by default, so this is the common
        // path rather than an edge of it.
        assert_eq!(
            super::to_plain_text(&super::metadata_link("https://example.com/retirement")),
            "https://example.com/retirement"
        );

        // A destination is literal. Reading through it loses the underscores
        // and hands the reader a URL that does not resolve.
        assert_eq!(
            super::to_plain_text("<https://example.com/_draft_>"),
            "https://example.com/_draft_"
        );

        assert_eq!(
            super::to_plain_text("see <https://a.example/x> and <https://b.example/y>"),
            "see https://a.example/x and https://b.example/y"
        );

        // An angle bracket that opens no link is a character like any other.
        assert_eq!(super::to_plain_text("a < b and c > d"), "a < b and c > d");
        assert_eq!(super::to_plain_text("<not a url>"), "<not a url>");
        assert_eq!(
            super::to_plain_text("<https://unclosed"),
            "<https://unclosed"
        );
    }

    #[test]
    fn a_code_span_is_closed_by_a_run_of_its_own_length() {
        // `metadata_code` wraps a value in a delimiter longer than anything
        // inside it, so a flag value carrying backticks needs the whole run
        // matched rather than the first backtick found.
        let wrapped = super::metadata_code("a`b");
        assert_eq!(super::to_plain_text(&wrapped), "a`b");
        assert_eq!(super::to_plain_text(&super::metadata_code("a``b")), "a``b");

        // Three backticks mid-line open a span, not a block; only a line that
        // begins with them is a fence.
        assert_eq!(
            super::to_plain_text("value ```x``` and more"),
            "value x and more"
        );
        assert_eq!(super::to_plain_text("plain `code` here"), "plain code here");
    }

    #[test]
    fn a_link_label_carries_its_brackets() {
        // A path may contain either bracket, and one of them would end the
        // label early — leaving the whole construct visible.
        let hover = super::format_definition_hover(
            std::iter::once((
                ".a { fill: red }".to_owned(),
                super::HoverSourceLink {
                    label: "theme]dark.css:1".to_owned(),
                    target: "file:///theme%5Ddark.css".to_owned(),
                },
            )),
            ".a",
        );
        assert!(
            hover.contains(r"[theme\]dark.css:1]("),
            "the label should be escaped where it is written: {hover}"
        );
        let plain = super::to_plain_text(&hover);
        assert!(
            plain.contains("theme]dark.css:1 (file:///theme%5Ddark.css)"),
            "and read back with the bracket and without the escape: {plain}"
        );
    }

    #[test]
    fn a_link_target_carries_its_parentheses() {
        // Parentheses are legal in a path, and an unpaired one would close the
        // destination early — in a Markdown client as much as here.
        let hover = super::format_definition_hover(
            std::iter::once((
                ".a { fill: red }".to_owned(),
                super::HoverSourceLink {
                    label: "sheet.css".to_owned(),
                    target: "file:///themes/dark(2)/sheet.css".to_owned(),
                },
            )),
            ".a",
        );
        assert!(
            hover.contains("file:///themes/dark%282%29/sheet.css"),
            "the destination should be escaped where it is written: {hover}"
        );
        let plain = super::to_plain_text(&hover);
        assert!(
            plain.ends_with("file:///themes/dark%282%29/sheet.css)"),
            "and survive the conversion whole: {plain}"
        );
    }

    #[test]
    fn plain_text_stays_linear_against_hostile_delimiters() {
        // Every delimiter that has to search for a partner gets the same
        // treatment as the bracket: a failed scan settles it for what follows
        // rather than restarting at each candidate. These inputs collapse to
        // little or nothing — a run of backticks is a run of empty code spans,
        // and `_ _` pairs are empty emphasis — so what is under test is the
        // time, not the text.
        for hostile in [
            "`".repeat(200_000),
            "_".repeat(200_000),
            format!("{}tail", "_ ".repeat(100_000)),
            format!("`{}", "x".repeat(200_000)),
        ] {
            let started = std::time::Instant::now();
            let _ = super::to_plain_text(&hostile);
            assert!(
                started.elapsed() < std::time::Duration::from_secs(2),
                "conversion should stay linear, took {:?}",
                started.elapsed()
            );
        }
    }

    #[test]
    fn plain_text_keeps_an_underscore_that_is_part_of_a_word() {
        // A custom property may be named with one, and dropping it would
        // rename the thing the hover is about.
        assert_eq!(
            super::to_plain_text("`--brand_accent` is defined here"),
            "--brand_accent is defined here"
        );
    }

    #[test]
    fn plain_text_unwraps_a_fenced_block_without_leaving_its_language() {
        // Dropping the backticks alone leaves `css` behind as a stray word on
        // its own line, which reads as part of the definition.
        assert_eq!(
            super::to_plain_text("Defined as:\n```css\n.a { fill: red }\n```\n"),
            "Defined as:\n.a { fill: red }\n"
        );
    }

    #[test]
    fn plain_text_does_not_rescan_for_every_unclosed_bracket() {
        // A CSS definition the user controls can carry many `[` and no `]`.
        // Restarting the search at each one is quadratic; the whole point is
        // that the first failed scan settles it for the rest.
        let hostile = "[".repeat(200_000);
        let started = std::time::Instant::now();
        let out = super::to_plain_text(&hostile);
        assert_eq!(out, hostile, "unpaired brackets are just characters");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "conversion should stay linear, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn plain_text_counts_parentheses_inside_a_target() {
        // A data URI can carry parentheses, so the closing one has to be the
        // matching one rather than the first.
        assert_eq!(
            super::to_plain_text("[swatch](data:image/svg+xml,%3Csvg%20fill%3Drgb(1,2,3)%3E)"),
            "swatch (data:image/svg+xml,%3Csvg%20fill%3Drgb(1,2,3)%3E)"
        );
    }

    #[test]
    fn plain_text_leaves_unpaired_brackets_alone() {
        // A bracket that opens nothing is just a bracket, and dropping it
        // would lose a character the document actually contains.
        assert_eq!(
            super::to_plain_text("an [unclosed link"),
            "an [unclosed link"
        );
        assert_eq!(super::to_plain_text("array[0] of them"), "array[0] of them");
    }

    #[test]
    fn plain_text_passes_multi_byte_characters_through() {
        assert_eq!(
            super::to_plain_text("**élan** — `stroke` ✓"),
            "élan — stroke ✓"
        );
    }
    #[test]
    fn baseline_containers_embed_exact_official_assets_at_icon_size()
    -> Result<(), Box<dyn std::error::Error>> {
        use base64::Engine as _;
        use quick_xml::{Reader, events::Event};

        for (tier, light, dark) in [
            (
                BaselineTier::Widely,
                include_str!("../assets/baseline-widely-icon.svg"),
                include_str!("../assets/baseline-widely-icon-dark.svg"),
            ),
            (
                BaselineTier::Newly,
                include_str!("../assets/baseline-newly-icon.svg"),
                include_str!("../assets/baseline-newly-icon-dark.svg"),
            ),
            (
                BaselineTier::Limited,
                include_str!("../assets/baseline-limited-icon.svg"),
                include_str!("../assets/baseline-limited-icon-dark.svg"),
            ),
        ] {
            let baseline = svg_data::BaselineStatus {
                status: Some(tier),
                ..svg_data::BaselineStatus::EMPTY
            };
            let markdown = format_baseline(&baseline);
            let uri = markdown
                .split("](")
                .nth(1)
                .ok_or("icon URL")?
                .split(')')
                .next()
                .ok_or("icon URL end")?;
            let xml = String::from_utf8(
                base64::engine::general_purpose::STANDARD.decode(
                    uri.strip_prefix("data:image/svg+xml;base64,")
                        .ok_or("data URI")?,
                )?,
            )?;
            let mut reader = Reader::from_str(&xml);
            let mut embedded = Vec::new();
            let mut root_seen = false;
            loop {
                match reader.read_event()? {
                    Event::Start(tag) if tag.name().as_ref() == b"svg" => {
                        root_seen = true;
                        assert_eq!(
                            tag.try_get_attribute("width")?
                                .ok_or("width")?
                                .value
                                .as_ref(),
                            b"18"
                        );
                        assert_eq!(
                            tag.try_get_attribute("height")?
                                .ok_or("height")?
                                .value
                                .as_ref(),
                            b"10"
                        );
                        assert_eq!(
                            tag.try_get_attribute("viewBox")?
                                .ok_or("viewBox")?
                                .value
                                .as_ref(),
                            b"0 0 18 10"
                        );
                    }
                    Event::Empty(tag) if tag.name().as_ref() == b"image" => {
                        let class = tag.try_get_attribute("class")?.ok_or("theme")?;
                        let href = tag.try_get_attribute("href")?.ok_or("embedded asset")?;
                        let uri = std::str::from_utf8(&href.value)?;
                        embedded.push((
                            String::from_utf8(class.value.to_vec())?,
                            base64::engine::general_purpose::STANDARD.decode(
                                uri.strip_prefix("data:image/svg+xml;base64,")
                                    .ok_or("embedded data URI")?,
                            )?,
                        ));
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
            assert!(root_seen);
            assert_eq!(
                embedded,
                vec![
                    ("light".to_owned(), light.as_bytes().to_vec()),
                    ("dark".to_owned(), dark.as_bytes().to_vec())
                ]
            );
            assert!(xml.contains("prefers-color-scheme:dark"));
        }
        Ok(())
    }

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
                &crate::hover_settings::HoverSettings::default(),
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
            &crate::hover_settings::HoverSettings::default(),
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
            version_added: Some(svg_data::browser_compat::VersionAdded::Version(
                "preview".to_owned(),
            )),
            ..BrowserVersion::default()
        }
    }

    #[test]
    fn preview_browser_version_is_preserved() {
        let baked = BrowserSupport::from([("chrome".to_owned(), vec![bv_unknown()])]);

        // New separator is ` · ` (prose bullet) instead of ` | `.
        assert_eq!(
            format_browser_support_line(
                Some(&baked),
                &crate::hover_settings::HoverSettings::default()
            ),
            Some("Chrome preview · Edge unknown · Firefox unknown · Safari unknown".to_owned())
        );
    }

    #[test]
    fn absent_browser_support_omits_chip_row() {
        assert_eq!(
            format_browser_support_line(None, &crate::hover_settings::HoverSettings::default()),
            None
        );
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
                crate::hover::AttributeHoverContext {
                    element_name: None,
                    profile: SpecSnapshotId::LATEST,
                    profile_lifecycle: None,
                    rt: None,
                    native: None,
                    settings: &crate::hover_settings::HoverSettings::default(),
                },
            );

            assert!(hover.contains(label), "{name}: {hover}");
            assert!(!hover.contains("Grammar:"), "{name}: {hover}");
            assert!(!hover.contains("Keywords:"), "{name}: {hover}");
        }
    }

    #[test]
    fn qualified_browser_versions_are_not_double_prefixed() {
        let version = BrowserVersion {
            version_added: Some(svg_data::browser_compat::VersionAdded::Version(
                "≤80".to_owned(),
            )),
            ..Default::default()
        };
        assert_eq!(browser_version_label(Some(&version)), "≤80");
        assert_eq!(format_browser_version("≤13.1"), "≤13.1");
        let removed = BrowserVersion {
            version_removed: Some("≤100".to_owned()),
            ..version
        };
        assert_eq!(browser_version_label(Some(&removed)), "≤80 (removed)");
        let reason = svg_data::VerdictReason::RemovedIn {
            browser: "chrome".to_owned(),
            version: "≤100".to_owned(),
        };
        assert_eq!(format_verdict_reason(&reason), "removed in chrome ≤100");
    }

    #[test]
    fn missing_and_unknown_browser_data_stays_unknown() {
        let support =
            BrowserSupport::from([("chrome".to_owned(), vec![BrowserVersion::default()])]);
        assert_eq!(
            format_browser_support_line(
                Some(&support),
                &crate::hover_settings::HoverSettings::default()
            )
            .as_deref(),
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

    #[test]
    fn browser_details_are_selectable_without_changing_facts()
    -> Result<(), Box<dyn std::error::Error>> {
        let raw = serde_json::from_str(include_str!(
            "../../svg-data/src/fixtures/browser-support.json"
        ))?;
        let support = svg_data::browser_compat::extract_browser_support(&raw).ok_or("support")?;
        let original = support.clone();
        let mut settings = HoverSettings::from_config(&serde_json::json!({"svg":{"hover":{
            "browsers":["chrome"], "browser_history":true,
            "browser_details":["version_last","implementation_links","flags","notes"]
        }}}))?;
        let text = format_browser_notes_list(Some(&support), &settings)
            .ok_or("history")?
            .join("\n");
        for expected in [
            "≤20",
            "80",
            "last supported in 69",
            "preference",
            "runtime_flag",
            "true",
            "implementation",
            "First caveat",
            "second",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }
        assert!(!text.contains("ships as"), "{text}");
        assert!(!text.contains("partial implementation"), "{text}");
        settings.browser_history = false;
        assert!(format_browser_notes_list(Some(&support), &settings).is_none());
        assert_eq!(support, original);
        Ok(())
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
        let verdict = reconcile_verdict(
            Some(&previous),
            &Facts::default(),
            &HoverSettings::default(),
        );
        assert_eq!(
            verdict.map(|v| v.reasons),
            Some(vec![svg_data::VerdictReason::ProfileObsolete {
                last_seen: SpecSnapshotId::Svg11Rec20110816
            }])
        );
    }
}
