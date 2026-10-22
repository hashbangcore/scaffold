//! Command line dispatch: native verbs now, dynamic init flags later.

use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::discovery;
use crate::error::{Error, Result};

const VERSION: &str = env!("CARGO_PKG_VERSION");

enum Action {
    List(Option<String>),
}

pub fn run() -> Result<()> {
    let argv: Vec<OsString> = std::env::args_os().collect();
    let first: Option<String> = argv.get(1).map(|raw| raw.to_string_lossy().into_owned());
    match first.as_deref() {
        // clap only accepts `-V/--version`; `-v` is routed by hand.
        Some("-v") => {
            println!("scaffold {VERSION}");
            Ok(())
        }
        _ => dispatch(argv),
    }
}

fn dispatch(argv: Vec<OsString>) -> Result<()> {
    match parse_action(argv)? {
        Action::List(group) => run_list(group),
    }
}

fn parse_action(argv: Vec<OsString>) -> Result<Action> {
    let matches = parse_native(argv);
    if let Some((_, list_matches)) = matches.subcommand() {
        return Ok(Action::List(
            list_matches.get_one::<String>("group").cloned(),
        ));
    }
    if matches.get_flag("root_list") {
        return Ok(Action::List(
            matches.get_one::<String>("root_group").cloned(),
        ));
    }
    Err(Error::Internal {
        reason: "native parse produced no dispatchable action".to_owned(),
    })
}

/// Parse root arguments with clap, keeping help/version observable.
fn parse_native(argv: Vec<OsString>) -> ArgMatches {
    match native_root().try_get_matches_from(argv) {
        Ok(matches) => matches,
        Err(err) => match err.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                print!("{}", err.render());
                std::process::exit(0);
            }
            ErrorKind::DisplayVersion => {
                print!("{}", err.render());
                std::process::exit(0);
            }
            _ => {
                eprintln!("{err}");
                std::process::exit(2);
            }
        },
    }
}

/// Root command listing at top level: `scaffold [-l [group]]`.
fn native_root() -> Command {
    Command::new("scaffold")
        .version(VERSION)
        .about("Materialize templates from a local template repository")
        .long_about(
            "Groups (one per language) live under $SCAFFOLD. \
             `scaffold init` materializes a template; \
             flags under init come from each template's schema.json.",
        )
        .arg_required_else_help(true)
        .arg(
            Arg::new("root_list")
                .short('l')
                .long("list")
                .action(ArgAction::SetTrue)
                .help("list groups; with an argument, list that group's templates"),
        )
        .arg(
            Arg::new("root_group")
                .help("group to inspect; requires --list")
                .requires("root_list"),
        )
        .subcommand(
            Command::new("list")
                .about("List groups, or the templates of one group")
                .arg(Arg::new("group").help("group name; when omitted, list all groups")),
        )
}

fn run_list(group: Option<String>) -> Result<()> {
    let env_value = std::env::var_os("SCAFFOLD");
    let repo = discovery::source_root(env_value.as_deref())?;
    match group {
        None => {
            for name in discovery::groups(&repo)? {
                println!("{name}");
            }
        }
        Some(group) => {
            let names = discovery::templates(&repo, &group)?;
            if names.is_empty() {
                println!("no templates under group '{group}'");
            } else {
                for name in names {
                    println!("{name}");
                }
            }
        }
    }
    Ok(())
}
