use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn v(directory: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_v"))
        .current_dir(directory)
        .env("HOME", directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "V Test")
        .env("GIT_AUTHOR_EMAIL", "v@example.test")
        .env("GIT_COMMITTER_NAME", "V Test")
        .env("GIT_COMMITTER_EMAIL", "v@example.test")
        .output()
        .unwrap()
}

#[test]
fn success_shows_only_a_green_checkmark_and_the_committed_version() {
    let directory = TempDir::new().unwrap();
    fs::write(directory.path().join("file"), "initial").unwrap();
    let initial = v(directory.path());
    assert_eq!(initial.status.code(), Some(0));
    assert_eq!(initial.stdout, "\x1b[32m✔\x1b[0m v0\n".as_bytes());
    assert!(initial.stderr.is_empty());

    fs::write(directory.path().join("file"), "changed").unwrap();
    let checkpoint = v(directory.path());
    assert_eq!(checkpoint.status.code(), Some(0));
    assert_eq!(checkpoint.stdout, "\x1b[32m✔\x1b[0m v1\n".as_bytes());
    assert!(checkpoint.stderr.is_empty());

    let clean = v(directory.path());
    assert_eq!(clean.status.code(), Some(0));
    assert_eq!(clean.stdout, "\x1b[34m✔\x1b[0m\n".as_bytes());
    assert!(clean.stderr.is_empty());
}

#[test]
fn empty_directory_shows_a_red_cross_and_an_uncolored_error() {
    let directory = TempDir::new().unwrap();
    let output = v(directory.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        "\x1b[31mｘ\x1b[0mdirectory is empty\n".as_bytes()
    );
}

#[cfg(unix)]
#[test]
fn git_failures_preserve_their_exit_code_and_error_message() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempDir::new().unwrap();
    let bin = directory.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let git = bin.join("git");
    fs::write(&git, "#!/bin/sh\necho 'git error' >&2\nexit 42\n").unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_v"))
        .current_dir(directory.path())
        .env("HOME", directory.path())
        .env("PATH", bin)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(42));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, "\x1b[31mｘ\x1b[0mgit error\n".as_bytes());
}

#[test]
fn help_and_its_alias_work_without_git_home_or_a_repository() {
    let directory = TempDir::new().unwrap();
    let mut previous = None;
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_v"))
            .arg(flag)
            .current_dir(directory.path())
            .env("PATH", "")
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("Usage: v [message...]"));
        assert!(help.contains("commit body"));
        assert!(help.contains("clean index and working tree"));
        for flags in [
            "-u, --undo",
            "-r, --redo",
            "-s, --squash",
            "-d, --discard",
            "-h, --help",
        ] {
            assert!(help.contains(flags), "missing {flags}");
        }
        assert!(!help.contains('\x1b'));
        assert!(!help.contains('✔'));
        if let Some(previous) = previous {
            assert_eq!(help, previous);
        }
        previous = Some(help);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
