//! Integration tests: spawn the built binary against disposable fixtures.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EXPECTED_VERSION: &str = concat!("scaffold ", env!("CARGO_PKG_VERSION"));

/// RAII fixture directory; removed on drop.
struct TempRepo(PathBuf);

impl TempRepo {
    fn new(tag: &str, groups: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!("scaffold-it-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create fixture root");
        for group in groups {
            std::fs::create_dir_all(dir.join(group)).expect("create fixture group");
        }
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let file = self.0.join(rel);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent");
        }
        std::fs::write(file, content).expect("write fixture file");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_scaffold")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/scaffold"))
}

fn run(repo: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env("SCAFFOLDS", repo)
        .output()
        .expect("spawn binary")
}

fn run_without_scaffolds(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env_remove("SCAFFOLDS")
        .output()
        .expect("spawn binary")
}

fn run_in(repo: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .current_dir(cwd)
        .env("SCAFFOLDS", repo)
        .output()
        .expect("spawn binary")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

#[test]
fn version_subcommand_prints_package_version() {
    let output = run(Path::new("."), &["version"]);
    assert!(output.status.success());
    assert_eq!(stdout_of(&output).trim_end(), EXPECTED_VERSION);
}

#[test]
fn empty_invocation_and_help_subcommand_show_the_same_reference() {
    let empty = run(Path::new("."), &[]);
    let help = run(Path::new("."), &["help"]);
    assert!(empty.status.success());
    assert!(help.status.success());
    assert_eq!(stdout_of(&empty), stdout_of(&help));

    let stdout = stdout_of(&help);
    for command in ["init", "list", "schema", "help", "version"] {
        assert!(stdout.contains(command), "help lists {command}: {stdout}");
    }
    assert!(
        stdout.contains("schema-defined variable options"),
        "documents dynamic variables: {stdout}"
    );
    assert!(
        stdout
            .contains("Use the $SCAFFOLDS environment variable to select the scaffold directory.")
    );
}

#[test]
fn schema_prints_the_selected_schema_file() {
    let repo = TempRepo::new("schema-present", &["rust"]);
    let contents = "{\n  \"include\": [\"src/**\"]\n}\n";
    repo.write("rust/min/schema.json", contents);

    let output = run(repo.path(), &["schema", "rust/min"]);

    assert_eq!(exit_code(&output), 0, "{}", stderr_of(&output));
    assert_eq!(stdout_of(&output), contents);
    assert!(stderr_of(&output).is_empty());
}

#[test]
fn schema_without_a_schema_file_prints_a_notice_and_succeeds() {
    let repo = TempRepo::new("schema-missing", &["rust"]);
    repo.write("rust/min/src/main.rs", "fn main() {}\n");

    let output = run(repo.path(), &["schema", "rust/min"]);

    assert_eq!(exit_code(&output), 0, "{}", stderr_of(&output));
    assert_eq!(stdout_of(&output), "No schema.json for 'rust/min'.\n");
    assert!(stderr_of(&output).is_empty());
}

#[test]
fn schema_has_no_subcommand_specific_help() {
    let repo = TempRepo::new("schema-help", &["rust"]);

    let output = run(repo.path(), &["schema", "--help"]);

    assert_ne!(exit_code(&output), 0);
    assert!(!stdout_of(&output).contains("Print a template's schema.json"));
}

#[test]
fn short_and_flag_aliases_are_rejected() {
    for arg in ["-h", "-V", "-l", "-v", "--help", "--version", "--list"] {
        let output = run(Path::new("."), &[arg]);
        assert_ne!(exit_code(&output), 0, "unexpectedly accepted {arg}");
    }
}

#[test]
fn list_prints_sorted_group_names() {
    let repo = TempRepo::new("list-groups", &["rust", "css", "just"]);
    repo.write("notes.md", "not a group\n");

    let output = run(repo.path(), &["list"]);
    assert_eq!(exit_code(&output), 0);
    assert_eq!(stdout_of(&output), "css\njust\nrust\n");
}

#[test]
fn list_subcommand_prints_templates_for_a_group() {
    let repo = TempRepo::new("list-template", &["just"]);
    repo.write("just/vim/placeholder", "-");
    repo.write("just/just/placeholder", "-");
    repo.write("just/loose.txt", "ignored file, not template\n");

    let verb = run(repo.path(), &["list", "just"]);
    assert_eq!(exit_code(&verb), 0);
    assert_eq!(stdout_of(&verb), "just\nvim\n");
}

#[test]
fn list_reports_empty_group() {
    let repo = TempRepo::new("list-empty", &["css"]);

    let output = run(repo.path(), &["list", "css"]);
    assert_eq!(exit_code(&output), 0);
    assert_eq!(stdout_of(&output), "no templates under group 'css'\n");
}

#[test]
fn list_unknown_group_fails_with_clear_message() {
    let repo = TempRepo::new("list-ghost", &["rust"]);

    let output = run(repo.path(), &["list", "ghost"]);
    assert_ne!(exit_code(&output), 0);
    let stderr = stderr_of(&output);
    assert!(stderr.contains("ghost"), "stderr names the group: {stderr}");
    assert!(
        stderr.contains("does not exist"),
        "stderr explains the failure: {stderr}"
    );
}

#[test]
fn missing_scaffold_env_fails_without_files() {
    let repo = TempRepo::new("missing-env", &["rust"]);
    let probe = repo.path().join("probe");

    let output = run_without_scaffolds(&["list"]);
    assert_ne!(exit_code(&output), 0);
    assert!(stderr_of(&output).contains("SCAFFOLDS"));
    assert!(!probe.exists(), "a missing env must not create anything");
}

#[test]
fn init_has_no_subcommand_specific_help() {
    let repo = TempRepo::new("init-help", &["just"]);
    repo.write(
        "just/publish/schema.json",
        r#"{"variables": {"watch": {"__watch__": "true"}}}"#,
    );

    let output = run(repo.path(), &["init", "just/publish", "--help"]);
    assert_ne!(exit_code(&output), 0);
    assert!(!stdout_of(&output).contains("--watch"));
}

#[test]
fn unknown_init_flag_lists_declared_variables() {
    let repo = TempRepo::new("init-unknown", &["just"]);
    repo.write(
        "just/publish/schema.json",
        r#"{"variables": {"watch": {"__watch__": "true"}, "task": {"__task__": "build"}}}"#,
    );

    let output = run(repo.path(), &["init", "just/publish", "--host"]);
    assert_ne!(exit_code(&output), 0, "unknown flags fail");
    let stderr = stderr_of(&output);
    assert!(stderr.contains("--host"), "the flag is named: {stderr}");
    assert!(stderr.contains("--watch"), "variables listed: {stderr}");
    assert!(stderr.contains("--task"), "variables listed: {stderr}");

    let typo = run(repo.path(), &["init", "just/publish", "--tatsk"]);
    assert_ne!(exit_code(&typo), 0);
    assert!(stderr_of(&typo).contains("did you mean '--task'?"));
}

#[test]
fn init_copies_everything_except_schema_and_applies_dynamic_flags() {
    let repo = TempRepo::new("init-copy", &["rust"]);
    repo.write(
        "rust/min/schema.json",
        r#"{"variables": {"name": {"__name__": "default"}, "watch": {"__watch__": "false"}}}"#,
    );
    repo.write(
        "rust/min/Cargo.toml",
        "name = \"__name__\"\nwatch = __watch__\n",
    );
    repo.write("rust/min/src/main.rs", "fn main() {}\n");
    repo.write("rust/min/README.md", "included in the full copy\n");
    let dest = repo.path().join("demo");

    let output = run(
        repo.path(),
        &[
            "init",
            "rust/min",
            dest.to_str().expect("utf8"),
            "--name",
            "demo",
            "--watch",
            "true",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_of(&output));

    let cargo = std::fs::read_to_string(dest.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("name = \"demo\""));
    assert!(cargo.contains("watch = true"));
    assert!(dest.join("src/main.rs").is_file());
    assert!(dest.join("README.md").is_file());
    assert!(!dest.join("schema.json").exists());
}

#[test]
fn init_without_destination_copies_into_the_current_directory() {
    let repo = TempRepo::new("init-current-source", &["just"]);
    repo.write("just/publish/schema.json", r#"{"variables": {}}"#);
    repo.write("just/publish/Justfile", "all:\n    echo ok\n");
    let cwd = TempRepo::new("init-current-destination", &[]);

    let output = run_in(repo.path(), cwd.path(), &["init", "just/publish"]);

    assert_eq!(exit_code(&output), 0, "{}", stderr_of(&output));
    assert_eq!(
        std::fs::read_to_string(cwd.path().join("Justfile")).unwrap(),
        "all:\n    echo ok\n"
    );
    assert!(!cwd.path().join("schema.json").exists());
}

#[test]
fn init_selects_the_group_named_template_by_default() {
    let repo = TempRepo::new("init-default-template", &["just"]);
    repo.write(
        "just/just/schema.json",
        r#"{"variables":{"name":{"__name__":"default"}}}"#,
    );
    repo.write("just/just/marker.txt", "default template: __name__\n");
    repo.write("just/publish/marker.txt", "publish template\n");
    let destination = repo.path().join("output");

    let output = run(
        repo.path(),
        &[
            "init",
            "just",
            destination.to_str().expect("utf8"),
            "--name",
            "ola",
        ],
    );

    assert_eq!(exit_code(&output), 0, "{}", stderr_of(&output));
    assert_eq!(
        std::fs::read_to_string(destination.join("marker.txt")).unwrap(),
        "default template: ola\n"
    );
}

#[test]
fn schema_selects_the_only_template_by_default() {
    let repo = TempRepo::new("schema-default-template", &["single"]);
    let contents = "{\"variables\":{}}\n";
    repo.write("single/custom/schema.json", contents);

    let output = run(repo.path(), &["schema", "single"]);

    assert_eq!(exit_code(&output), 0, "{}", stderr_of(&output));
    assert_eq!(stdout_of(&output), contents);
}

#[test]
fn schema_accepts_a_trailing_slash_after_the_group() {
    let repo = TempRepo::new("schema-default-trailing-slash", &["just"]);
    let contents = "{\"variables\":{\"name\":{\"__name__\":\"default\"}}}\n";
    repo.write("just/just/schema.json", contents);
    repo.write("just/publish/schema.json", "{\"variables\":{}}\n");

    let output = run(repo.path(), &["schema", "just/"]);

    assert_eq!(exit_code(&output), 0, "{}", stderr_of(&output));
    assert_eq!(stdout_of(&output), contents);
}

#[test]
fn init_creates_nested_destination_directories() {
    let repo = TempRepo::new("init-nested-source", &["just"]);
    repo.write("just/publish/nested/Justfile", "all:\n    echo ok\n");
    let parent = TempRepo::new("init-nested-destination", &[]);
    let destination = parent.path().join("demo1/demo2");

    let output = run(
        repo.path(),
        &["init", "just/publish", destination.to_str().expect("utf8")],
    );

    assert_eq!(exit_code(&output), 0, "{}", stderr_of(&output));
    assert_eq!(
        std::fs::read_to_string(destination.join("nested/Justfile")).unwrap(),
        "all:\n    echo ok\n"
    );
}

#[test]
fn init_refuses_an_existing_explicit_destination() {
    let repo = TempRepo::new("init-existing-source", &["just"]);
    repo.write("just/publish/Justfile", "new\n");
    let destination = TempRepo::new("init-existing-destination", &[]);
    std::fs::write(destination.path().join("existing"), "keep\n").unwrap();

    let output = run(
        repo.path(),
        &[
            "init",
            "just/publish",
            destination.path().to_str().expect("utf8"),
        ],
    );

    assert_ne!(exit_code(&output), 0);
    assert!(stderr_of(&output).contains("already exists"));
    assert_eq!(
        std::fs::read_to_string(destination.path().join("existing")).unwrap(),
        "keep\n"
    );
    assert!(!destination.path().join("Justfile").exists());
}
