//! Binary entrypoint for the `svg-language-server` executable.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => args.next().map_or_else(
            || {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                ExitCode::SUCCESS
            },
            |trailing| usage_error(&trailing),
        ),
        // LSP clients conventionally pass --stdio; stdio is the only
        // transport, so accept it as a no-op.
        Some("--stdio") => {
            if let Some(trailing) = args.next() {
                return usage_error(&trailing);
            }
            svg_language_server::run_stdio_server().await;
            ExitCode::SUCCESS
        }
        Some(arg) => usage_error(arg),
        None => {
            svg_language_server::run_stdio_server().await;
            ExitCode::SUCCESS
        }
    }
}

fn usage_error(arg: &str) -> ExitCode {
    eprintln!("unexpected argument: {arg}");
    eprintln!("usage: {} [--stdio | --version]", env!("CARGO_PKG_NAME"));
    eprintln!("runs an LSP server over stdio");
    ExitCode::from(2)
}
