//! Command line dispatch: native verbs plus a template-driven init parser.

use std::collections::BTreeMap;
use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::discovery;
use crate::error::{Error, Result};
use crate::materialize;
use crate::schema::{self, Schema};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Long names accepted under `init` besides the template's own variables.
const NATIVE_LONGS: &[&str] = &["name", "output", "template", "force", "help"];

pub fn run() -> Result<()> {
    let argv: Vec<OsString> = std::env::args_os().collect();
    let first: Option<String> = argv.get(1).map(|raw| raw.to_string_lossy().into_owned());
    match first.as_deref() {
        // clap only accepts `-V/--version`; `-v` is routed by hand.
        Some("-v") => {
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
    let group = if let Some((_, list_matches)) = matches.subcommand() {
        list_matches.get_one::<String>("group").cloned()
    } else if matches.get_flag("root_list") {
        matches.get_one::<String>("root_group").cloned()
    } else {
        return Err(Error::Internal {
            reason: "native parse produced no dispatchable action".to_owned(),
        });
    };
    run_list(group)
}

/// Parse root arguments with clap, keeping help/version observable.
fn native_root() -> Command {
    Command::new("scaffold")
        .version(VERSION)
        .about("Materialize templates from a local template repository")
        .long_about(
            "Groups (one per language) live under $SCAFFOLD. `scaffold init` \
             materializes a template; arguments under init come from each \
             template's schema.json.",
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
        // Displayed in the root help; `init` itself never parses here.
        .subcommand(
            Command::new("init").about("Materialize a template (see `scaffold init --help`)"),
        )
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

// --- Init path: early scan, schema lookup, dynamic flags, materialization ---

/// The selection found before any schema is known.
#[derive(Debug, Default, PartialEq)]
struct Scan {
    group: Option<String>,
    /// Template from the composed `group/template` positional.
    composite: Option<String>,
    /// Template from a `-t/--template` anywhere on the line.
    flagged: Option<String>,
}

impl Scan {
    /// `-t` takes precedence over the composed form.
    fn explicit(&self) -> Option<String> {
        self.flagged.clone().or(self.composite.clone())
    }
}

/// Scan raw argv for the `[group][/template]` selection and explicit `-t`.
///
/// The selection positional comes first by contract; known value-taking
/// natives (`-t`, `-o`, `--name`) consume their next token so flag values are
/// never mistaken for the selection.
fn scan_selection(tokens: &[OsString]) -> Scan {
    let mut scan = Scan::default();
    let mut skip_next = false;
    for (index, raw) in tokens.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        let token = raw.to_string_lossy().into_owned();
        if token == "-t" || token == "--template" {
            scan.flagged = tokens
                .get(index + 1)
                .map(|raw| raw.to_string_lossy().into_owned());
            skip_next = true;
        } else if let Some(value) = inline_value_of(&token, "--template") {
            scan.flagged = Some(value);
        } else if token == "-o" || token == "--output" || token == "--name" {
            skip_next = true;
        } else if !token.starts_with('-') && scan.group.is_none() {
            let (group, template) = match token.rsplit_once('/') {
                Some((head, tail)) => (head, Some(tail.to_owned())),
                None => (token.as_str(), None),
            };
            scan.group = Some(group.trim_end_matches('/').to_owned());
            scan.composite = template;
        }
    }
    scan
}

/// Value of a `--flag=value` token, if that is its shape.
fn inline_value_of(token: &str, flag: &str) -> Option<String> {
    token
        .strip_prefix(flag)
        .filter(|rest| rest.starts_with('='))
        .map(|value| value[1..].to_owned())
}

/// The init command: native flags plus one `--<variable>` per schema variable.
fn init_root(variables: &[String]) -> Command {
    let mut cmd = Command::new("init")
        .about("Materialize a template from a group")
        .arg(
            Arg::new("selection")
                .required(true)
                .help("group[/template]; -t overrides a composed template"),
        )
        .arg(
            Arg::new("dest")
                .help("destination path; '/'-suffixed or existing directory gets a project"),
        )
        .arg(
            Arg::new("name")
                .long("name")
                .num_args(1)
                .help("project name; drives the default destination and the name variable"),
        )
        .arg(
            Arg::new("template")
                .short('t')
                .long("template")
                .num_args(1)
                .help("template alias inside the group"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .num_args(1)
                .help("destination path; wins over the positional destination"),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .action(ArgAction::SetTrue)
                .help("overwrite existing destination files"),
        );
    for variable in variables {
        if variable == "name" {
            continue; // the native --name covers it
        }
        cmd = cmd.arg(
            Arg::new(variable.clone())
                .long(variable.clone())
                .num_args(1)
                .required(false),
        );
    }
    Command::new("scaffold").subcommand(cmd)
}

/// Current init dispatch: discovery, schema, dynamic parse, materialize.
fn run_init(argv: &[OsString]) -> Result<()> {
    let env_value = std::env::var_os("SCAFFOLD");
    let repo = discovery::source_root(env_value.as_deref())?;
    let scan = scan_selection(&argv[2..]);
    let Some(group) = scan.group.clone() else {
        // Nothing to resolve; clap reports the missing selection usage error.
        static_usage(argv.to_vec());
    };
    let explicit = scan.explicit();
    let selected = discovery::select_template(&repo, &group, explicit.as_deref())?;
    let label = format!("{}/{}", selected.group, selected.template);
    let schema = schema::load(&selected.dir, &label)?;
    let variables: Vec<String> = schema
        .as_ref()
        .map(|schema| schema.variables.keys().cloned().collect())
        .unwrap_or_default();
    reject_unknown_flags(&argv[2..], &variables, &selected)?;
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

/// Unknown flags must reference the template's declared variables.
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
        if matches!(head, "-t" | "--template" | "-o" | "--output" | "--name")
            && !token.contains('=')
        {
            skip_next = true;
            continue;
        }
        let stripped = head.trim_start_matches('-');
        if NATIVE_LONGS.contains(&stripped) || variables.iter().any(|var| var == stripped) {
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
        if schema.variables.contains_key("name")
            && let Some(name) = matches.get_one::<String>("name")
        {
            overrides.insert("name".to_owned(), name.clone());
        }
    }
    materialize::InitOptions {
        name: matches.get_one::<String>("name").cloned(),
        dest: matches.get_one::<String>("dest").cloned(),
        output: matches.get_one::<String>("output").cloned(),
        force: matches.get_flag("force"),
        overrides,
    }
}

#[cfg(test)]
mod tests;
