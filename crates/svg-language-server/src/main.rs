//! Binary entrypoint for the `svg-language-server` executable.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    if let Some(trailing) = args.next() {
        return usage_error(&trailing);
    }
    match command.as_deref() {
        Some("--version" | "-V") => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            println!("{}", usage());
            ExitCode::SUCCESS
        }
        Some("--stdio") | None => serve(),
        Some(arg) => usage_error(arg),
    }
}

fn serve() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("svg-language-server failed to start its runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let code = runtime.block_on(svg_language_server::run_stdio_server());
    runtime.shutdown_background();
    code
}

fn usage() -> String {
    format!(
        "{name} {version}\nruns an LSP server over stdio\n\nusage: {name} [--stdio | --version | \
         --help]",
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
    )
}

fn usage_error(arg: &str) -> ExitCode {
    eprintln!("unexpected argument: {arg}");
    eprintln!("{}", usage());
    ExitCode::from(2)
}
