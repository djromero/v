use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

#[test]
fn a_clean_invocation_does_not_advance_the_version() {
    let root = TempDir::new().unwrap();
    let home = root.path().join("home");
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("README.md"), "initial\n").unwrap();

    let bootstrap = v(&project, &home);
    assert!(bootstrap.status.success(), "{}", diagnostics(&bootstrap));

    let clean = v(&project, &home);
    assert!(clean.status.success(), "{}", diagnostics(&clean));
    assert_eq!(clean.stdout, "\x1b[34m✔\x1b[0m\n".as_bytes());
    assert!(clean.stderr.is_empty());
    assert_eq!(git_text(&project, ["rev-list", "--count", "HEAD"]), "1\n");

    fs::write(project.join("README.md"), "changed\n").unwrap();
    let checkpoint = v(&project, &home);
    assert!(checkpoint.status.success(), "{}", diagnostics(&checkpoint));
    assert_eq!(git_text(&project, ["log", "--format=%s"]), "v1\nv0\n");
}

fn v(directory: &Path, home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_v"))
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

fn git_text<const N: usize>(directory: &Path, arguments: [&str; N]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", diagnostics(&output));
    String::from_utf8(output.stdout).unwrap()
}

fn diagnostics(output: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
