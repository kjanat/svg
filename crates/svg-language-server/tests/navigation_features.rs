//! Integration tests for symbols, references, rename, links, folding, selection, semantic
//! tokens and inlay hints.

mod support;

use serde_json::{Value, json};
use support::TestServer;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const URI: &str = "file:///project/icons/nav.svg";

const SOURCE: &str = concat!(
    "<svg xmlns=\"http://www.w3.org/2000/svg\">\n",
    "  <defs>\n",
    "    <linearGradient id=\"grad\"><stop offset=\"0\"/></linearGradient>\n",
    "  </defs>\n",
    "  <style>.hot { fill: url(#grad); }</style>\n",
    "  <rect class=\"hot\" fill=\"url(#grad)\"/>\n",
    "  <use href=\"#grad\"/>\n",
    "  <image href=\"img/a.png\"/>\n",
    "</svg>\n",
);

fn position_of(source: &str, needle: &str, nth: usize, shift: usize) -> TestResult<Value> {
    let offset = source
        .match_indices(needle)
        .nth(nth)
        .map(|(offset, _)| offset + shift)
        .ok_or_else(|| format!("occurrence {nth} of {needle:?} missing"))?;
    let before = &source[..offset];
    let line = before.matches('\n').count();
    let character = before
        .rfind('\n')
        .map_or(offset, |newline| offset - newline - 1);
    Ok(json!({ "line": line, "character": character }))
}

fn range_of(source: &str, needle: &str, nth: usize) -> TestResult<Value> {
    let start = position_of(source, needle, nth, 0)?;
    let end = position_of(source, needle, nth, needle.len())?;
    Ok(json!({ "start": start, "end": end }))
}

fn open_fixture() -> TestResult<TestServer> {
    let mut server = TestServer::start()?;
    server.open(URI, SOURCE)?;
    Ok(server)
}

fn text_document_position(position: &Value) -> Value {
    json!({ "textDocument": { "uri": URI }, "position": position })
}

fn sorted_ranges(value: &Value) -> TestResult<Vec<Value>> {
    let mut ranges: Vec<Value> = value
        .as_array()
        .ok_or("expected an array")?
        .iter()
        .map(|item| item.get("range").cloned().unwrap_or_else(|| item.clone()))
        .collect();
    ranges.sort_by_key(|range| {
        (
            range["start"]["line"].as_u64(),
            range["start"]["character"].as_u64(),
        )
    });
    Ok(ranges)
}

#[test]
fn advertises_navigation_capabilities() -> TestResult {
    let mut server = TestServer::start()?;
    let capabilities = &server.init_response["result"]["capabilities"];
    assert_eq!(capabilities["documentSymbolProvider"], json!(true));
    assert_eq!(capabilities["referencesProvider"], json!(true));
    assert_eq!(
        capabilities["renameProvider"]["prepareProvider"],
        json!(true)
    );
    assert!(capabilities["documentLinkProvider"].is_object());
    assert_eq!(capabilities["foldingRangeProvider"], json!(true));
    assert_eq!(capabilities["selectionRangeProvider"], json!(true));
    assert_eq!(capabilities["inlayHintProvider"], json!(true));
    let semantic = &capabilities["semanticTokensProvider"];
    assert_eq!(semantic["full"], json!(true));
    let types = semantic["legend"]["tokenTypes"]
        .as_array()
        .ok_or("token types")?;
    for expected in ["type", "property", "string", "variable", "class", "comment"] {
        assert!(
            types.contains(&json!(expected)),
            "{expected} missing: {types:?}"
        );
    }
    assert_eq!(semantic["legend"]["tokenModifiers"], json!(["declaration"]));
    server.shutdown_and_exit()
}

