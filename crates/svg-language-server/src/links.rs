use std::str::FromStr;

use tower_lsp_server::ls_types::{DocumentLink, Uri};
use tree_sitter::Tree;
use url::Url;

use crate::{positions::LineIndex, references::span_range};

pub fn document_links(source: &str, tree: &Tree, base: &Uri) -> Vec<DocumentLink> {
    let bytes = source.as_bytes();
    let base = Url::parse(base.as_str()).ok();
    let index = LineIndex::new(bytes);
    svg_references::external_links(bytes, tree)
        .into_iter()
        .filter_map(|link| {
            let resolved = Url::parse(&link.target).ok().or_else(|| {
                let mut resolved = base.clone()?;
                for xml_base in &link.base {
                    resolved = resolved.join(xml_base).ok()?;
                }
                resolved.join(&link.target).ok()
            })?;
            let target = Uri::from_str(resolved.as_str()).ok()?;
            Some(DocumentLink {
                range: span_range(&index, bytes, &link.span),
                target: Some(target),
                tooltip: None,
                data: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use tower_lsp_server::ls_types::{Position, Range};

    use super::*;

    fn parse(source: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        assert!(
            parser
                .set_language(&tree_sitter_svg::LANGUAGE.into())
                .is_ok()
        );
        parser
            .parse(source, None)
            .unwrap_or_else(|| unreachable!("parser has a language"))
    }

    #[test]
    fn resolves_relative_and_absolute_targets() -> Result<(), Box<dyn std::error::Error>> {
        let source = concat!(
            "<svg><use href=\"#local\"/><image href=\"img/a.png\"/>",
            "<a href=\"https://example.com/x\"/><use href=\"../s.svg#i\"/></svg>",
        );
        let base = Uri::from_str("file:///project/icons/doc.svg")?;
        let links = document_links(source, &parse(source), &base);
        let targets: Vec<_> = links
            .iter()
            .filter_map(|link| {
                link.target
                    .as_ref()
                    .map(|target| target.as_str().to_owned())
            })
            .collect();
        assert_eq!(
            targets,
            vec![
                "file:///project/icons/img/a.png",
                "https://example.com/x",
                "file:///project/s.svg#i",
            ]
        );
        assert_eq!(
            links[0].range,
            Range::new(Position::new(0, 38), Position::new(0, 47))
        );
        Ok(())
    }

    #[test]
    fn resolves_relative_targets_against_xml_base() -> Result<(), Box<dyn std::error::Error>> {
        let source = concat!(
            "<svg xml:base=\"assets/\"><image href=\"a.png\"/>",
            "<g xml:base=\"https://cdn.example/lib/\"><use href=\"s.svg#i\"/></g></svg>",
        );
        let base = Uri::from_str("file:///project/icon.svg")?;
        let targets: Vec<_> = document_links(source, &parse(source), &base)
            .into_iter()
            .filter_map(|link| link.target.map(|target| target.as_str().to_owned()))
            .collect();
        assert_eq!(
            targets,
            vec![
                "file:///project/assets/a.png",
                "https://cdn.example/lib/s.svg#i",
            ]
        );
        Ok(())
    }
}
