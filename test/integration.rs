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
