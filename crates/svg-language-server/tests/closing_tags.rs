//! Protocol coverage for matching closing tags and applying their edits.

mod support;

use serde_json::{Value, json};
use support::TestServer;
use tower_lsp_server::ls_types::{Position, TextEdit};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn position_at(source: &str, offset: usize) -> TestResult<Position> {
    let prefix = &source[..offset];
    Ok(Position::new(
        u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count())?,
        u32::try_from(
            prefix
                .rsplit('\n')
                .next()
                .ok_or("line")?
                .encode_utf16()
                .count(),
        )?,
    ))
}

fn offset_at(source: &str, position: Position) -> TestResult<usize> {
    for offset in source
        .char_indices()
        .map(|(offset, _)| offset)
        .chain([source.len()])
    {
        if position_at(source, offset)? == position {
            return Ok(offset);
        }
    }
    Err("edit position is not a character boundary in the document".into())
}

fn complete(server: &mut TestServer, marked: &str) -> TestResult<(String, Position, Value)> {
    let offset = marked.find('|').ok_or("cursor marker")?;
    let source = marked.replacen('|', "", 1);
    let position = position_at(&source, offset)?;
    server.open("file:///closing-tags.svg", &source)?;
    let response = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///closing-tags.svg" },
            "position": position,
        }),
    )?;
    server.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": "file:///closing-tags.svg" } }),
    )?;
    Ok((source, position, response["result"].clone()))
}

const MATCHING_TAG_CASES: &[(&str, &str, &str)] = &[
    ("<svg><g></|", "g", "<svg><g></g>"),
    ("<svg><g></|>", "g", "<svg><g></g>"),
    (
        "<svg><linearGradient></lin|>",
        "linearGradient",
        "<svg><linearGradient></linearGradient>",
    ),
    ("<svg></|", "svg", "<svg></svg>"),
    ("<svg><g></g|></svg>", "g", "<svg><g></g></svg>"),
    ("<svg><g></|g></svg>", "g", "<svg><g></g></svg>"),
    (
        "<svg><linearGradient></li|nearGradient></svg>",
        "linearGradient",
        "<svg><linearGradient></linearGradient></svg>",
    ),
    ("<svg><g></wrong|>", "g", "<svg><g></g>"),
    ("<svg><g></|   >", "g", "<svg><g></g   >"),
    ("<svg><g></|\r\n>", "g", "<svg><g></g\r\n>"),
    ("<svg><g></|\n<rect/>", "g", "<svg><g></g>\n<rect/>"),
    ("<svg><g><g></g></|", "g", "<svg><g><g></g></g>"),
    ("<svg><g><g></|", "g", "<svg><g><g></g>"),
    (
        "<svg><g><rect/><g/><circle/></|",
        "g",
        "<svg><g><rect/><g/><circle/></g>",
    ),
    ("<svg><g><rect></g></|", "svg", "<svg><g><rect></g></svg>"),
    ("<svg><g></bogus></|", "g", "<svg><g></bogus></g>"),
    ("<svg><path></|", "path", "<svg><path></path>"),
    (
        "<svg><animateMotion></|",
        "animateMotion",
        "<svg><animateMotion></animateMotion>",
    ),
    ("<svg>é😀<g></|", "g", "<svg>é😀<g></g>"),
    ("<svg><étoile></é|>", "étoile", "<svg><étoile></étoile>"),
    ("<svg>\r\n  é😀<g></g|", "g", "<svg>\r\n  é😀<g></g>"),
    (
        "<svg><g><!-- <rect> --><![CDATA[<circle>]]></|",
        "g",
        "<svg><g><!-- <rect> --><![CDATA[<circle>]]></g>",
    ),
    (
        "<svg><g><script>let x = '<rect>';</script></|",
        "g",
        "<svg><g><script>let x = '<rect>';</script></g>",
    ),
    (
        "<svg><style></sty|le></svg>",
        "style",
        "<svg><style></style></svg>",
    ),
    (
        "<svg><script></scr|ipt></svg>",
        "script",
        "<svg><script></script></svg>",
    ),
    (
        "<svg><foreignObject><div/></|",
        "foreignObject",
        "<svg><foreignObject><div/></foreignObject>",
    ),
    (
        r#"<svg xmlns:s="http://www.w3.org/2000/svg"><s:g></s:|>"#,
        "s:g",
        r#"<svg xmlns:s="http://www.w3.org/2000/svg"><s:g></s:g>"#,
    ),
    (
        r#"<s:svg xmlns:s="http://www.w3.org/2000/svg"><s:g></|"#,
        "s:g",
        r#"<s:svg xmlns:s="http://www.w3.org/2000/svg"><s:g></s:g>"#,
    ),
    (
        r#"<svg><g xmlns:s="http://www.w3.org/2000/svg"><s:rect></|"#,
        "s:rect",
        r#"<svg><g xmlns:s="http://www.w3.org/2000/svg"><s:rect></s:rect>"#,
    ),
    (
        r#"<svg><foreignObject><svg xmlns="http://www.w3.org/2000/svg"><g></|"#,
        "g",
        r#"<svg><foreignObject><svg xmlns="http://www.w3.org/2000/svg"><g></g>"#,
    ),
    (
        r#"<svg><g xmlns="urn:foreign"></g><g></|"#,
        "g",
        r#"<svg><g xmlns="urn:foreign"></g><g></g>"#,
    ),
];

