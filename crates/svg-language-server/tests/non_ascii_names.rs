//! Goto-definition tests for non-ASCII `id` and `class` names.

mod support;

use serde_json::json;
use support::TestServer;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn utf16_column(text: &str, byte_offset: usize) -> Result<u64, Box<dyn std::error::Error>> {
    let prefix = text
        .get(..byte_offset)
        .ok_or("byte offset on a char boundary")?;
    Ok(u64::try_from(prefix.encode_utf16().count())?)
}

fn offline_server() -> Result<TestServer, Box<dyn std::error::Error>> {
    TestServer::start_with_initialize_options(&json!({ "svg": { "runtime_compat": false } }))
}

fn assert_definition(
    server: &mut TestServer,
    uri: &str,
    text: &str,
    reference_byte: usize,
    definition_byte: usize,
    name: &str,
) -> TestResult {
    let response = server.request(
        "textDocument/definition",
        &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": utf16_column(text, reference_byte)? }
        }),
    )?;
    let start = utf16_column(text, definition_byte)?;
    let end = utf16_column(text, definition_byte + name.len())?;
    let definition = &response["result"];
    assert_eq!(definition["uri"].as_str(), Some(uri), "{name}: {response}");
    assert_eq!(
        definition["range"]["start"]["character"].as_u64(),
        Some(start),
        "{name}: {response}"
    );
    assert_eq!(
        definition["range"]["end"]["character"].as_u64(),
        Some(end),
        "{name}: {response}"
    );
    Ok(())
}

#[test]
fn non_ascii_id_definitions_resolve_from_href() -> TestResult {
    let mut server = offline_server()?;
    for (index, id) in ["😀", "𠀀", "é", "中"].into_iter().enumerate() {
        let uri = format!("file:///non-ascii-id-{index}.svg");
        let text = format!(r##"<svg><g id="{id}"/><use href="#{id}"/></svg>"##);
        server.open(&uri, &text)?;
        let definition_byte = text.find(r#"id=""#).ok_or("id attribute")? + 4;
        let reference_byte = text.find(r##"href="#"##).ok_or("href attribute")? + 7;
        assert_definition(
            &mut server,
            &uri,
            &text,
            reference_byte,
            definition_byte,
            id,
        )?;
    }
    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn non_ascii_class_definition_resolves_to_stylesheet() -> TestResult {
    let mut server = offline_server()?;
    let uri = "file:///non-ascii-class.svg";
    let css_body = "{fill:red}";
    let text = format!(r#"<svg><style>.é{css_body}</style><rect class="md:flex é"/></svg>"#);
    server.open(uri, &text)?;
    let definition_byte = text.find(".é").ok_or("class selector")? + 1;
    let reference_byte = text.rfind('é').ok_or("class reference")?;
    assert_definition(
        &mut server,
        uri,
        &text,
        reference_byte,
        definition_byte,
        "é",
    )?;
    server.shutdown_and_exit()?;
    Ok(())
}
