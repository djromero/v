use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

#[test]
fn bootstraps_a_repository_with_a_version_subject_and_message_body() {
    let root = TempDir::new().unwrap();
    let home = root.path().join("home");
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("README.md"), "hello\n").unwrap();
    fs::write(project.join(".gitignore"), "__xplain/\n").unwrap();
    fs::write(project.join(".private"), "not added by *\n").unwrap();
    fs::create_dir(project.join("__xplain")).unwrap();
    fs::write(project.join("__xplain/details.md"), "ignored\n").unwrap();

    let output = v(&project, &home, ["experimental", "launcher"]);
    assert!(output.status.success(), "{}", diagnostics(&output));
    assert!(project.join(".git").is_dir());

    let message = git(&project, ["log", "-1", "--format=%s%n%b"]);
    assert!(message.status.success(), "{}", diagnostics(&message));
    assert_eq!(
        String::from_utf8(message.stdout).unwrap(),
        "v0\nexperimental launcher\n\n"
    );

    let files = git(&project, ["ls-files"]);
    assert!(files.status.success(), "{}", diagnostics(&files));
    assert_eq!(
        String::from_utf8(files.stdout).unwrap(),
        ".gitignore\nREADME.md\n"
    );
}

fn v<const N: usize>(directory: &Path, home: &Path, arguments: [&str; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_v"))
        .args(arguments)
        .current_dir(directory)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "V Test")
        .env("GIT_AUTHOR_EMAIL", "v@example.test")
        .env("GIT_COMMITTER_NAME", "V Test")
        .env("GIT_COMMITTER_EMAIL", "v@example.test")
        .output()
        .unwrap()
}

fn git<const N: usize>(directory: &Path, arguments: [&str; N]) -> Output {
    Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

fn diagnostics(output: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