#[test]
fn document_symbols_nest_elements() -> TestResult {
    let mut server = open_fixture()?;
    let response = server.request(
        "textDocument/documentSymbol",
        &json!({ "textDocument": { "uri": URI } }),
    )?;
    let roots = response["result"].as_array().ok_or("symbol array")?;
    assert_eq!(roots.len(), 1, "{response}");
    let root = &roots[0];
    assert_eq!(root["name"], "svg");
    assert_eq!(root["selectionRange"], range_of(SOURCE, "svg", 0)?);
    let children: Vec<_> = root["children"]
        .as_array()
        .ok_or("svg children")?
        .iter()
        .map(|child| child["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(children, vec!["defs", "style", "rect", "use", "image"]);
    let gradient = &root["children"][0]["children"][0];
    assert_eq!(gradient["name"], "linearGradient");
    assert_eq!(gradient["detail"], "#grad");
    assert_eq!(gradient["children"][0]["name"], "stop");
    assert_eq!(root["children"][2]["detail"], ".hot");
    assert!(root["children"][1].get("children").is_none());
    server.shutdown_and_exit()
}

#[test]
fn document_symbols_are_flat_without_hierarchical_support() -> TestResult {
    let mut server = TestServer::start_with_capabilities(&json!({}))?;
    server.open(URI, SOURCE)?;
    let response = server.request(
        "textDocument/documentSymbol",
        &json!({ "textDocument": { "uri": URI } }),
    )?;
    let symbols = response["result"].as_array().ok_or("symbol array")?;
    let rows: Vec<_> = symbols
        .iter()
        .map(|symbol| {
            (
                symbol["name"].as_str().unwrap_or_default(),
                symbol["containerName"].as_str(),
                symbol["location"]["uri"].as_str(),
            )
        })
        .collect();
    assert_eq!(rows[0], ("svg", None, Some(URI)), "{response}");
    assert_eq!(rows[2], ("linearGradient", Some("defs"), Some(URI)));
    assert_eq!(rows.len(), 8, "{response}");
    assert!(
        symbols
            .iter()
            .all(|symbol| symbol.get("children").is_none())
    );
    server.shutdown_and_exit()
}

#[test]
fn references_follow_include_declaration() -> TestResult {
    let mut server = open_fixture()?;
    let at_reference = position_of(SOURCE, "#grad", 2, 2)?;
    let mut params = text_document_position(&at_reference);
    params["context"] = json!({ "includeDeclaration": true });
    let with_declaration = server.request("textDocument/references", &params)?;
    let ranges = sorted_ranges(&with_declaration["result"])?;
    let expected: Vec<Value> = (0..4)
        .map(|nth| range_of(SOURCE, "grad", nth))
        .collect::<TestResult<_>>()?;
    assert_eq!(ranges, expected, "{with_declaration}");
    assert!(
        with_declaration["result"]
            .as_array()
            .ok_or("locations")?
            .iter()
            .all(|location| location["uri"] == URI)
    );

    params["context"] = json!({ "includeDeclaration": false });
    let without_declaration = server.request("textDocument/references", &params)?;
    assert_eq!(
        sorted_ranges(&without_declaration["result"])?,
        expected[1..].to_vec()
    );

    let class_params = json!({
        "textDocument": { "uri": URI },
        "position": position_of(SOURCE, "\"hot\"", 0, 2)?,
        "context": { "includeDeclaration": true }
    });
    let classes = server.request("textDocument/references", &class_params)?;
    assert_eq!(
        sorted_ranges(&classes["result"])?,
        vec![range_of(SOURCE, "hot", 0)?, range_of(SOURCE, "hot", 1)?]
    );
    server.shutdown_and_exit()
}

#[test]
fn prepare_rename_and_rename_cover_every_occurrence() -> TestResult {
    let mut server = open_fixture()?;
    let at_href = position_of(SOURCE, "#grad", 2, 3)?;

    let prepared = server.request(
        "textDocument/prepareRename",
        &text_document_position(&at_href),
    )?;
    assert_eq!(
        prepared["result"],
        range_of(SOURCE, "grad", 3)?,
        "{prepared}"
    );

    let not_renameable = server.request(
        "textDocument/prepareRename",
        &text_document_position(&position_of(SOURCE, "rect", 0, 1)?),
    )?;
    assert!(not_renameable["result"].is_null(), "{not_renameable}");

    let mut params = text_document_position(&at_href);
    params["newName"] = json!("sky");
    let renamed = server.request("textDocument/rename", &params)?;
    let edits = sorted_ranges(&renamed["result"]["changes"][URI])?;
    let expected: Vec<Value> = (0..4)
        .map(|nth| range_of(SOURCE, "grad", nth))
        .collect::<TestResult<_>>()?;
    assert_eq!(edits, expected, "{renamed}");
    assert!(
        renamed["result"]["changes"][URI]
            .as_array()
            .ok_or("edits")?
            .iter()
            .all(|edit| edit["newText"] == "sky")
    );

    params["newName"] = json!("1sky");
    let rejected = server.request("textDocument/rename", &params)?;
    let message = rejected["error"]["message"]
        .as_str()
        .ok_or("rename error")?;
    assert!(message.contains("XML name"), "{rejected}");
    server.shutdown_and_exit()
}

#[test]
fn escaped_names_and_presentation_vars_share_one_symbol() -> TestResult {
    let uri = "file:///project/escaped.svg";
    let source = concat!(
        "<svg>\n",
        "  <style>.a\\&b { --c: red }</style>\n",
        "  <g class=\"a&amp;b\"><rect fill=\"var(--c)\" opacity=\"var(--c, 1)\"/></g>\n",
        "</svg>\n",
    );
    let mut server = TestServer::start()?;
    server.open(uri, source)?;
    let at = |needle: &str, shift: usize| -> TestResult<Value> {
        Ok(
            json!({ "textDocument": { "uri": uri }, "position": position_of(source, needle, 0, shift)? }),
        )
    };

    let definition = server.request("textDocument/definition", &at("a&amp;b", 1)?)?;
    assert_eq!(
        definition["result"]["range"],
        range_of(source, "a\\&b", 0)?,
        "{definition}"
    );

    let mut params = at("var(--c)", 5)?;
    params["context"] = json!({ "includeDeclaration": true });
    let references = server.request("textDocument/references", &params)?;
    let expected: Vec<Value> = (0..3)
        .map(|nth| range_of(source, "--c", nth))
        .collect::<TestResult<_>>()?;
    assert_eq!(
        sorted_ranges(&references["result"])?,
        expected,
        "{references}"
    );

    let mut params = at("a&amp;b", 0)?;
    params["newName"] = json!("x&y");
    let renamed = server.request("textDocument/rename", &params)?;
    let edits: Vec<_> = renamed["result"]["changes"][uri]
        .as_array()
        .ok_or("edits")?
        .iter()
        .map(|edit| edit["newText"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(edits, vec!["x\\&amp;y", "x&amp;y"], "{renamed}");
    server.shutdown_and_exit()
}

#[test]
fn document_links_resolve_against_document_uri() -> TestResult {
    let mut server = open_fixture()?;
    let response = server.request(
        "textDocument/documentLink",
        &json!({ "textDocument": { "uri": URI } }),
    )?;
    let links = response["result"].as_array().ok_or("link array")?;
    assert_eq!(links.len(), 1, "{response}");
    assert_eq!(links[0]["target"], "file:///project/icons/img/a.png");
    assert_eq!(links[0]["range"], range_of(SOURCE, "img/a.png", 0)?);
    server.shutdown_and_exit()
}

#[test]
fn folding_ranges_respect_line_folding_only() -> TestResult {
    let mut server = TestServer::start_with_capabilities(&json!({
        "textDocument": { "foldingRange": { "lineFoldingOnly": true } }
    }))?;
    server.open(URI, SOURCE)?;
    let response = server.request(
        "textDocument/foldingRange",
        &json!({ "textDocument": { "uri": URI } }),
    )?;
    let mut ranges: Vec<_> = response["result"]
        .as_array()
        .ok_or("folding array")?
        .iter()
        .map(|range| {
            (
                range["startLine"].as_u64(),
                range["endLine"].as_u64(),
                range.get("startCharacter").is_some(),
                range["kind"].as_str().map(str::to_owned),
            )
        })
        .collect();
    ranges.sort();
    assert_eq!(
        ranges,
        vec![
            (Some(0), Some(7), false, Some("region".to_owned())),
            (Some(1), Some(2), false, Some("region".to_owned())),
        ],
        "{response}"
    );
    server.shutdown_and_exit()
}

#[test]
fn selection_ranges_expand_outward() -> TestResult {
    let mut server = open_fixture()?;
    let response = server.request(
        "textDocument/selectionRange",
        &json!({
            "textDocument": { "uri": URI },
            "positions": [position_of(SOURCE, "\"hot\"", 0, 2)?]
        }),
    )?;
    let selections = response["result"].as_array().ok_or("selection array")?;
    assert_eq!(selections.len(), 1, "{response}");
    let mut chain = Vec::new();
    let mut current = Some(&selections[0]);
    while let Some(selection) = current.filter(|selection| !selection.is_null()) {
        chain.push(selection["range"].clone());
        current = selection.get("parent");
    }
    assert_eq!(chain[0], range_of(SOURCE, "hot", 1)?);
    assert!(chain.contains(&range_of(SOURCE, "class=\"hot\"", 0)?));
    assert_eq!(
        chain.last(),
        Some(&json!({
            "start": { "line": 0, "character": 0 },
            "end": { "line": 9, "character": 0 }
        }))
    );
    server.shutdown_and_exit()
}

#[test]
fn semantic_tokens_mark_id_declaration() -> TestResult {
    let mut server = open_fixture()?;
    let legend =
        server.init_response["result"]["capabilities"]["semanticTokensProvider"]["legend"].clone();
    let variable = legend["tokenTypes"]
        .as_array()
        .ok_or("token types")?
        .iter()
        .position(|kind| kind == "variable")
        .ok_or("variable type")?;
    let response = server.request(
        "textDocument/semanticTokens/full",
        &json!({ "textDocument": { "uri": URI } }),
    )?;
    let data: Vec<u64> = response["result"]["data"]
        .as_array()
        .ok_or("token data")?
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(data.len() % 5, 0, "{response}");
    let mut line = 0;
    let mut character = 0;
    let mut decoded = Vec::new();
    for [delta_line, delta_start, length, kind, modifiers] in data.as_chunks::<5>().0 {
        if *delta_line > 0 {
            line += delta_line;
            character = *delta_start;
        } else {
            character += delta_start;
        }
        decoded.push((line, character, *length, *kind, *modifiers));
    }
    let declaration = position_of(SOURCE, "grad", 0, 0)?;
    let variable = u64::try_from(variable)?;
    assert!(
        decoded.contains(&(
            declaration["line"].as_u64().ok_or("line")?,
            declaration["character"].as_u64().ok_or("character")?,
            4,
            variable,
            1
        )),
        "{decoded:?}"
    );
    let reference = position_of(SOURCE, "grad", 3, 0)?;
    assert!(decoded.contains(&(
        reference["line"].as_u64().ok_or("line")?,
        reference["character"].as_u64().ok_or("character")?,
        4,
        variable,
        0
    )));
    server.shutdown_and_exit()
}

#[test]
fn inlay_hints_name_referenced_element() -> TestResult {
    let mut server = open_fixture()?;
    let response = server.request(
        "textDocument/inlayHint",
        &json!({
            "textDocument": { "uri": URI },
            "range": {
                "start": { "line": 5, "character": 0 },
                "end": { "line": 7, "character": 0 }
            }
        }),
    )?;
    let hints = response["result"].as_array().ok_or("hint array")?;
    let summary: Vec<_> = hints
        .iter()
        .map(|hint| (hint["position"].clone(), hint["label"].clone()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                position_of(SOURCE, "grad", 2, 4)?,
                json!(": linearGradient")
            ),
            (
                position_of(SOURCE, "grad", 3, 4)?,
                json!(": linearGradient")
            ),
        ],
        "{response}"
    );
    server.shutdown_and_exit()
}
