//! Specification lifecycle is consistent across profile-aware editor features.

mod support;

use serde_json::json;
use support::TestServer;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn attribute_hover_rejects_foreign_owners() -> TestResult {
    let mut server = TestServer::start_with_initialize_options(&json!({"svg": {
        "profile": "Svg2EditorsDraft", "runtime_compat": false,
    }}))?;
    for (index, (source, attribute)) in [
        (
            r#"<svg xmlns:h="http://www.w3.org/1999/xhtml"><h:style type="text/css"/></svg>"#,
            "type",
        ),
        (
            r#"<svg><style xmlns="http://www.w3.org/1999/xhtml" type="text/css"/></svg>"#,
            "type",
        ),
        (
            r#"<svg><foreignObject><style type="text/css"/></foreignObject></svg>"#,
            "type",
        ),
        (
            r#"<svg xmlns:h="http://www.w3.org/1999/xhtml"><h:rect width="1"/></svg>"#,
            "width",
        ),
        (r#"<svg><rect xmlns="" width="1"/></svg>"#, "width"),
    ]
    .into_iter()
    .enumerate()
    {
        let uri = format!("file:///foreign-attribute-{index}.svg");
        server.open(&uri, source)?;
        let response = server.request("textDocument/hover", &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": source.find(&format!("{attribute}=")).ok_or("attribute")? + 1 },
        }))?;
        assert!(
            response["result"].is_null(),
            "foreign owner must not receive SVG attribute metadata: {source}: {response}"
        );
    }
    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn spec_lifecycle_hover_and_completion_follow_the_selected_edition() -> TestResult {
    for (profile, svg2) in [
        ("Svg11Rec20030114", false),
        ("Svg11Rec20110816", false),
        ("Svg2Cr20181004", true),
        ("Svg2EditorsDraft", true),
    ] {
        let mut server = TestServer::start_with_initialize_options(&json!({"svg": {
            "profile": profile, "force_profile": true, "runtime_compat": false,
        }}))?;
        let uri = "file:///spec-lifecycle.svg";
        let source = r#"<svg><text glyph-orientation-vertical="0">text</text></svg>"#;
        server.open(uri, source)?;
        let response = server.request("textDocument/hover", &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": source.find("glyph").ok_or("attribute")? + 3 },
        }))?;
        let hover = response["result"]["contents"]["value"]
            .as_str()
            .ok_or("hover")?;
        assert_eq!(
            hover.contains("**SVG specification:** Obsoleted"),
            svg2,
            "{profile}: {hover}"
        );
        if svg2 {
            assert!(
                hover.contains("#GlyphOrientationVerticalProperty"),
                "{hover}"
            );
        }

        let uri = "file:///spec-completion.svg";
        let source = "<svg><text  /></svg>";
        server.open(uri, source)?;
        let response = server.request("textDocument/completion", &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": source.find("  ").ok_or("attribute space")? + 1 },
        }))?;
        let items = response["result"].as_array().ok_or("completion items")?;
        let vertical = items
            .iter()
            .find(|i| i["label"] == "glyph-orientation-vertical")
            .ok_or("retained attribute completion")?;
        assert_eq!(
            vertical["deprecated"].as_bool().unwrap_or(false),
            svg2,
            "{profile}: {vertical}"
        );
        assert_eq!(
            items
                .iter()
                .any(|i| i["label"] == "glyph-orientation-horizontal"),
            !svg2,
            "removed attribute must follow {profile}"
        );

        check_scoped_attributes(&mut server, profile)?;
        server.shutdown_and_exit()?;
    }
    Ok(())
}

fn check_scoped_attributes(server: &mut TestServer, profile: &str) -> TestResult {
    for source in [
        r#"<svg><style type="text/css"></style><animateTransform type="rotate"/></svg>"#,
        r#"<svg xmlns:s="http://www.w3.org/2000/svg"><s:style type="text/css"></s:style><s:animateTransform type="rotate"/></svg>"#,
    ] {
        let uri = "file:///scoped-lifecycle.svg";
        server.open(uri, source)?;
        for (offset, obsolete) in [
            (
                source.find("type=").ok_or("style type")?,
                profile == "Svg2EditorsDraft",
            ),
            (source.rfind("type=").ok_or("animation type")?, false),
        ] {
            let response = server.request("textDocument/hover", &json!({
                "textDocument": { "uri": uri }, "position": { "line": 0, "character": offset + 1 },
            }))?;
            let hover = response["result"]["contents"]["value"]
                .as_str()
                .ok_or("type hover")?;
            assert_eq!(
                hover.contains("**SVG specification:** Obsoleted"),
                obsolete,
                "{profile}: {hover}"
            );
        }
    }
    for (element, obsolete) in [
        ("style", profile == "Svg2EditorsDraft"),
        ("animateTransform", false),
    ] {
        let source = format!(r#"<svg xmlns:s="http://www.w3.org/2000/svg"><s:{element}  /></svg>"#);
        let uri = "file:///scoped-completion.svg";
        server.open(uri, &source)?;
        let response = server.request("textDocument/completion", &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": source.find("  ").ok_or("attribute space")? + 1 },
        }))?;
        let attribute = response["result"]
            .as_array()
            .ok_or("scoped completion items")?
            .iter()
            .find(|item| item["label"] == "type")
            .ok_or("type completion")?;
        assert_eq!(
            attribute["deprecated"].as_bool().unwrap_or(false),
            obsolete,
            "{profile}/{element}: {attribute}"
        );
    }
    Ok(())
}
