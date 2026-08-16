//! Command-line interface for sefy.
//!
//! Every command that touches a vault follows the same shape: resolve the file,
//! read the master password, open the vault in memory, act, and — for anything
//! that changed something — seal it back to disk.

mod cli;
mod commands;
mod editor;
mod output;
mod session;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::{Cli, Command, PluginAction};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // `{error:#}` prints the whole context chain on one line, which is
            // what makes "cannot read X: permission denied" readable.
            eprintln!("error: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let arguments = Cli::parse();
    let password_env = arguments.password_env.as_deref();

    // None of these needs a vault, so they come before the file is resolved.
    match arguments.command {
        Command::Plugin { action } => {
            return match action {
                PluginAction::List { paths } => commands::plugin_list(paths),
            };
        }
        Command::Completions { shell } => {
            let mut command = Cli::command();
            let name = command.get_name().to_owned();
            clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
            return Ok(());
        }
        Command::Init => {
            let path = session::vault_path(arguments.vault)?;
            return commands::init(&path, password_env);
        }
        _ => {}
    }

    let path = session::vault_path(arguments.vault)?;
    let password = session::password(password_env)?;
    let mut vault = session::open(&path, &password)?;

    match arguments.command {
        Command::Add { kind } => commands::add(&mut vault, kind),
        Command::Get(args) => commands::get(&vault, args),
        Command::Show { reference } => commands::show(&vault, &reference),
        Command::Ls(args) => commands::ls(&vault, args),
        Command::Find(args) => commands::find(&vault, args),
        Command::Edit(args) => commands::edit(&mut vault, args),
        Command::Rm { reference, yes } => commands::rm(&mut vault, &reference, yes),
        Command::Extract {
            reference,
            output,
            force,
        } => commands::extract(&vault, &reference, output, force),
        Command::Tags => commands::tags(&vault),
        Command::Export {
            output,
            i_know_this_writes_plaintext,
            force,
        } => commands::export(&vault, output, i_know_this_writes_plaintext, force),
        Command::Import { input } => commands::import(&mut vault, input),
        Command::Merge {
            other,
            other_password_env,
        } => commands::merge(&mut vault, &other, other_password_env.as_deref()),
        Command::ChangePassword { new_password_env } => {
            commands::change_password(&mut vault, new_password_env.as_deref())
        }
        // All three are handled above, before the vault is opened.
        Command::Init | Command::Plugin { .. } | Command::Completions { .. } => unreachable!(),
    }
}
