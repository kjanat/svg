use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream},
    sync::mpsc,
};
use tower_lsp_server::{
    ClientSocket, LanguageServer, LspService, Server,
    jsonrpc::{Request, Response},
};

const HEADER_END: &[u8] = b"\r\n\r\n";
const CONTENT_LENGTH: &[u8] = b"content-length:";
const MAX_HEADER_BYTES: usize = 8 * 1024;
const PIPE_CAPACITY: usize = 64 * 1024;
const READ_CHUNK: usize = 8 * 1024;
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;

pub async fn serve<I, O, S>(input: I, output: O, service: LspService<S>, socket: ClientSocket)
where
    I: AsyncRead + Unpin,
    O: AsyncWrite + Unpin + Send + 'static,
    S: LanguageServer,
{
    let (frames_tx, frames_rx) = mpsc::unbounded_channel();
    let writer = tokio::spawn(write_frames(frames_rx, output));
    let (to_server, server_input) = tokio::io::duplex(PIPE_CAPACITY);
    let server_output = FrameSink::new(frames_tx.clone());

    run_until_served(
        Server::new(server_input, server_output, socket).serve(service),
        filter_input(input, to_server, frames_tx),
    )
    .await;

    if let Err(error) = writer.await {
        tracing::warn!(%error, "LSP output writer stopped abnormally");
    }
}

async fn run_until_served(served: impl Future<Output = ()>, filtered: impl Future<Output = ()>) {
    tokio::pin!(served, filtered);
    tokio::select! {
        () = &mut served => {}
        () = &mut filtered => served.await,
    }
}

async fn write_frames<O>(mut frames: mpsc::UnboundedReceiver<Vec<u8>>, mut output: O)
where
    O: AsyncWrite + Unpin,
{
    while let Some(frame) = frames.recv().await {
        if let Err(error) = write_frame(&mut output, &frame).await {
            tracing::warn!(%error, "failed to write LSP message");
            return;
        }
    }
}

async fn write_frame<O>(output: &mut O, frame: &[u8]) -> io::Result<()>
where
    O: AsyncWrite + Unpin,
{
    output.write_all(frame).await?;
    output.flush().await
}

async fn filter_input<I>(
    mut input: I,
    mut to_server: DuplexStream,
    errors: mpsc::UnboundedSender<Vec<u8>>,
) where
    I: AsyncRead + Unpin,
{
    let mut reader = FrameReader::default();
    let mut chunk = vec![0; READ_CHUNK];
    loop {
        let read = match input.read(&mut chunk).await {
            Ok(0) => return,
            Ok(read) => read,
            Err(error) => {
                tracing::warn!(%error, "failed to read LSP input");
                return;
            }
        };
        reader.push(chunk.get(..read).unwrap_or_default());
        while let Some(item) = reader.next_item() {
            match item {
                Inbound::Message(body) => {
                    if let Err(error) = forward(&mut to_server, &body).await {
                        tracing::warn!(%error, "LSP server input closed");
                        return;
                    }
                }
                Inbound::Rejected(response) => {
                    if errors.send(frame(&response)).is_err() {
                        return;
                    }
                }
            }
        }
    }
}

async fn forward(to_server: &mut DuplexStream, body: &[u8]) -> io::Result<()> {
    to_server.write_all(&frame(body)).await?;
    to_server.flush().await
}

fn frame(body: &[u8]) -> Vec<u8> {
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend_from_slice(body);
    framed
}

#[derive(Debug, PartialEq, Eq)]
enum Inbound {
    Message(Vec<u8>),
    Rejected(Vec<u8>),
}

#[derive(Default)]
struct FrameReader {
    buffer: Vec<u8>,
    resyncing: bool,
}