#[test]
fn closing_tag_edit_finishes_the_matching_element() -> TestResult {
    let mut server = TestServer::start_with_initialize_options(&json!({
        "svg": { "runtime_compat": false },
    }))?;
    for &(marked, name, expected) in MATCHING_TAG_CASES {
        let (source, position, result) = complete(&mut server, marked)?;
        let items = result
            .as_array()
            .ok_or_else(|| format!("{marked}: {result}"))?;
        assert_eq!(items.len(), 1, "{marked}: {items:?}");
        let item = &items[0];
        assert_eq!(item["label"], name, "{marked}");
        let edit: TextEdit = serde_json::from_value(item["textEdit"].clone())?;
        assert_eq!(
            edit.range.start.line, edit.range.end.line,
            "completion edits must be single-line"
        );
        assert!(
            edit.range.start <= position && position <= edit.range.end,
            "the edit must contain the cursor"
        );
        let mut actual = source.clone();
        actual.replace_range(
            offset_at(&source, edit.range.start)?..offset_at(&source, edit.range.end)?,
            &edit.new_text,
        );
        assert_eq!(actual, expected, "{marked}: {item}");
    }
    server.shutdown_and_exit()
}

#[test]
fn closing_tags_do_not_leak_into_excluded_contexts() -> TestResult {
    let mut server = TestServer::start_with_initialize_options(&json!({
        "svg": { "runtime_compat": false },
    }))?;
    for marked in [
        "</|",
        "<svg/></|",
        "<svg><g></g></svg></|",
        "<svg><!-- </| -->",
        "<svg><!-- </|",
        "<svg><![CDATA[</|]]>",
        "<svg><![CDATA[</|",
        "<?xml-stylesheet data='</|'?><svg/>",
        "<!DOCTYPE svg [<!ENTITY x '</|'>]><svg/>",
        "<svg><script>const x = '</|';</script></svg>",
        "<svg><script>const x = '</|';",
        "<svg><style>.a::before { content: '</|'; }</style></svg>",
        "<svg><script></|",
        "<svg><style></|",
        r#"<svg><g data-note="</|"/></svg>"#,
        r#"<svg><g data-note="</|"#,
        "<svg><g fill='</|' />",
        "<svg><g fill='</|",
        "<svg><g fill=</|",
        "<svg><g fill=red </|",
        "<svg><g style='content: </|'/>",
        "<svg><g onclick='x = </|'/>",
        "<svg><foreignObject><div></|",
        "<svg><metadata><record></|",
        r#"<svg><g xmlns="urn:foreign"></|"#,
        r#"<svg xmlns:f="urn:foreign"><f:g></|"#,
        "<svg><unbound:g></|",
        r#"<svg xmlns:s="http://www.w3.org/2000/svg"><g xmlns:s="urn:foreign"><s:g></|"#,
        r#"<svg xmlns=""><g></|"#,
    ] {
        let (_, _, result) = complete(&mut server, marked)?;
        assert!(
            result.is_null() || result.as_array().is_some_and(Vec::is_empty),
            "{marked}: {result}"
        );
    }
    server.shutdown_and_exit()
}

#[test]
fn slash_triggers_completion_and_document_changes_update_the_match() -> TestResult {
    let mut server = TestServer::start()?;
    let triggers = server.init_response["result"]["capabilities"]["completionProvider"]
        ["triggerCharacters"]
        .as_array()
        .ok_or("triggers")?;
    assert!(triggers.iter().any(|value| value == "/"), "{triggers:?}");
    server.open("file:///changed-closing.svg", "<svg><g></")?;
    let initial = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///changed-closing.svg" },
            "position": position_at("<svg><g></", "<svg><g></".len())?,
        }),
    )?;
    assert_eq!(initial["result"][0]["label"], "g");
    let changed = "<svg><defs></";
    server.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": "file:///changed-closing.svg", "version": 2 },
            "contentChanges": [{ "text": changed }],
        }),
    )?;
    let response = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///changed-closing.svg" },
            "position": position_at(changed, changed.len())?,
            "context": { "triggerKind": 2, "triggerCharacter": "/" },
        }),
    )?;
    assert_eq!(response["result"][0]["label"], "defs", "{response}");
    assert_eq!(
        response["result"][0]["textEdit"]["newText"], "defs>",
        "{response}"
    );
    server.shutdown_and_exit()
}

#[test]
fn closing_an_existing_element_is_not_filtered_by_the_profile() -> TestResult {
    let mut server = TestServer::start_with_initialize_options(&json!({
        "svg": { "profile": "svg2draft", "force_profile": true, "runtime_compat": false },
    }))?;
    let (_, _, result) = complete(&mut server, "<svg><font></|")?;
    assert_eq!(result[0]["label"], "font", "{result}");
    let (_, _, children) = complete(&mut server, "<svg><g></g> |</svg>")?;
    let items = children.as_array().ok_or("child completions")?;
    let labels: Vec<_> = items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(labels.contains(&"circle"), "{labels:?}");
    assert!(!labels.contains(&"font"), "{labels:?}");
    server.shutdown_and_exit()
}
