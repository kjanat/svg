//! Shell completion arguments and generation shared by the SVG executables.

use std::{env, io};

use clap::{Args, Command};
use clap_complete::env::{Bash, EnvCompleter, Fish, Shells, Zsh};
use clap_complete::{ArgValueCandidates, CompletionCandidate};

/// Arguments shared by the SVG command-line tools.
#[derive(Args, Debug)]
pub struct CompletionArgs {
    /// Print a completion script for bash, zsh, fish, or powershell.
    /// With no shell, use `$SHELL` or fall back to PowerShell via `PSModulePath`.
    #[arg(
        long,
        value_name = "SHELL",
        num_args = 0..=1,
        default_missing_value = "auto",
        exclusive = true,
        value_parser = parse_shell,
        add = ArgValueCandidates::new(shell_candidates)
    )]
    completions: Option<CompletionRequest>,
}

#[derive(Clone, Copy, Debug)]
enum CompletionRequest {
    Detect,
    Shell(CompletionShell),
}

#[derive(Clone, Copy, Debug)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

fn shell_candidates() -> Vec<CompletionCandidate> {
    ["bash", "zsh", "fish", "powershell"]
        .into_iter()
        .map(CompletionCandidate::new)
        .collect()
}

fn normalize_shell(value: &str) -> Option<CompletionShell> {
    let name = value.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    let name = [".exe", ".cmd", ".bat"]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(&name);
    match name {
        "bash" => Some(CompletionShell::Bash),
        "zsh" => Some(CompletionShell::Zsh),
        "fish" => Some(CompletionShell::Fish),
        "powershell" | "pwsh" => Some(CompletionShell::Powershell),
        _ => None,
    }
}

fn parse_shell(value: &str) -> Result<CompletionRequest, String> {
    if value == "auto" {
        return Ok(CompletionRequest::Detect);
    }
    normalize_shell(value)
        .map(CompletionRequest::Shell)
        .ok_or_else(|| {
            format!("unsupported shell {value:?}; expected bash, zsh, fish, or powershell")
        })
}

fn detect_shell() -> io::Result<CompletionShell> {
    env::var("SHELL")
        .ok()
        .as_deref()
        .and_then(normalize_shell)
        .or_else(|| env::var_os("PSModulePath").map(|_| CompletionShell::Powershell))
        .ok_or_else(|| {
            io::Error::other("could not detect shell; pass one explicitly, e.g. --completions zsh")
        })
}

impl CompletionArgs {
    /// Print the selected script, if requested.
    pub fn generate(&self, command: impl FnOnce() -> Command) -> Option<io::Result<()>> {
        self.completions.map(|request| {
            let shell = match request {
                CompletionRequest::Detect => detect_shell()?,
                CompletionRequest::Shell(shell) => shell,
            };
            let command = command();
            write_script(shell, command.get_name(), &mut io::stdout().lock())
        })
    }
}

/// Handle a candidate request before parsing the ordinary command line.
pub fn complete(command: impl Fn() -> Command) {
    clap_complete::CompleteEnv::with_factory(command)
        .var("SVG_COMPLETE")
        .shells(Shells(&[&Bash, &Fish, &Zsh]))
        .complete();
}

fn write_script(shell: CompletionShell, name: &str, output: &mut dyn io::Write) -> io::Result<()> {
    let adapter: &dyn EnvCompleter = match shell {
        CompletionShell::Bash => &Bash,
        CompletionShell::Zsh => &Zsh,
        CompletionShell::Fish => &Fish,
        CompletionShell::Powershell => {
            return output.write_all(
                include_str!("completion.ps1")
                    .replace("{command}", name)
                    .as_bytes(),
            );
        }
    };
    adapter.write_registration("SVG_COMPLETE", name, name, name, output)?;
    if matches!(shell, CompletionShell::Zsh) {
        // A file loaded through fpath also handles the first completion.
        writeln!(
            output,
            "\nif [[ $funcstack[1] == _{name} ]]; then \"${{_comps[{name}]}}\" \"$@\"; fi"
        )?;
    }
    Ok(())
}
