//! Protocol lifecycle, transport robustness and editor-option handling.

use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, channel},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const TIMEOUT: Duration = Duration::from_secs(20);
const BINARY: &str = env!("CARGO_BIN_EXE_svg-language-server");

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Value>,
    stderr: Option<JoinHandle<Vec<u8>>>,
    reader: Option<JoinHandle<()>>,
    next_id: u64,
    pending: Vec<Value>,
}

fn frame(body: &[u8]) -> Vec<u8> {
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend_from_slice(body);
    framed
}

fn read_frame(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            length = value.parse().ok();
        }
    }
    let mut body = vec![0; length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

impl Client {
    fn spawn(args: &[&str]) -> TestResult<Self> {
        let mut child = Command::new(BINARY)
            .args(args)
            .env(
                "SVG_LS_LOG_DIR",
                std::env::temp_dir().join("svg-ls-protocol-tests"),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().ok_or("stdin")?;
        let stdout = child.stdout.take().ok_or("stdout")?;
        let mut stderr = child.stderr.take().ok_or("stderr")?;
        let (tx, messages) = channel();
        let reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Some(message) = read_frame(&mut reader) {
                if tx.send(message).is_err() {
                    return;
                }
            }
        });
        let stderr = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.read_to_end(&mut bytes);
            bytes
        });
        Ok(Self {
            child,
            stdin: Some(stdin),
            messages,
            stderr: Some(stderr),
            reader: Some(reader),
            next_id: 1,
            pending: Vec::new(),
        })
    }

    fn start(capabilities: &Value, options: &Value) -> TestResult<(Self, Value)> {
        let mut client = Self::spawn(&[])?;
        let init = client.request(
            "initialize",
            &json!({ "processId": null, "rootUri": null, "capabilities": capabilities, "initializationOptions": options }),
        )?;
        client.notify("initialized", &json!({}))?;
        Ok((client, init))
    }

    fn offline() -> TestResult<Self> {
        Ok(Self::start(&json!({}), &json!({ "svg": { "runtime_compat": false } }))?.0)
    }

    fn send_raw(&mut self, bytes: &[u8]) -> TestResult {
        let stdin = self.stdin.as_mut().ok_or("stdin closed")?;
        stdin.write_all(bytes)?;
        stdin.flush()?;
        Ok(())
    }

    fn send(&mut self, message: &Value) -> TestResult {
        self.send_raw(&frame(serde_json::to_string(message)?.as_bytes()))
    }

    fn notify(&mut self, method: &str, params: &Value) -> TestResult {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    fn request(&mut self, method: &str, params: &Value) -> TestResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        self.wait_for(|message| message["id"] == json!(id) && message.get("method").is_none())
    }

    fn wait_for(&mut self, matches: impl Fn(&Value) -> bool) -> TestResult<Value> {
        if let Some(index) = self.pending.iter().position(&matches) {
            return Ok(self.pending.remove(index));
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.messages.recv_timeout(remaining) {
                Ok(message) if matches(&message) => return Ok(message),
                Ok(message) => self.pending.push(message),
                Err(RecvTimeoutError::Timeout) => {
                    return Err("timed out waiting for a message".into());
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("server closed its output".into());
                }
            }
        }
    }

    fn open(&mut self, uri: &str, text: &str) -> TestResult {
        self.notify(
            "textDocument/didOpen",
            &json!({ "textDocument": { "uri": uri, "languageId": "svg", "version": 1, "text": text } }),
        )
    }

    fn wait_exit(&mut self) -> TestResult<(ExitStatus, Duration)> {
        let started = Instant::now();
        let deadline = started + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok((status, started.elapsed()));
            }
            if Instant::now() > deadline {
                self.child.kill()?;
                return Err("server did not exit".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn stderr(&mut self) -> Vec<u8> {
        self.stderr
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    }

    fn shutdown_and_exit(&mut self) -> TestResult<ExitStatus> {
        let response = self.request("shutdown", &Value::Null)?;
        assert!(response.get("error").is_none(), "{response}");
        self.notify("exit", &Value::Null)?;
        Ok(self.wait_exit()?.0)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn initialize_reports_the_server_name_and_version() -> TestResult {
    let mut client = Client::offline()?;
    let (_, init) = Client::start(&json!({}), &json!({ "svg": { "runtime_compat": false } }))?;
    assert_eq!(
        init["result"]["serverInfo"],
        json!({ "name": "svg-language-server", "version": env!("CARGO_PKG_VERSION") })
    );
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn malformed_messages_are_answered_and_the_server_keeps_running() -> TestResult {
    let mut client = Client::offline()?;
    client.send_raw(&frame(br#"{"jsonrpc":"2.0","method":"#))?;
    let body = br#"{"jsonrpc":"2.0","method":"$/ignored"}"#;
    client.send_raw(
        &[
            format!("Content-Lenght: {}\r\n\r\n", body.len()).as_bytes(),
            body,
        ]
        .concat(),
    )?;
    client.send_raw(&frame(b"[]"))?;
    client.send_raw(&frame(br#"{"jsonrpc":"2.0","id":41,"nonsense":true}"#))?;
    let parse = |code: i64| move |message: &Value| message["error"]["code"] == json!(code);
    let first = client.wait_for(parse(-32700))?;
    assert_eq!(first["id"], Value::Null);
    client.wait_for(parse(-32700))?;
    let invalid = client
        .wait_for(|message| message["error"]["code"] == json!(-32600) && message["id"].is_null())?;
    assert_eq!(invalid["id"], Value::Null);
    let identified = client.wait_for(|message| message["id"] == json!(41))?;
    assert_eq!(identified["error"]["code"], json!(-32600));

    client.open("file:///alive.svg", "<svg><rect/></svg>")?;
    let hover = client.request(
        "textDocument/hover",
        &json!({ "textDocument": { "uri": "file:///alive.svg" }, "position": { "line": 0, "character": 7 } }),
    )?;
    assert!(hover["result"]["contents"].is_object(), "{hover}");
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn many_and_large_messages_pass_through_intact() -> TestResult {
    let mut client = Client::offline()?;
    let uri = "file:///large.svg";
    let rects = (0..40_000).fold(String::new(), |mut rects, index| {
        rects.push_str("<rect id=\"r");
        rects.push_str(&index.to_string());
        rects.push_str("\" width=\"1\"/>\n");
        rects
    });
    let source = format!("<svg>\n{rects}</svg>\n");
    assert!(source.len() > 1_000_000);
    client.open(uri, &source)?;
    for version in 2..=200 {
        let text = format!("<svg><circle r=\"{version}\"/></svg>");
        client.notify(
            "textDocument/didChange",
            &json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }),
        )?;
    }
    let symbols = client.request(
        "textDocument/formatting",
        &json!({ "textDocument": { "uri": uri }, "options": { "tabSize": 2, "insertSpaces": true } }),
    )?;
    let edits = symbols["result"].as_array().ok_or("formatting edits")?;
    let formatted: String = edits
        .iter()
        .filter_map(|edit| edit["newText"].as_str())
        .collect();
    assert!(formatted.contains("r=\"200\""), "{symbols}");

    let bytes = frame(
        serde_json::to_string(&json!({
            "jsonrpc": "2.0", "id": 9000, "method": "textDocument/hover",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 0, "character": 7 } },
        }))?
        .as_bytes(),
    );
    for chunk in bytes.chunks(7) {
        client.send_raw(chunk)?;
    }
    let hover = client.wait_for(|message| message["id"] == json!(9000))?;
    assert!(hover["result"]["contents"].is_object(), "{hover}");
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn rapid_incremental_edits_apply_in_order() -> TestResult {
    const EDITS: u64 = 500;
    let mut client = Client::offline()?;
    let uri = "file:///incremental.svg";
    client.open(uri, "<svg>\n</svg>\n")?;
    for line in 1..=EDITS {
        client.notify(
            "textDocument/didChange",
            &json!({
                "textDocument": { "uri": uri, "version": line + 1 },
                "contentChanges": [{
                    "range": { "start": { "line": line, "character": 0 }, "end": { "line": line, "character": 0 } },
                    "text": format!("<g id=\"e{line}\"/>\n"),
                }],
            }),
        )?;
        if line % 10 == 0 {
            client.send(&json!({
                "jsonrpc": "2.0", "id": 10_000 + line, "method": "textDocument/hover",
                "params": { "textDocument": { "uri": uri }, "position": { "line": line, "character": 1 } },
            }))?;
        }
    }
    for line in (10..=EDITS).step_by(10) {
        let hover = client.wait_for(|message| message["id"] == json!(10_000 + line))?;
        assert!(hover["result"]["contents"].is_object(), "{hover}");
    }
    let response = client.request(
        "textDocument/documentSymbol",
        &json!({ "textDocument": { "uri": uri } }),
    )?;
    let children = response["result"][0]["children"]
        .as_array()
        .ok_or("svg children")?;
    let placed: Vec<_> = children
        .iter()
        .map(|child| {
            (
                child["detail"].as_str().unwrap_or_default().to_owned(),
                child["range"]["start"]["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect();
    let expected: Vec<_> = (1..=EDITS)
        .map(|line| (format!("#e{line}"), line))
        .collect();
    assert_eq!(placed, expected);
    assert_eq!(
        response["result"][0]["range"]["end"]["line"],
        json!(EDITS + 1)
    );
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn exit_code_follows_the_shutdown_handshake() -> TestResult {
    let mut client = Client::offline()?;
    client.notify("exit", &Value::Null)?;
    assert_eq!(client.wait_exit()?.0.code(), Some(1));

    let mut client = Client::offline()?;
    drop(client.stdin.take());
    assert_eq!(client.wait_exit()?.0.code(), Some(1));

    let mut client = Client::offline()?;
    assert_eq!(client.shutdown_and_exit()?.code(), Some(0));
    Ok(())
}

#[test]
fn exit_does_not_wait_for_the_compat_refresh() -> TestResult {
    let (mut client, _) = Client::start(&json!({}), &Value::Null)?;
    client.request("shutdown", &Value::Null)?;
    client.notify("exit", &Value::Null)?;
    let (status, elapsed) = client.wait_exit()?;
    assert!(status.success());
    assert!(elapsed < Duration::from_secs(1), "exit took {elapsed:?}");
    Ok(())
}

#[test]
fn command_line_help_and_version_succeed() -> TestResult {
    for flag in ["--help", "-h"] {
        let output = Command::new(BINARY).arg(flag).output()?;
        assert!(output.status.success(), "{flag}");
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("usage: svg-language-server"), "{stdout}");
    }
    let output = Command::new(BINARY).arg("--version").output()?;
    assert_eq!(
        String::from_utf8(output.stdout)?,
        format!("svg-language-server {}\n", env!("CARGO_PKG_VERSION"))
    );
    let output = Command::new(BINARY).arg("--bogus").output()?;
    assert_eq!(output.status.code(), Some(2));
    Ok(())
}

#[test]
fn stderr_logs_have_no_colour_codes_when_not_a_terminal() -> TestResult {
    let mut client = Client::offline()?;
    assert!(client.shutdown_and_exit()?.success());
    let stderr = client.stderr();
    assert_ne!(stderr, Vec::<u8>::new());
    assert!(
        !stderr.contains(&0x1b),
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    Ok(())
}

fn obsoleted_hover(client: &mut Client, uri: &str) -> TestResult<bool> {
    let source = r#"<svg><text glyph-orientation-vertical="0">t</text></svg>"#;
    client.open(uri, source)?;
    let response = client.request(
        "textDocument/hover",
        &json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 14 } }),
    )?;
    let value = response["result"]["contents"]["value"]
        .as_str()
        .ok_or("hover")?;
    Ok(value.contains("Obsoleted"))
}

#[test]
fn an_unrelated_configuration_change_keeps_the_svg_settings() -> TestResult {
    let svg11 = json!({ "svg": { "profile": "Svg11Rec20110816", "force_profile": true, "runtime_compat": false } });
    let (mut client, _) = Client::start(&json!({}), &svg11)?;
    assert!(!obsoleted_hover(&mut client, "file:///a.svg")?);
    for settings in [json!({}), Value::Null, json!({ "other": { "x": 1 } })] {
        client.notify(
            "workspace/didChangeConfiguration",
            &json!({ "settings": settings }),
        )?;
    }
    assert!(!obsoleted_hover(&mut client, "file:///b.svg")?);
    client.notify(
        "workspace/didChangeConfiguration",
        &json!({ "settings": { "svg": { "profile": "Svg2EditorsDraft", "force_profile": true } } }),
    )?;
    assert!(obsoleted_hover(&mut client, "file:///c.svg")?);
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn the_server_pulls_svg_settings_from_a_client_that_offers_them() -> TestResult {
    let capabilities = json!({ "workspace": { "configuration": true } });
    let (mut client, _) = Client::start(
        &capabilities,
        &json!({ "svg": { "runtime_compat": false } }),
    )?;
    let svg11 =
        json!({ "profile": "Svg11Rec20110816", "force_profile": true, "runtime_compat": false });
    let answer = |client: &mut Client, value: &Value| -> TestResult {
        let request = client.wait_for(|message| message["method"] == "workspace/configuration")?;
        assert_eq!(request["params"]["items"], json!([{ "section": "svg" }]));
        client.send(&json!({ "jsonrpc": "2.0", "id": request["id"], "result": [value] }))
    };
    answer(&mut client, &svg11)?;
    assert!(!obsoleted_hover(&mut client, "file:///pulled.svg")?);

    client.notify(
        "workspace/didChangeConfiguration",
        &json!({ "settings": null }),
    )?;
    answer(
        &mut client,
        &json!({ "profile": "Svg2EditorsDraft", "force_profile": true }),
    )?;
    assert!(obsoleted_hover(&mut client, "file:///repulled.svg")?);
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn code_actions_honour_the_requested_kinds() -> TestResult {
    let mut client = Client::offline()?;
    client.open("file:///actions.svg", "<svg><rect/></svg>")?;
    let actions = |client: &mut Client, only: Value| -> TestResult<Vec<String>> {
        let response = client.request(
            "textDocument/codeAction",
            &json!({
                "textDocument": { "uri": "file:///actions.svg" },
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
                "context": { "diagnostics": [], "only": only },
            }),
        )?;
        Ok(response["result"]
            .as_array()
            .ok_or("actions")?
            .iter()
            .filter_map(|action| action["kind"].as_str().map(str::to_owned))
            .collect())
    };
    assert_eq!(
        actions(&mut client, json!(["quickfix"]))?,
        Vec::<String>::new()
    );
    assert_eq!(
        actions(&mut client, json!(["source"]))?,
        vec!["source".to_owned()]
    );
    assert_eq!(
        actions(&mut client, Value::Null)?,
        vec!["source".to_owned()]
    );
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn an_unknown_command_is_an_error() -> TestResult {
    let mut client = Client::offline()?;
    let response = client.request(
        "workspace/executeCommand",
        &json!({ "command": "svg.nope", "arguments": [] }),
    )?;
    assert_eq!(response["error"]["code"], json!(-32602), "{response}");
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn incremental_edits_change_only_their_range() -> TestResult {
    let (mut client, init) =
        Client::start(&json!({}), &json!({ "svg": { "runtime_compat": false } }))?;
    assert_eq!(init["result"]["capabilities"]["textDocumentSync"], json!(2));
    let uri = "file:///incremental.svg";
    client.open(
        uri,
        "<svg>\n  <text>😀</text><rect fill=\"red\"/>\n</svg>\n",
    )?;
    let line = "  <text>😀</text><rect fill=\"";
    let start = u32::try_from(line.encode_utf16().count())?;
    client.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [
                { "range": { "start": { "line": 1, "character": start }, "end": { "line": 1, "character": start + 3 } }, "text": "blue" },
                { "range": { "start": { "line": 2, "character": 6 }, "end": { "line": 2, "character": 6 } }, "text": "<!-- end -->" },
            ],
        }),
    )?;
    let response = client.request(
        "textDocument/formatting",
        &json!({ "textDocument": { "uri": uri }, "options": { "tabSize": 2, "insertSpaces": true } }),
    )?;
    let colors = client.request(
        "textDocument/documentColor",
        &json!({ "textDocument": { "uri": uri } }),
    )?;
    let color = &colors["result"][0];
    assert_eq!(color["color"]["blue"], json!(1.0), "{colors}");
    assert_eq!(
        color["range"]["start"],
        json!({ "line": 1, "character": start }),
        "{colors}"
    );
    assert!(response.get("error").is_none(), "{response}");
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn hover_reports_the_token_range() -> TestResult {
    let mut client = Client::offline()?;
    let source = "<svg>😀<rect fill=\"red\"/></svg>";
    client.open("file:///range.svg", source)?;
    let column = u32::try_from("<svg>😀<rect ".encode_utf16().count())?;
    let response = client.request(
        "textDocument/hover",
        &json!({ "textDocument": { "uri": "file:///range.svg" }, "position": { "line": 0, "character": column + 1 } }),
    )?;
    assert_eq!(
        response["result"]["range"],
        json!({ "start": { "line": 0, "character": column }, "end": { "line": 0, "character": column + 4 } })
    );
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}

#[test]
fn range_formatting_and_whitespace_options_are_applied() -> TestResult {
    let (mut client, init) =
        Client::start(&json!({}), &json!({ "svg": { "runtime_compat": false } }))?;
    assert_eq!(
        init["result"]["capabilities"]["documentRangeFormattingProvider"],
        json!(true)
    );
    let uri = "file:///range-format.svg";
    let source = "<svg>\n<g>\n<rect/>\n</g>\n<circle/>\n</svg>\n\n\n";
    client.open(uri, source)?;
    let format = |client: &mut Client, method: &str, extra: Value| -> TestResult<Vec<Value>> {
        let mut params = json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 2, "insertSpaces": true, "trimFinalNewlines": true, "insertFinalNewline": true },
        });
        if let (Some(params), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
            params.extend(extra.clone());
        }
        let response = client.request(method, &params)?;
        Ok(response["result"].as_array().ok_or("edits")?.clone())
    };
    let full = format(&mut client, "textDocument/formatting", json!({}))?;
    let trailing = json!({
        "range": { "start": { "line": 6, "character": 0 }, "end": { "line": 8, "character": 0 } },
        "newText": "",
    });
    assert_eq!(
        full,
        vec![
            json!({
                "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 5, "character": 0 } },
                "newText": "  <g>\n    <rect />\n  </g>\n  <circle />\n",
            }),
            trailing.clone(),
        ]
    );
    let ranged = format(
        &mut client,
        "textDocument/rangeFormatting",
        json!({ "range": { "start": { "line": 4, "character": 0 }, "end": { "line": 4, "character": 9 } } }),
    )?;
    assert_eq!(ranged, full[..1].to_vec());
    let tail = format(
        &mut client,
        "textDocument/rangeFormatting",
        json!({ "range": { "start": { "line": 7, "character": 0 }, "end": { "line": 7, "character": 0 } } }),
    )?;
    assert_eq!(tail, vec![trailing]);
    assert!(client.shutdown_and_exit()?.success());
    Ok(())
}
