//! Regression tests for non-ASCII `id` and `class` attribute values.

use svg_lint::{DiagnosticCode, SvgDiagnostic, lint};
use tree_sitter::Parser;

fn document(attribute: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><rect {attribute} width="1" height="1"></rect></svg>"#
    )
}

fn assert_parses_and_lints_clean(attribute: &str) -> Result<(), Box<dyn std::error::Error>> {
    let source = document(attribute);
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
    let tree = parser.parse(&source, None).ok_or("parse failed")?;
    assert!(
        !tree.root_node().has_error(),
        "{attribute}: {}",
        tree.root_node().to_sexp()
    );
    let diagnostics: Vec<SvgDiagnostic> = lint(source.as_bytes());
    assert_eq!(diagnostics, [], "{attribute}");
    Ok(())
}

#[test]
fn non_ascii_id_values_lint_clean() -> Result<(), Box<dyn std::error::Error>> {
    for attribute in [r#"id="😀""#, r#"id="𠀀""#, r#"id="é""#, r#"id="中""#] {
        assert_parses_and_lints_clean(attribute)?;
    }
    Ok(())
}

#[test]
fn non_ascii_class_values_lint_clean() -> Result<(), Box<dyn std::error::Error>> {
    for attribute in [
        r#"class="😀""#,
        r#"class="é""#,
        r#"class="md:flex""#,
        r#"class=" 𠀀  w-1/2 [&amp;>*]:p-4 ""#,
        r#"class=" ""#,
        r#"class='a"b'"#,
    ] {
        assert_parses_and_lints_clean(attribute)?;
    }
    Ok(())
}

#[test]
fn duplicate_astral_ids_are_reported() {
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg"><g id="😀"/><g id="😀"/></svg>"#;
    let codes: Vec<DiagnosticCode> = lint(source.as_bytes())
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        !codes.is_empty()
            && codes
                .iter()
                .all(|code| *code == DiagnosticCode::DuplicateId),
        "{codes:?}"
    );
}
