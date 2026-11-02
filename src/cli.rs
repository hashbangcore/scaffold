//! Command line dispatch: native verbs plus a template-driven init parser.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use clap::error::ErrorKind;
use clap::{Arg, ArgMatches, Command};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use clap_complete::env::CompleteEnv;
use clap_complete::env::Shells;

use crate::discovery;
use crate::error::{Error, Result};
use crate::materialize;
use crate::schema::{self, Schema};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() -> Result<()> {
    CompleteEnv::with_factory(native_root).complete();
    let argv: Vec<OsString> = std::env::args_os().collect();
    let first: Option<String> = argv.get(1).map(|raw| raw.to_string_lossy().into_owned());
    match first.as_deref() {
        None | Some("help") => {
            print_help();
            Ok(())
        }
        Some("version") => {
            println!("scaffold {VERSION}");
            Ok(())
        }
        Some("init") => run_init(&argv),
        _ => run_native(argv),
    }
}

fn run_native(argv: Vec<OsString>) -> Result<()> {
    let matches = match native_root().try_get_matches_from(argv) {
        Ok(matches) => matches,
        Err(err) => clap_exit(err),
    };
    match matches.subcommand() {
        Some(("list", list_matches)) => run_list(list_matches.get_one::<String>("group").cloned()),
        Some(("schema", schema_matches)) => {
            let selection = schema_matches
                .get_one::<String>("selection")
                .expect("schema selection is required");
            run_schema(selection)
        }
        Some(("completion", completion_matches)) => {
            let shell = completion_matches
                .get_one::<String>("shell")
                .expect("completion shell is required");
            write_completion(shell)
        }
        _ => Err(Error::Internal {
            reason: "native parse produced no dispatchable action".to_owned(),
        }),
    }
}

/// Build the single command reference shown by `help` and an empty invocation.
fn native_root() -> Command {
    Command::new("scaffold")
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .subcommand(
            Command::new("init")
                .disable_help_flag(true)
                .disable_version_flag(true)
                .disable_help_subcommand(true)
                .about("Copy a scaffold with schema-defined variable options")
                .arg(
                    Arg::new("selection")
                        .required(true)
                        .add(ArgValueCompleter::new(complete_scaffolds)),
                ),
        )
        .subcommand(
            Command::new("list")
                .about("List groups or templates in a group")
                .arg(Arg::new("group").help("group whose templates should be listed")),
        )
        .subcommand(
            Command::new("schema")
                .disable_help_flag(true)
                .disable_version_flag(true)
                .disable_help_subcommand(true)
                .about("Print a scaffold's schema.json")
                .arg(
                    Arg::new("selection")
                        .required(true)
                        .help("group or group/template selection")
                        .add(ArgValueCompleter::new(complete_scaffolds)),
                ),
        )
        .subcommand(Command::new("help").about("Show this command reference"))
        .subcommand(Command::new("version").about("Show the program version"))
        .subcommand(
            Command::new("completion")
                .disable_help_flag(true)
                .disable_version_flag(true)
                .disable_help_subcommand(true)
                .about("Generate shell completion setup")
                .arg(
                    Arg::new("shell")
                        .required(true)
                        .value_parser(["bash", "elvish", "fish", "powershell", "zsh"])
                        .help("shell to generate completion setup for"),
                ),
        )
        .after_help("Use the $SCAFFOLDS environment variable to select the scaffold directory.")
}

fn write_completion(shell: &str) -> Result<()> {
    let shells = Shells::builtins();
    let completer = shells.completer(shell).ok_or_else(|| Error::Internal {
        reason: format!("no completion generator for shell `{shell}`"),
    })?;
    let binary = std::env::args_os()
        .next()
        .unwrap_or_else(|| OsString::from("scaffold"));
    let binary = binary.to_string_lossy();
    completer
        .write_registration(
            "COMPLETE",
            "scaffold",
            "scaffold",
            &binary,
            &mut std::io::stdout(),
        )
        .map_err(|source| Error::io("write shell completion setup", source))
}

