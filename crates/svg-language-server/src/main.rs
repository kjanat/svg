//! Binary entrypoint for the `svg-language-server` executable.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(arg) => {
            eprintln!("unexpected argument: {arg}");
            eprintln!("usage: {} [--version]", env!("CARGO_PKG_NAME"));
            eprintln!("runs an LSP server over stdio when invoked without arguments");
            ExitCode::from(2)
        }
        None => {
            svg_language_server::run_stdio_server().await;
            ExitCode::SUCCESS
        }
    }
}
