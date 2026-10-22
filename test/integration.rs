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
        .env("SCAFFOLD", repo)
        .output()
        .expect("spawn binary")
}

fn run_without_scaffold(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env_remove("SCAFFOLD")
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
fn version_flag_prints_package_version() {
    let output = run(Path::new("."), &["-v"]);
    assert!(output.status.success());
    assert_eq!(stdout_of(&output).trim_end(), EXPECTED_VERSION);
}

#[test]
fn help_flag_lists_available_verbs() {
    let output = run(Path::new("."), &["-h"]);
    assert!(output.status.success());
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("list"),
        "help should mention listing: {stdout}"
    );
    assert!(stdout.contains("Usage"), "help should show usage: {stdout}");
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
fn list_group_prints_templates_through_both_verb_forms() {
    let repo = TempRepo::new("list-template", &["just"]);
    repo.write("just/vim/placeholder", "-");
    repo.write("just/just/placeholder", "-");
    repo.write("just/loose.txt", "ignored file, not template\n");

    let verb = run(repo.path(), &["list", "just"]);
    assert_eq!(exit_code(&verb), 0);
    assert_eq!(stdout_of(&verb), "just\nvim\n");

    let alias = run(repo.path(), &["-l", "just"]);
    assert_eq!(exit_code(&alias), 0);
    assert_eq!(stdout_of(&alias), "just\nvim\n");
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

    let output = run_without_scaffold(&["list"]);
    assert_ne!(exit_code(&output), 0);
    assert!(stderr_of(&output).contains("SCAFFOLD"));
    assert!(!probe.exists(), "a missing env must not create anything");
}

#[test]
fn init_help_lists_the_schema_dynamic_flags() {
    let repo = TempRepo::new("init-help", &["just"]);
    repo.write(
        "just/just/schema.json",
        r#"{"variables": {"watch": {"__watch__": "true"}, "task": {"__task__": "build"}}}"#,
    );

    let output = run(repo.path(), &["init", "just", "--help"]);
    assert_eq!(exit_code(&output), 0, "help exits successfully");
    let stdout = stdout_of(&output);
    assert!(stdout.contains("--watch"), "dynamic flag listed: {stdout}");
    assert!(stdout.contains("--task"), "dynamic flag listed: {stdout}");
    assert!(
        stdout.contains("--name"),
        "native name still listed: {stdout}"
    );
}

#[test]
fn unknown_init_flag_lists_declared_variables() {
    let repo = TempRepo::new("init-unknown", &["just"]);
    repo.write(
        "just/just/schema.json",
        r#"{"variables": {"watch": {"__watch__": "true"}, "task": {"__task__": "build"}}}"#,
    );

    let output = run(repo.path(), &["init", "just", "--host"]);
    assert_ne!(exit_code(&output), 0, "unknown flags fail");
    let stderr = stderr_of(&output);
    assert!(stderr.contains("--host"), "the flag is named: {stderr}");
    assert!(stderr.contains("--watch"), "variables listed: {stderr}");
    assert!(stderr.contains("--task"), "variables listed: {stderr}");
    assert!(!stderr.contains("did you mean"), "no false hint: {stderr}");

    let typo = run(repo.path(), &["init", "just", "--tatsk"]);
    assert_ne!(exit_code(&typo), 0);
    assert!(
        stderr_of(&typo).contains("did you mean '--task'?"),
        "typo hint present: {}",
        stderr_of(&typo)
    );
}

#[test]
fn full_happy_path_with_dynamic_flags_and_schema() {
    let repo = TempRepo::new("happy", &["rust"]);
    repo.write(
        "rust/min/schema.json",
        r#"{
  "include": ["Cargo.toml", "src/"],
  "variables": {
    "name": {"__name__": "min_package"},
    "watch": {"__watch__": "false"},
    "task": {"__task__": ""}
  }
}"#,
    );
    repo.write(
        "rust/min/Cargo.toml",
        "[package]\nname = \"__name__\"\nwatch = __watch__\n",
    );
    repo.write("rust/min/src/main.rs", "fn main() { __task__(); }\n");
    repo.write("rust/min/README.md", "not exported\n");
    let dest = repo.path().join("demo");
    // The spec's mode rule: a destination is a project with a trailing `/`.
    let dest_flag = format!("{}/", dest.display());

    let output = run(
        repo.path(),
        &[
            "init",
            "rust",
            "--name",
            "demo",
            dest_flag.as_str(),
            "--watch",
            "true",
            "--task",
            "build",
        ],
    );
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);
    assert_eq!(exit_code(&output), 0, "succeeds: {stdout} {stderr}");

    let cargo = std::fs::read_to_string(dest.join("Cargo.toml")).expect("cargo written");
    assert!(cargo.contains("name = \"demo\""), "name bound: {cargo}");
    assert!(cargo.contains("watch = true"), "watch overridden: {cargo}");
    let main = std::fs::read_to_string(dest.join("src/main.rs")).expect("main written");
    assert_eq!(main, "fn main() { build(); }\n");
    assert!(!dest.join("README.md").exists(), "whitelist held");
    assert!(!dest.join("schema.json").exists(), "schema never ships");
    assert_eq!(stdout, "", "quiet success");
}