fn complete_scaffolds(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    let Ok(root) = discovery::source_root(std::env::var_os("SCAFFOLDS").as_deref()) else {
        return Vec::new();
    };
    let Ok(groups) = discovery::groups(&root) else {
        return Vec::new();
    };
    complete_scaffold_candidates(&root, current, groups)
        .into_iter()
        .map(CompletionCandidate::new)
        .collect()
}

fn complete_scaffold_candidates(root: &Path, current: &str, groups: Vec<String>) -> Vec<String> {
    let mut candidates = Vec::new();
    for group in groups {
        let Ok(templates) = discovery::templates(root, &group) else {
            continue;
        };
        for template in templates {
            let selection = format!("{group}/{template}");
            if selection.starts_with(current) {
                candidates.push(selection);
            }
        }
    }
    candidates
}

fn print_help() {
    let mut command = native_root();
    let _ = command.print_help();
    println!();
}

/// Terminate in clap's own style: help/version to stdout with 0, usage errors to stderr with 2.
fn clap_exit(err: clap::Error) -> ! {
    match err.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        | ErrorKind::DisplayVersion => println!("{}", err.render()),
        _ => eprintln!("{err}"),
    }
    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            | ErrorKind::DisplayVersion
    ) {
        std::process::exit(0);
    }
    std::process::exit(2);
}

fn run_list(group: Option<String>) -> Result<()> {
    let env_value = std::env::var_os("SCAFFOLDS");
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

fn select_scaffold(repo: &Path, selection: &str) -> Result<discovery::Selected> {
    let selection = selection.strip_suffix('/').unwrap_or(selection);
    match selection.split_once('/') {
        Some((group, template)) if !group.is_empty() && !template.is_empty() => {
            discovery::select_template(repo, group, Some(template))
        }
        Some(_) => Err(Error::InvalidSelection {
            selection: selection.to_owned(),
        }),
        None => discovery::select_template(repo, selection, None),
    }
}

fn run_schema(selection: &str) -> Result<()> {
    let env_value = std::env::var_os("SCAFFOLDS");
    let repo = discovery::source_root(env_value.as_deref())?;
    let selected = select_scaffold(&repo, selection)?;
    let schema_path = selected.dir.join(schema::SCHEMA_FILE);
    match std::fs::read_to_string(&schema_path) {
        Ok(contents) => print!("{contents}"),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            println!("No schema.json for '{selection}'.");
        }
        Err(source) => {
            return Err(Error::io(format!("read schema for '{selection}'"), source));
        }
    }
    Ok(())
}

// --- Init path: early scan, schema lookup, dynamic flags, materialization ---

/// The init command accepts one destination and the selected template's variables.
fn init_root(variables: &[String]) -> Command {
    let mut cmd = Command::new("init")
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .about("Materialize a scaffold")
        .arg(
            Arg::new("selection")
                .required(true)
                .help("group or group/template selection"),
        )
        .arg(
            Arg::new("destination")
                .help("destination directory; defaults to the current directory"),
        );
    for variable in variables {
        cmd = cmd.arg(
            Arg::new(variable.clone())
                .long(variable.clone())
                .num_args(1)
                .required(false),
        );
    }
    Command::new("scaffold")
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .subcommand(cmd)
}

/// Resolve a template, parse its declared variables, and materialize it.
fn run_init(argv: &[OsString]) -> Result<()> {
    let env_value = std::env::var_os("SCAFFOLDS");
    let repo = discovery::source_root(env_value.as_deref())?;
    let selection = argv.get(2).map(|raw| raw.to_string_lossy().into_owned());
    let Some(selection) = selection.filter(|value| !value.starts_with('-')) else {
        // Nothing to resolve; clap reports the missing selection usage error.
        static_usage(argv.to_vec());
    };
    let selected = select_scaffold(&repo, &selection)?;
    let label = format!("{}/{}", selected.group, selected.template);
    let schema = schema::load(&selected.dir, &label)?;
    let variables: Vec<String> = schema
        .as_ref()
        .map(|schema| schema.variables.keys().cloned().collect())
        .unwrap_or_default();
    reject_unknown_flags(&argv[3..], &variables, &selected)?;
    let matches = parse_init(&variables, argv.to_vec())?;
    let options = init_options(&matches, schema.as_ref());
    materialize::run(&selected, schema.as_ref(), &options)
}

