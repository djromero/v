use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

#[test]
fn performs_the_demonstrated_zero_effort_workflow() {
    let root = TempDir::new().unwrap();
    let home = root.path().join("home");
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("README.md"), "v0 starts here\n").unwrap();

    let bootstrap = v(&project, &home, ["experimental", "launcher"]);
    assert!(bootstrap.status.success(), "{}", diagnostics(&bootstrap));

    let clean = v(&project, &home, []);
    assert!(clean.status.success());
    assert_eq!(clean.stdout, "\x1b[34m✔\x1b[0m\n".as_bytes());
    assert!(clean.stderr.is_empty());

    fs::write(project.join("README.md"), "v0 is done\n").unwrap();
    let checkpoint = v(&project, &home, []);
    assert!(checkpoint.status.success(), "{}", diagnostics(&checkpoint));

    assert_eq!(git_text(&project, ["rev-list", "--count", "HEAD"]), "2\n");
    assert_eq!(
        git_text(&project, ["log", "--reverse", "--format=%s"]),
        "v0\nv1\n"
    );
    assert_eq!(
        git_text(&project, ["show", "-s", "--format=%b", "HEAD~1"]),
        "experimental launcher\n\n"
    );
    assert_eq!(
        git_text(&project, ["show", "-s", "--format=%b", "HEAD"]),
        "\n"
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