#[test]
fn file_mode_writes_a_single_exported_file() {
    let repo = TempRepo::new("filemode", &["flake"]);
    repo.write(
        "flake/flake/schema.json",
        r#"{"variables": {"name": {"__name__": "default_name"}}}"#,
    );
    repo.write(
        "flake/flake/flake.nix",
        "description = \"__name__\";\noutputs = {{ }};\n",
    );
    let dest = repo.path().join("nested/deeper/flake.nix");

    let output = run(
        repo.path(),
        &[
            "init",
            "flake",
            "--name",
            "nombre",
            dest.to_str().expect("utf8"),
        ],
    );
    let stderr = stderr_of(&output);
    assert_eq!(exit_code(&output), 0, "succeeds: {stderr}");
    let content = std::fs::read_to_string(&dest).expect("file written");
    assert_eq!(
        content, "description = \"nombre\";\noutputs = {{ }};\n",
        "single file substituted at the exact path"
    );
}

#[test]
fn file_mode_refuses_template_with_several_exports() {
    let repo = TempRepo::new("filemode-multi", &["flake"]);
    repo.write("flake/flake/flake.nix", "a\n");
    repo.write("flake/flake/README.md", "b\n");

    let output = run(
        repo.path(),
        &[
            "init",
            "flake",
            repo.path().join("one.nix").to_str().expect("utf8"),
        ],
    );
    assert_ne!(exit_code(&output), 0);
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("exactly one"),
        "explains the rule: {stderr}"
    );
    assert!(stderr.contains("2"), "reports the export count: {stderr}");
    assert!(
        !repo.path().join("one.nix").exists(),
        "failure writes nothing"
    );
}

#[test]
fn list_group_with_several_templates_prints_sorted() {
    let repo = TempRepo::new("list-multi", &["rust"]);
    repo.write("rust/base/placeholder", "-");
    repo.write("rust/min/placeholder", "-");

    let group_view = run(repo.path(), &["list", "rust"]);
    assert_eq!(exit_code(&group_view), 0);
    assert_eq!(stdout_of(&group_view), "base\nmin\n");

    let alias = run(repo.path(), &["-l", "rust"]);
    assert_eq!(exit_code(&alias), 0);
    assert_eq!(stdout_of(&alias), "base\nmin\n");
}

#[test]
fn init_flag_takes_precedence_over_the_composed_form() {
    let repo = TempRepo::new("precedence", &["rust"]);
    repo.write("rust/base/marker", "base\n");
    repo.write("rust/min/marker", "min\n");
    let dest = repo.path().join("picked");

    let composed = run(
        repo.path(),
        &["init", "rust/min", "-o", &format!("{}/", dest.display())],
    );
    assert_eq!(exit_code(&composed), 0);
    assert_eq!(
        std::fs::read_to_string(dest.join("marker")).unwrap(),
        "min\n",
        "composed positional selects min"
    );
    std::fs::remove_dir_all(&dest).expect("reset dest");

    let flagged = run(
        repo.path(),
        &[
            "init",
            "rust/min",
            "-t",
            "base",
            "-o",
            &format!("{}/", dest.display()),
        ],
    );
    assert_eq!(exit_code(&flagged), 0);
    assert_eq!(
        std::fs::read_to_string(dest.join("marker")).unwrap(),
        "base\n",
        "-t overrides the composed selection"
    );
}

#[test]
fn init_refuses_conflicts_and_force_overwrites() {
    let repo = TempRepo::new("conflict-e2e", &["just"]);
    repo.write("just/just/Justfile", "new\n");
    let dest = repo.path().join("workspace");
    std::fs::create_dir_all(&dest).expect("existing destination");
    std::fs::write(dest.join("Justfile"), "old\n").expect("conflicting file");

    let refused = run(repo.path(), &["init", "just", dest.to_str().expect("utf8")]);
    assert_ne!(exit_code(&refused), 0);
    assert!(stderr_of(&refused).contains("--force"));
    assert_eq!(
        std::fs::read_to_string(dest.join("Justfile")).unwrap(),
        "old\n",
        "refusal keeps the original"
    );

    let forced = run(
        repo.path(),
        &["init", "just", dest.to_str().expect("utf8"), "--force"],
    );
    assert_eq!(exit_code(&forced), 0);
    assert_eq!(
        std::fs::read_to_string(dest.join("Justfile")).unwrap(),
        "new\n",
        "force overwrites"
    );
}
