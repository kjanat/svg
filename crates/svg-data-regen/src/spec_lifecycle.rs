//! Extract explicit lifecycle declarations from specification prose.
//!
//! Membership remains a separate input. Only declarations about an element,
//! attribute or property are accepted; obsolete values, DOM APIs, prospective
//! resolutions and replacement/alias relationships do not classify a feature.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::catalog::{
    CatalogDeclaredStatus, CatalogInventory, CatalogLifecycleDeclaration, CatalogLifecycleEntry,
    CatalogLifecycleStatus, CatalogSnapshot, CatalogSpecSnapshotId,
};
use crate::util::{boxed, compile_regex, normalize_html_ws, parse_html};
use crate::{Fallible, fetch};

static SUBJECT: LazyLock<Regex> = LazyLock::new(|| {
    compile_regex(r"'([A-Za-z_][A-Za-z0-9_.:-]*)'\s+(property|attribute|element)\b")
});
static PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    compile_regex(
        r"(?i)\b(deprecated|obsoleted?|removed)\s+'([A-Za-z_][A-Za-z0-9_.:-]*)'\s+(?:property|attribute|element)\b",
    )
});
static SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    compile_regex(
        r"(?i)'([A-Za-z_][A-Za-z0-9_.:-]*)'\s+(?:(?:property|attribute|element)\s+(?:is\s+|has\s+(?:also\s+)?been\s+|which\s+has\s+also\s+been\s+)?)?(deprecated|obsoleted?|removed)\b",
    )
});
static THIS: LazyLock<Regex> = LazyLock::new(|| {
    compile_regex(
        r"(?i)^This (?:property|attribute|element)\b.*?\b(?:is|has been|It has been) (deprecated|obsoleted?|removed)\b",
    )
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub element: bool,
    pub owner: Option<String>,
    pub fact: CatalogLifecycleDeclaration,
}

fn text(tag: &tl::HTMLTag<'_>, parser: &tl::Parser<'_>) -> String {
    normalize_html_ws(&tag.inner_text(parser)).replace(['‘', '’'], "'")
}

fn status(word: &str) -> CatalogDeclaredStatus {
    match word.to_ascii_lowercase().as_str() {
        "deprecated" => CatalogDeclaredStatus::Deprecated,
        "removed" => CatalogDeclaredStatus::Removed,
        _ => CatalogDeclaredStatus::Obsolete,
    }
}

/// Extract declarations from one chapter. Definitions and headings supply the
/// subject for "this property" statements; explicit quoted subjects stand alone.
pub fn extract(html: &str, source: &str) -> Fallible<Vec<Declaration>> {
    let dom = parse_html(html)?;
    let parser = dom.parser();
    let excluded = excluded_ranges(&dom);
    let mut anchor = String::new();
    let mut subject: Option<(String, bool)> = None;
    let mut owner: Option<String> = None;
    let mut result = Vec::new();
    for node in dom.nodes() {
        let Some(tag) = node.as_tag() else { continue };
        let (start, end) = tag.boundaries(parser);
        if excluded.iter().any(|(a, b)| start >= *a && end <= *b) {
            continue;
        }
        let name = tag.name().as_utf8_str();
        if matches!(name.as_ref(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            anchor = tag
                .attributes()
                .id()
                .map_or_else(String::new, |id| id.as_utf8_str().into_owned());
            subject = SUBJECT
                .captures(&text(tag, parser))
                .map(|c| (c[1].to_owned(), &c[2] == "element"));
            owner = None;
        }
        if name == "dfn"
            && tag
                .attributes()
                .get("data-dfn-type")
                .flatten()
                .is_some_and(|v| {
                    matches!(
                        v.as_utf8_str().as_ref(),
                        "element-attr" | "property" | "element"
                    )
                })
        {
            anchor = tag
                .attributes()
                .id()
                .map_or_else(String::new, |id| id.as_utf8_str().into_owned());
            let element = tag
                .attributes()
                .get("data-dfn-type")
                .flatten()
                .is_some_and(|v| v.as_utf8_str() == "element");
            subject = Some((text(tag, parser), element));
            owner = tag
                .attributes()
                .get("data-dfn-for")
                .flatten()
                .map(|v| v.as_utf8_str().into_owned())
                .filter(|v| !v.is_empty() && !v.ends_with("-attributes"));
        }
        if name != "p" {
            continue;
        }
        let found = paragraph_declarations(&text(tag, parser), subject.as_ref())
            .map_err(|e| boxed(format!("{source}#{anchor}: {e}")))?;
        for (name, element, status) in found {
            if anchor.is_empty() {
                return Err(boxed(format!(
                    "{source}: lifecycle declaration for {name} has no source anchor"
                )));
            }
            let context = subject
                .as_ref()
                .filter(|(n, _)| n == &name)
                .and_then(|_| owner.clone());
            let owners: Vec<_> = context.as_deref().map_or_else(
                || vec![None],
                |value| {
                    value
                        .split([',', ' '])
                        .filter(|s| !s.is_empty())
                        .map(|s| Some(s.to_owned()))
                        .collect()
                },
            );
            for owner in owners {
                result.push(Declaration {
                    name: name.clone(),
                    element,
                    owner,
                    fact: CatalogLifecycleDeclaration {
                        status,
                        source: format!("{source}#{anchor}"),
                    },
                });
            }
        }
    }
    Ok(result)
}

fn excluded_ranges(dom: &tl::VDom<'_>) -> Vec<(usize, usize)> {
    dom.nodes()
        .iter()
        .filter_map(|node| {
            let tag = node.as_tag()?;
            (matches!(
                tag.name().as_utf8_str().as_ref(),
                "pre" | "script" | "style" | "del"
            ) || ["issue", "example", "requirement", "svg2-requirement"]
                .iter()
                .any(|class| crate::util::has_class(tag, class)))
            .then(|| tag.boundaries(dom.parser()))
        })
        .collect()
}

fn paragraph_declarations(
    prose: &str,
    subject: Option<&(String, bool)>,
) -> Fallible<Vec<(String, bool, CatalogDeclaredStatus)>> {
    let mut found = Vec::new();
    if let Some(c) = THIS.captures(prose) {
        if let Some((name, element)) = subject {
            if *element != prose.starts_with("This element") {
                return Err(boxed(
                    "lifecycle declaration does not match its definition's subject kind",
                ));
            }
            found.push((name.clone(), *element, status(&c[1])));
        } else if prose.starts_with("This property") {
            return Err(boxed(
                "property lifecycle declaration has no identifiable definition",
            ));
        }
    }
    if (prose.starts_with("Deprecated attribute") || prose.starts_with("Deprecated XML attribute"))
        && let Some((name, element)) = subject
    {
        found.push((name.clone(), *element, CatalogDeclaredStatus::Deprecated));
    }
    for c in PREFIX.captures_iter(prose) {
        let prefix = c.get(0).map_or("", |m| &prose[..m.start()]);
        if prefix.ends_with("non-") || prefix.ends_with("not ") {
            continue;
        }
        found.push((c[2].to_owned(), c[0].ends_with("element"), status(&c[1])));
    }
    for c in SUFFIX.captures_iter(prose) {
        found.push((c[1].to_owned(), c[0].contains(" element "), status(&c[2])));
    }
    Ok(found)
}

/// Fetch the dated historical chapters or the same commit used by definitions.
/// The publication's table of contents discovers chapters; no cached verdicts
/// or manually classified feature list participates in extraction.
pub fn fetch_declarations(
    profile: CatalogSpecSnapshotId,
    commit: &str,
    draft_chapters: &[String],
) -> Fallible<Vec<Declaration>> {
    let (base, chapters) = if profile == CatalogSpecSnapshotId::Svg2EditorsDraft {
        (
            format!("https://raw.githubusercontent.com/w3c/svgwg/{commit}/master/"),
            draft_chapters
                .iter()
                .map(|name| format!("{name}.html"))
                .collect::<BTreeSet<_>>(),
        )
    } else {
        let base = match profile {
            CatalogSpecSnapshotId::Svg11Rec20030114 => {
                "https://www.w3.org/TR/2003/REC-SVG11-20030114/"
            }
            CatalogSpecSnapshotId::Svg11Rec20110816 => {
                "https://www.w3.org/TR/2011/REC-SVG11-20110816/"
            }
            _ => "https://www.w3.org/TR/2018/CR-SVG2-20181004/",
        };
        let overview_url = if profile == CatalogSpecSnapshotId::Svg11Rec20030114 {
            base.to_owned()
        } else {
            format!("{base}Overview.html")
        };
        let overview = fetch::url_text(&overview_url, "text/html")
            .map_err(|e| boxed(format!("{overview_url}: {e}")))?;
        let dom = parse_html(&overview)?;
        let chapters = dom
            .nodes()
            .iter()
            .filter_map(|node| {
                let tag = node.as_tag()?;
                if tag.name().as_utf8_str() != "a" {
                    return None;
                }
                let href = tag.attributes().get("href").flatten()?.as_utf8_str();
                let page = href.split('#').next()?;
                (std::path::Path::new(page)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
                    && !page.contains(['/', ':'])
                    && page != "Overview.html")
                    .then(|| page.to_owned())
            })
            .collect::<BTreeSet<_>>();
        if chapters.is_empty() {
            return Err(boxed(format!(
                "{base}: no chapters discovered for lifecycle extraction"
            )));
        }
        (base.to_owned(), chapters)
    };
    let mut facts = BTreeMap::<(bool, String, Option<String>), Declaration>::new();
    for chapter in chapters {
        // Change logs describe other editions and are not declarations for this one.
        if matches!(
            chapter.as_str(),
            "single-page.html"
                | "changes.html"
                | "eltindex.html"
                | "attindex.html"
                | "propidx.html"
                | "idlindex.html"
        ) {
            continue;
        }
        let source = format!("{base}{chapter}");
        let html =
            fetch::url_text(&source, "text/html").map_err(|e| boxed(format!("{source}: {e}")))?;
        for declaration in extract(&html, &source)? {
            let key = (
                declaration.element,
                declaration.name.clone(),
                declaration.owner.clone(),
            );
            if let Some(previous) = facts.get(&key) {
                if previous.fact.status != declaration.fact.status {
                    return Err(boxed(format!(
                        "conflicting lifecycle declarations for {}: {} and {}",
                        declaration.name, previous.fact.source, declaration.fact.source
                    )));
                }
            } else {
                facts.insert(key, declaration);
            }
        }
    }
    let declarations: Vec<_> = facts
        .values()
        .filter(|d| {
            d.owner.is_none()
                || !facts
                    .get(&(d.element, d.name.clone(), None))
                    .is_some_and(|global| global.fact.status == d.fact.status)
        })
        .cloned()
        .collect();
    println!(
        "  {profile:?}: {} explicit lifecycle declarations",
        declarations.len()
    );
    Ok(declarations)
}

/// Validate subjects against the cross-edition inventory, then retain source
/// declarations independently of whether a feature is defined in this edition.
pub fn apply(
    snapshot: &mut CatalogSnapshot,
    inventories: &[CatalogInventory],
    declarations: &[Declaration],
) -> Fallible<()> {
    for declaration in declarations {
        if let Some(owner) = &declaration.owner
            && !snapshot.inventory.elements.iter().any(|e| {
                &e.name == owner
                    && (e.attributes.contains(&declaration.name)
                        || declaration.fact.status == CatalogDeclaredStatus::Removed)
            })
        {
            return Err(boxed(format!(
                "{:?}: lifecycle declaration for {owner}/{} at {} has no applicable attribute \
                 definition",
                snapshot.profile, declaration.name, declaration.fact.source
            )));
        }
        let contains = |inventory: &CatalogInventory| {
            if declaration.element {
                inventory
                    .elements
                    .iter()
                    .any(|e| e.name == declaration.name)
            } else if let Some(owner) = &declaration.owner {
                inventory
                    .elements
                    .iter()
                    .any(|e| &e.name == owner && e.attributes.contains(&declaration.name))
            } else {
                inventory.attributes.contains(&declaration.name)
            }
        };
        let known_in: Vec<_> = inventories
            .iter()
            .filter(|i| contains(i))
            .map(|i| i.profile)
            .collect();
        if known_in.is_empty() {
            return Err(boxed(format!(
                "{:?}: lifecycle subject {} from {} is absent from every inventory",
                snapshot.profile, declaration.name, declaration.fact.source
            )));
        }
        if declaration.fact.status != CatalogDeclaredStatus::Removed
            && !known_in.contains(&snapshot.profile)
        {
            return Err(boxed(format!(
                "{:?}: retained feature {} at {} is missing from this edition",
                snapshot.profile, declaration.name, declaration.fact.source
            )));
        }
        let entries = if declaration.element {
            &mut snapshot.lifecycle.elements
        } else {
            &mut snapshot.lifecycle.attributes
        };
        let entry = if let Some(index) = entries
            .iter()
            .position(|e| e.name == declaration.name && e.owner == declaration.owner)
        {
            &mut entries[index]
        } else {
            let canonical = crate::catalog::canonical_attribute_name(&declaration.name);
            let catalog_name =
                (canonical.as_ref() != declaration.name).then(|| canonical.into_owned());
            entries.push(CatalogLifecycleEntry {
                name: declaration.name.clone(),
                owner: declaration.owner.clone(),
                catalog_name,
                present: known_in.contains(&snapshot.profile),
                lifecycle: CatalogLifecycleStatus::Stable,
                known_in,
                declaration: None,
            });
            entries
                .last_mut()
                .ok_or("missing newly inserted lifecycle entry")?
        };
        entry.lifecycle = match declaration.fact.status {
            CatalogDeclaredStatus::Deprecated => CatalogLifecycleStatus::Deprecated,
            CatalogDeclaredStatus::Obsolete | CatalogDeclaredStatus::Removed => {
                CatalogLifecycleStatus::Obsolete
            }
        };
        if declaration.fact.status == CatalogDeclaredStatus::Removed {
            entry.present = false;
        }
        entry.declaration = Some(declaration.fact.clone());
    }
    snapshot
        .lifecycle
        .elements
        .sort_by(|a, b| a.name.cmp(&b.name));
    snapshot
        .lifecycle
        .attributes
        .sort_by(|a, b| (&a.name, &a.owner).cmp(&(&b.name, &b.owner)));
    Ok(())
}

/// Apply normative removals to the index inventory. Restore a retained alias
/// only when its canonical attribute is already defined, preserving its bearers.
/// This also makes cross-edition `known_in` reflect explicit removals.
pub fn reconcile_inventory(
    inventory: &mut CatalogInventory,
    declarations: &[Declaration],
) -> Fallible<()> {
    for declaration in declarations {
        let name = &declaration.name;
        if declaration.fact.status == CatalogDeclaredStatus::Removed {
            if declaration.element {
                inventory.elements.retain(|e| &e.name != name);
            } else {
                if declaration.owner.is_none() {
                    inventory.attributes.retain(|a| a != name);
                }
                for element in &mut inventory.elements {
                    if declaration
                        .owner
                        .as_ref()
                        .is_none_or(|owner| owner == &element.name)
                    {
                        element.attributes.retain(|a| a != name);
                    }
                }
            }
        } else if !declaration.element && !inventory.attributes.contains(name) {
            let canonical = crate::catalog::canonical_attribute_name(name);
            if canonical.as_ref() == name
                || !inventory.attributes.iter().any(|a| a == canonical.as_ref())
            {
                return Err(boxed(format!(
                    "{:?}: retained feature {name} at {} has no definition/applicability in the \
                     inventory",
                    inventory.profile, declaration.fact.source
                )));
            }
            inventory.attributes.push(name.clone());
            for element in &mut inventory.elements {
                if declaration
                    .owner
                    .as_ref()
                    .is_none_or(|owner| owner == &element.name)
                    && element.attributes.iter().any(|a| a == canonical.as_ref())
                {
                    element.attributes.push(name.clone());
                    element.attributes.sort();
                    element.attributes.dedup();
                }
            }
        }
    }
    inventory.attributes.sort();
    inventory.attributes.dedup();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_reconcile_membership_without_leaking_attribute_scope() -> Fallible<()> {
        let declarations = extract(
            r#"
            <h2 id="text">Text</h2><p>The 'glyph-orientation-horizontal' property is removed.</p>
            <h2 id="vertical">The 'glyph-orientation-vertical' property</h2>
            <p>This property has been obsoleted.</p>
            <dfn id="link" data-dfn-type="element-attr" data-dfn-for="use">xlink:href</dfn>
            <p>This attribute is deprecated.</p>
            "#,
            "https://example.org/edition/text.html",
        )?;
        let mut current = text_and_links_inventory();
        let mut historical = current.clone();
        historical.profile = CatalogSpecSnapshotId::Svg11Rec20110816;
        reconcile_inventory(&mut current, &declarations)?;
        assert!(
            !current
                .attributes
                .iter()
                .any(|a| a == "glyph-orientation-horizontal")
        );
        assert!(
            current.elements[1]
                .attributes
                .iter()
                .any(|a| a == "xlink:href")
        );
        assert!(
            !current.elements[2]
                .attributes
                .iter()
                .any(|a| a == "xlink:href")
        );
        let inventories = [historical, current.clone()];
        let mut snapshot = CatalogSnapshot::from_inventory(&current, &inventories, &[], &[]);
        apply(&mut snapshot, &inventories, &declarations)?;
        for (name, present, status) in [
            (
                "glyph-orientation-horizontal",
                false,
                CatalogDeclaredStatus::Removed,
            ),
            (
                "glyph-orientation-vertical",
                true,
                CatalogDeclaredStatus::Obsolete,
            ),
            ("xlink:href", true, CatalogDeclaredStatus::Deprecated),
        ] {
            let entry = snapshot
                .lifecycle
                .attributes
                .iter()
                .find(|e| e.name == name && e.declaration.is_some())
                .ok_or("lifecycle entry")?;
            assert_eq!(entry.present, present, "{name}");
            assert_eq!(
                entry
                    .declaration
                    .as_ref()
                    .ok_or("source declaration")?
                    .status,
                status
            );
        }
        let mut invalid = declarations[2].clone();
        invalid.owner = Some("missing-element".into());
        let error = apply(&mut snapshot, &inventories, &[invalid])
            .err()
            .ok_or("invalid bearer must fail")?;
        assert!(error.to_string().contains("missing-element/xlink:href"));
        Ok(())
    }

    fn text_and_links_inventory() -> CatalogInventory {
        use crate::catalog::CatalogInventoryElement;
        CatalogInventory {
            profile: CatalogSpecSnapshotId::Svg2EditorsDraft,
            sources: vec![],
            elements: vec![
                CatalogInventoryElement {
                    name: "text".into(),
                    attributes: vec![
                        "glyph-orientation-horizontal".into(),
                        "glyph-orientation-vertical".into(),
                    ],
                },
                CatalogInventoryElement {
                    name: "use".into(),
                    attributes: vec!["href".into()],
                },
                CatalogInventoryElement {
                    name: "image".into(),
                    attributes: vec!["href".into()],
                },
            ],
            attributes: vec![
                "glyph-orientation-horizontal".into(),
                "glyph-orientation-vertical".into(),
                "href".into(),
            ],
        }
    }

    #[test]
    fn declarations_keep_the_subject_and_ignore_values_aliases_and_dom_apis() -> Fallible<()> {
        let declarations = extract(
            r#"
            <h4 id="vertical">The <span>'glyph-orientation-vertical'</span> property</h4>
            <p>This property applies only to vertical text. It has been obsoleted in SVG 2.</p>
            <h4 id="writing">The 'writing-mode' property</h4>
            <p>The SVG 1.1 values are obsolete but must still be supported.</p>
            <h4 id="font">The 'font-stretch' property</h4>
            <p>For historical reasons, this property is a legacy name alias of 'font-width'.</p>
            <h3 id="dom">DOM interfaces</h3><p>This attribute is deprecated.</p>
            <h3 id="links">Links</h3>
            <p>The deprecated 'xlink:href' attribute is retained alongside the 'xlink:title' attribute which has also been deprecated.</p>
            <p>The 'href' attribute is not deprecated.</p>
            <p>The 'fill' property may be deprecated in the future.</p>
            <p>The non-deprecated 'fill' property remains supported.</p>
            <!-- <p>The 'stroke' property is deprecated.</p> -->
            <div class="issue"><p>The 'filter' property is deprecated.</p></div>
            <div class="example"><p>The 'mask' property is deprecated.</p></div>
        "#,
            "https://example.org/edition/text.html",
        )?;
        assert_eq!(
            declarations
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>(),
            ["glyph-orientation-vertical", "xlink:href", "xlink:title"]
        );
        assert_eq!(declarations[0].fact.status, CatalogDeclaredStatus::Obsolete);
        assert!(declarations[0].fact.source.ends_with("#vertical"));
        Ok(())
    }

    #[test]
    fn attribute_definition_preserves_bearer_and_global_xml_scope() -> Fallible<()> {
        let declarations = extract(
            r#"
            <h2 id="style">Style</h2><dl><dt><table><tr><td>
            <dfn id="type" data-dfn-type="element-attr" data-dfn-for="style">type</dfn>
            </td></tr></table></dt><dd><p>This attribute is obsolete and should not be used.</p></dd></dl>
            <h2 id="core">Core attributes</h2><dl><dt>
            <dfn id="space" data-dfn-type="element-attr" data-dfn-for="core-attributes">xml:space</dfn>
            </dt><dd><p>Deprecated XML attribute to control whitespace.</p></dd></dl>
        "#,
            "https://example.org/edition/struct.html",
        )?;
        assert_eq!(declarations.len(), 2);
        assert_eq!(declarations[0].owner.as_deref(), Some("style"));
        assert_eq!(declarations[1].owner, None);
        assert!(declarations[0].fact.source.ends_with("#type"));
        Ok(())
    }

    #[test]
    fn missing_definition_metadata_fails_instead_of_losing_a_declaration() {
        for html in [
            "<h4>The 'fill' property</h4><p>This property is deprecated.</p>",
            "<h4 id='fill'>Property</h4><p>This property is deprecated.</p>",
            "<h4 id='style'>The 'style' element</h4><p>This attribute is obsolete.</p>",
        ] {
            assert!(extract(html, "https://example.org/spec").is_err(), "{html}");
        }
    }

    #[test]
    fn lifecycle_validation_rejects_unknown_subjects() -> Fallible<()> {
        let inventory = CatalogInventory {
            profile: CatalogSpecSnapshotId::Svg2EditorsDraft,
            sources: vec![],
            elements: vec![],
            attributes: vec!["fill".into()],
        };
        let mut snapshot =
            CatalogSnapshot::from_inventory(&inventory, std::slice::from_ref(&inventory), &[], &[]);
        let declaration = Declaration {
            name: "misspelled".into(),
            element: false,
            owner: None,
            fact: CatalogLifecycleDeclaration {
                status: CatalogDeclaredStatus::Deprecated,
                source: "https://example.org/spec#missing".into(),
            },
        };
        let error = apply(&mut snapshot, &[inventory], &[declaration])
            .err()
            .ok_or("unknown lifecycle subject must fail")?;
        assert!(error.to_string().contains("misspelled"));
        assert!(
            error
                .to_string()
                .contains("https://example.org/spec#missing")
        );
        Ok(())
    }
}
