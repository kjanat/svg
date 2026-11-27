//! Binary entrypoint for the `svg-language-server` executable.

use std::process::ExitCode;

use clap::{CommandFactory as _, Parser};

#[derive(Parser)]
#[command(
    name = "svg-language-server",
    version,
    disable_help_flag = true,
    disable_version_flag = true,
    about = "runs an LSP server over stdio",
    help_template = "{name} {version}\n{about}\n\nusage: {usage}\n\n{all-args}"
)]
struct Cli {
    #[command(flatten)]
    completion: svg_tree::cli::CompletionArgs,

    /// Run the language server over standard input and output (the default).
    #[arg(long, exclusive = true)]
    stdio: bool,

    /// Print help.
    #[arg(short, long, exclusive = true)]
    help: bool,

    /// Print version.
    #[arg(short = 'V', long, exclusive = true)]
    version: bool,
}

fn main() -> ExitCode {
    svg_tree::cli::complete(Cli::command);
    let cli = Cli::parse();
    if let Some(result) = cli.completion.generate(Cli::command) {
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("svg-language-server: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if cli.help {
        print!("{}", Cli::command().render_help());
        ExitCode::SUCCESS
    } else if cli.version {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        ExitCode::SUCCESS
    } else {
        serve()
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