impl FrameReader {
    fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    fn next_item(&mut self) -> Option<Inbound> {
        loop {
            if self.resyncing {
                let Some(start) = find_ignore_ascii_case(&self.buffer, CONTENT_LENGTH) else {
                    let keep = self.buffer.len().min(CONTENT_LENGTH.len() - 1);
                    self.buffer.drain(..self.buffer.len() - keep);
                    return None;
                };
                self.buffer.drain(..start);
                self.resyncing = false;
            }
            let Some(header_len) = find(&self.buffer, HEADER_END) else {
                if self.buffer.len() > MAX_HEADER_BYTES {
                    self.buffer.clear();
                    self.resyncing = true;
                    return Some(Inbound::Rejected(error_response(
                        PARSE_ERROR,
                        "Parse error",
                        &Value::Null,
                    )));
                }
                return None;
            };
            let body_start = header_len + HEADER_END.len();
            let Some(length) = content_length(self.buffer.get(..header_len).unwrap_or_default())
            else {
                self.buffer.drain(..body_start);
                self.resyncing = true;
                return Some(Inbound::Rejected(error_response(
                    PARSE_ERROR,
                    "Parse error",
                    &Value::Null,
                )));
            };
            let Some(body_end) = body_start.checked_add(length) else {
                self.buffer.drain(..body_start);
                self.resyncing = true;
                return Some(Inbound::Rejected(error_response(
                    PARSE_ERROR,
                    "Parse error",
                    &Value::Null,
                )));
            };
            if self.buffer.len() < body_end {
                return None;
            }
            let body: Vec<u8> = self.buffer.drain(..body_end).skip(body_start).collect();
            if body.is_empty() {
                continue;
            }
            return Some(classify(body));
        }
    }
}

fn classify(body: Vec<u8>) -> Inbound {
    let value: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => {
            return Inbound::Rejected(error_response(PARSE_ERROR, "Parse error", &Value::Null));
        }
    };
    let decodes = if value.get("method").is_some() {
        serde_json::from_value::<Request>(value.clone()).is_ok()
    } else {
        value.get("result").is_some() != value.get("error").is_some()
            && serde_json::from_value::<Response>(value.clone()).is_ok()
    };
    if decodes {
        return Inbound::Message(body);
    }
    let id = value
        .get("id")
        .filter(|id| id.is_string() || id.is_number())
        .cloned()
        .unwrap_or(Value::Null);
    Inbound::Rejected(error_response(INVALID_REQUEST, "Invalid request", &id))
}

fn error_response(code: i64, message: &str, id: &Value) -> Vec<u8> {
    json!({
        "jsonrpc": "2.0",
        "error": { "code": code, "message": message },
        "id": id,
    })
    .to_string()
    .into_bytes()
}

fn content_length(header: &[u8]) -> Option<usize> {
    let header = std::str::from_utf8(header).ok()?;
    header.split("\r\n").find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse().ok())
            .flatten()
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

struct FrameSink {
    pending: Vec<u8>,
    frames: mpsc::UnboundedSender<Vec<u8>>,
}

impl FrameSink {
    const fn new(frames: mpsc::UnboundedSender<Vec<u8>>) -> Self {
        Self {
            pending: Vec::new(),
            frames,
        }
    }

    fn emit_complete_frames(&mut self) -> io::Result<()> {
        while let Some(header_len) = find(&self.pending, HEADER_END) {
            let body_start = header_len + HEADER_END.len();
            let length = content_length(self.pending.get(..header_len).unwrap_or_default())
                .ok_or_else(|| io::Error::other("LSP output frame without Content-Length"))?;
            let end = body_start
                .checked_add(length)
                .ok_or_else(|| io::Error::other("LSP output frame length overflows"))?;
            if self.pending.len() < end {
                return Ok(());
            }
            let frame: Vec<u8> = self.pending.drain(..end).collect();
            self.frames
                .send(frame)
                .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        }
        Ok(())
    }
}

