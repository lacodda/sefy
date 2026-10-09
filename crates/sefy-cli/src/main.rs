//! Command-line interface for sefy.
//!
//! Every command that touches a vault follows the same shape: resolve the file,
//! read the master password, open the vault in memory, act, and — for anything
//! that changed something — seal it back to disk.

mod autosync;
mod browser;
mod cli;
mod commands;
mod device;
mod diff;
mod doctor;
mod editor;
mod launch;
mod output;
mod picker;
mod qr;
mod session;
mod when;

use anyhow::Result;
use autosync::AutoSync;
use clap::{CommandFactory, Parser};
use cli::{Cli, Command, PluginAction};
use std::process::ExitCode;

/// What `sefy run` ends with when sefy itself failed before the command
/// started: the vault, the password, a reference. The status `env` uses for
/// its own failures, so it is never mistaken for one the command returned.
const RUN_FAILED: u8 = 125;

fn main() -> ExitCode {
    let arguments = Cli::parse();
    let runs_a_command = matches!(arguments.command, Some(Command::Run(_)));
    match run(arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // `{error:#}` prints the whole context chain on one line, which is
            // what makes "cannot read X: permission denied" readable.
            eprintln!("error: {error:#}");
            if !runs_a_command {
                return ExitCode::FAILURE;
            }
            match error.downcast_ref::<launch::NotStarted>() {
                Some(not_started) => ExitCode::from(not_started.status()),
                None => ExitCode::from(RUN_FAILED),
            }
        }
    }
}

fn run(arguments: Cli) -> Result<()> {
    let password_env = arguments.password_env.as_deref();

    // No subcommand: browse the vault. It still needs the file and the
    // password, so it falls through to the block below rather than being
    // handled here with the commands that need neither.
    let Some(command) = arguments.command else {
        let path = session::vault_path(arguments.vault)?;
        let password = session::password(password_env)?;
        let vault = session::open(&path, &password)?;
        return commands::browse(&vault);
    };

    // None of these needs a vault, so they come before the file is resolved.
    match command {
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
        // Only keeping the result needs the vault.
        Command::Gen(args) if args.save.is_none() => {
            return commands::generate(None, args, &mut AutoSync::off());
        }
        Command::Init => {
            let path = session::vault_path(arguments.vault)?;
            return commands::init(&path, password_env);
        }
        // The doctor reports a missing vault or a wrong password as findings,
        // so it resolves both itself rather than failing on them here.
        Command::Doctor(remote) => {
            return doctor::run(arguments.vault, password_env, remote, arguments.auto_sync);
        }
        _ => {}
    }

    let path = session::vault_path(arguments.vault)?;
    let password = session::password(password_env)?;
    let mut vault = session::open(&path, &password)?;

    // Comes back only when the command could not be started; it takes the
    // vault with it, and changes nothing that could need syncing.
    let command = match command {
        Command::Run(args) => match commands::run(vault, args, password_env)? {},
        other => other,
    };

    // A transfer is a sync already; syncing after it would be a second one.
    let transfers = matches!(
        command,
        Command::Push(_) | Command::Pull(_) | Command::Sync(_)
    );
    let mut auto = if transfers {
        AutoSync::off()
    } else {
        AutoSync::new(arguments.auto_sync, &password)
    };

    let outcome = match command {
        Command::Add { kind } => commands::add(&mut vault, kind),
        Command::Get(args) => commands::get(&vault, args),
        Command::Show { reference } => commands::show(&vault, &reference),
        Command::Ls(args) => commands::ls(&vault, args),
        Command::Find(args) => commands::find(&vault, args),
        Command::Edit(args) => commands::edit(&mut vault, args),
        Command::History(args) => commands::history(&vault, args),
        Command::Restore(args) => commands::restore(&mut vault, args),
        Command::Rm { reference, yes } => commands::rm(&mut vault, &reference, yes),
        Command::Extract {
            reference,
            output,
            force,
        } => commands::extract(&vault, &reference, output, force),
        Command::Tags => commands::tags(&vault),
        Command::Gen(args) => commands::generate(Some(&mut vault), args, &mut auto),
        Command::Open(args) => commands::open(&vault, args),
        Command::Otp(args) => commands::otp(&mut vault, args, &mut auto),
        Command::Fill(args) => commands::fill(&vault, args),
        Command::Status => commands::status(&vault),
        Command::Export {
            output,
            format,
            with_history,
            i_know_this_writes_plaintext,
            force,
        } => commands::export(
            &vault,
            output,
            format,
            with_history,
            i_know_this_writes_plaintext,
            force,
        ),
        Command::Import { input } => commands::import(&mut vault, input),
        Command::Merge {
            other,
            other_password_env,
            dry_run,
        } => commands::merge(&mut vault, &other, other_password_env.as_deref(), dry_run),
        Command::ChangePassword { new_password_env } => {
            commands::change_password(&mut vault, new_password_env.as_deref())
        }
        Command::Push(args) => commands::push(&mut vault, args),
        Command::Pull(args) => commands::pull(&mut vault, args, &password),
        Command::Sync(args) => commands::sync(&mut vault, args, &password),
        // Handled above: these before the vault is opened, `run` just after.
        Command::Init
        | Command::Plugin { .. }
        | Command::Completions { .. }
        | Command::Doctor(_)
        | Command::Run(_) => unreachable!(),
    };

    // Whether or not the command went on to fail: a write it made before
    // failing is on disk all the same, and belongs on the remote with the rest.
    auto.catch_up(&mut vault);
    outcome
}