/// Run the static init parser only for its usage and help output.
fn static_usage(argv: Vec<OsString>) -> ! {
    clap_exit(
        init_root(&[])
            .try_get_matches_from(argv)
            .expect_err("the static init parser always fails without a selection"),
    );
}

fn parse_init(variables: &[String], argv: Vec<OsString>) -> Result<ArgMatches> {
    let matches = match init_root(variables).try_get_matches_from(argv) {
        Ok(matches) => matches,
        Err(err) => clap_exit(err),
    };
    let Some((_, init)) = matches.subcommand() else {
        return Err(Error::Internal {
            reason: "init matches without the init subcommand".to_owned(),
        });
    };
    Ok(init.clone())
}

/// Unknown flags must reference variables declared by the selected template.
fn reject_unknown_flags(
    tokens: &[OsString],
    variables: &[String],
    selected: &discovery::Selected,
) -> Result<()> {
    let mut skip_next = false;
    for raw in tokens {
        if skip_next {
            skip_next = false;
            continue;
        }
        let token = raw.to_string_lossy().into_owned();
        if token == "--" {
            break;
        }
        if !token.starts_with('-') {
            continue;
        }
        let head = match token.split_once('=') {
            Some((head, _)) => head,
            None => token.as_str(),
        };
        if head == "-h" || head == "--help" {
            continue;
        }
        let stripped = head.trim_start_matches('-');
        if stripped == "h" || stripped == "help" {
            continue;
        }
        if variables.iter().any(|var| var == stripped) {
            if !token.contains('=') {
                skip_next = true;
            }
            continue;
        }
        return Err(Error::UnknownFlag {
            flag: head.to_owned(),
            group: selected.group.clone(),
            template: selected.template.clone(),
            variables: variables.to_vec(),
            suggestion: typo_suggestion(stripped, variables),
        });
    }
    Ok(())
}

/// Closest declared variable within a small edit distance, if any.
fn typo_suggestion(typed: &str, variables: &[String]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    let threshold = (typed.len().max(2) / 2).max(1);
    for variable in variables {
        let distance = edit_distance(typed, variable);
        if distance <= threshold && best.is_none_or(|(current, _)| distance < current) {
            best = Some((distance, variable));
        }
    }
    best.map(|(_, variable)| variable.to_owned())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let previous_cell = row[j + 1];
            let substitute = diagonal + usize::from(ca != cb);
            let insert = row[j] + 1;
            let delete = previous_cell + 1;
            row[j + 1] = substitute.min(insert).min(delete);
            diagonal = previous_cell;
        }
    }
    row[b.len()]
}

/// Translate parsed init matches into materialization inputs.
fn init_options(matches: &ArgMatches, schema: Option<&Schema>) -> materialize::InitOptions {
    let mut overrides = BTreeMap::new();
    if let Some(schema) = schema {
        for variable in schema.variables.keys() {
            if let Some(value) = matches.get_one::<String>(variable) {
                overrides.insert(variable.clone(), value.clone());
            }
        }
    }
    materialize::InitOptions {
        destination: matches
            .get_one::<String>("destination")
            .map_or_else(|| PathBuf::from("."), PathBuf::from),
        destination_is_implicit: matches
            .get_one::<String>("destination")
            .is_none_or(|destination| Path::new(destination) == Path::new(".")),
        overrides,
    }
}

#[cfg(test)]
mod tests;