impl AsyncWrite for FrameSink {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.pending.extend_from_slice(bytes);
        Poll::Ready(self.emit_complete_frames().map(|()| bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(body: &str) -> Vec<u8> {
        frame(body.as_bytes())
    }

    fn drain(reader: &mut FrameReader) -> Vec<Inbound> {
        std::iter::from_fn(|| reader.next_item()).collect()
    }

    fn rejected(code: i64, id: &Value) -> Inbound {
        let message = if code == PARSE_ERROR {
            "Parse error"
        } else {
            "Invalid request"
        };
        Inbound::Rejected(error_response(code, message, id))
    }

    const REQUEST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"shutdown"}"#;

    #[test]
    fn valid_messages_pass_through_in_order() {
        let mut reader = FrameReader::default();
        let notification = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        let response = r#"{"jsonrpc":"2.0","id":7,"result":null}"#;
        reader.push(&[framed(REQUEST), framed(notification), framed(response)].concat());
        assert_eq!(
            drain(&mut reader),
            vec![
                Inbound::Message(REQUEST.into()),
                Inbound::Message(notification.into()),
                Inbound::Message(response.into()),
            ]
        );
    }

    #[test]
    fn split_frames_wait_for_their_remaining_bytes() {
        let mut reader = FrameReader::default();
        let bytes = framed(REQUEST);
        let (head, tail) = bytes.split_at(30);
        reader.push(head);
        assert_eq!(drain(&mut reader), vec![]);
        reader.push(tail);
        assert_eq!(drain(&mut reader), vec![Inbound::Message(REQUEST.into())]);
    }

    #[test]
    fn malformed_bodies_are_answered_and_reading_continues() {
        let mut reader = FrameReader::default();
        reader.push(
            &[
                framed(r#"{"jsonrpc":"2.0","method":"#),
                framed("[]"),
                framed(r#"{"jsonrpc":"2.0","id":4,"nonsense":true}"#),
                framed(REQUEST),
            ]
            .concat(),
        );
        assert_eq!(
            drain(&mut reader),
            vec![
                rejected(PARSE_ERROR, &Value::Null),
                rejected(INVALID_REQUEST, &Value::Null),
                rejected(INVALID_REQUEST, &json!(4)),
                Inbound::Message(REQUEST.into()),
            ]
        );
    }

    #[test]
    fn a_header_without_content_length_resyncs_at_the_next_frame() {
        let mut reader = FrameReader::default();
        let body = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        let broken = format!("Content-Lenght: {}\r\n\r\n{body}", body.len());
        reader.push(&[broken.into_bytes(), framed(REQUEST)].concat());
        assert_eq!(
            drain(&mut reader),
            vec![
                rejected(PARSE_ERROR, &Value::Null),
                Inbound::Message(REQUEST.into()),
            ]
        );
    }

    #[test]
    fn content_type_headers_and_header_case_are_accepted() {
        let mut reader = FrameReader::default();
        let header = format!(
            "content-length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n",
            REQUEST.len()
        );
        reader.push(&[header.into_bytes(), REQUEST.as_bytes().to_vec()].concat());
        assert_eq!(drain(&mut reader), vec![Inbound::Message(REQUEST.into())]);
    }

    #[test]
    fn oversized_garbage_is_rejected_once() {
        let mut reader = FrameReader::default();
        reader.push(&vec![b'x'; MAX_HEADER_BYTES + 1]);
        assert_eq!(
            drain(&mut reader),
            vec![rejected(PARSE_ERROR, &Value::Null)]
        );
        reader.push(&framed(REQUEST));
        assert_eq!(drain(&mut reader), vec![Inbound::Message(REQUEST.into())]);
    }

    #[test]
    fn frame_sink_emits_whole_frames_from_partial_writes() -> io::Result<()> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut sink = FrameSink::new(tx);
        let bytes = [framed(REQUEST), framed(REQUEST)].concat();
        let (head, tail) = bytes.split_at(50);
        sink.pending.extend_from_slice(head);
        sink.emit_complete_frames()?;
        assert!(rx.try_recv().is_err());
        sink.pending.extend_from_slice(tail);
        sink.emit_complete_frames()?;
        assert_eq!(rx.try_recv().ok(), Some(framed(REQUEST)));
        assert_eq!(rx.try_recv().ok(), Some(framed(REQUEST)));
        Ok(())
    }
}
