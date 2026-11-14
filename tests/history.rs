use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

struct Repository {
    _directory: TempDir,
    root: PathBuf,
    base: String,
    checkpoints: Vec<String>,
}

impl Repository {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let root = directory.path().to_owned();
        let mut repo = Self {
            _directory: directory,
            root,
            base: String::new(),
            checkpoints: Vec::new(),
        };
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "v.created", "true"]);
        fs::write(repo.root.join("file"), "base").unwrap();
        fs::write(repo.root.join(".gitignore"), "ignored\n").unwrap();
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "stable"]);
        repo.base = repo.head();
        for version in 0..3 {
            fs::write(repo.root.join("file"), format!("v{version}")).unwrap();
            repo.git(&["commit", "-am", &format!("v{version}")]);
            repo.checkpoints.push(repo.head());
        }
        repo
    }

    fn command(&self, program: &str, cwd: &Path, args: &[&str]) -> Output {
        Command::new(program)
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "V Test")
            .env("GIT_AUTHOR_EMAIL", "v@example.test")
            .env("GIT_COMMITTER_NAME", "V Test")
            .env("GIT_COMMITTER_EMAIL", "v@example.test")
            .output()
            .unwrap()
    }

    fn git(&self, args: &[&str]) -> String {
        let output = self.command("git", &self.root, args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .to_owned()
    }

    fn v(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_v"), &self.root, args)
    }

    fn succeeds(&self, args: &[&str], message: &str) {
        let output = self.v(args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout,
            format!("\x1b[32m✔\x1b[0m {message}\n").as_bytes()
        );
        assert!(output.stderr.is_empty());
        assert_eq!(self.git(&["status", "--porcelain"]), "");
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    fn snapshot(&self) -> (String, String, String, String) {
        (
            self.head(),
            self.git(&["write-tree"]),
            self.git(&["status", "--porcelain", "--ignored"]),
            self.git(&["reflog", "show", "--format=%H %gs"]),
        )
    }

    fn refuses(&self, args: &[&str], message: &str) {
        let before = self.snapshot();
        let file = fs::read(self.root.join("file")).unwrap();
        let output = self.v(args);
        assert!(
            !output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.starts_with("\x1b[31mｘ\x1b[0m"), "{error}");
        assert!(error.contains(message), "{error}");
        assert_eq!(before, self.snapshot());
        assert_eq!(file, fs::read(self.root.join("file")).unwrap());
    }
}

#[test]
fn undo_redo_are_linear_and_preserve_exact_commits() {
    let repo = Repository::new();
    for version in (0..3).rev() {
        repo.succeeds(&["--undo"], &format!("v{version} undone"));
        let expected = if version == 0 {
            &repo.base
        } else {
            &repo.checkpoints[version - 1]
        };
        assert_eq!(&repo.head(), expected);
    }
    repo.refuses(&["--undo"], "HEAD is not a checkpoint");
    for version in 0..3 {
        repo.succeeds(&["--redo"], &format!("v{version} restored"));
        assert_eq!(repo.head(), repo.checkpoints[version]);
        assert_eq!(
            fs::read_to_string(repo.root.join("file")).unwrap(),
            format!("v{version}")
        );
    }
    repo.refuses(&["--redo"], "nothing to redo");
    repo.succeeds(&["--undo"], "v2 undone");
    repo.succeeds(&["--redo"], "v2 restored");
}

#[test]
fn discard_keeps_the_existing_forward_tail() {
    let repo = Repository::new();
    repo.succeeds(&["--undo"], "v2 undone");
    repo.succeeds(&["--discard"], "v0..v1 discarded");
    assert_eq!(repo.head(), repo.base);
    assert_eq!(fs::read_to_string(repo.root.join("file")).unwrap(), "base");
    for version in 0..3 {
        repo.succeeds(&["--redo"], &format!("v{version} restored"));
        assert_eq!(repo.head(), repo.checkpoints[version]);
    }
    repo.succeeds(&["--discard"], "v0..v2 discarded");
    repo.refuses(&["--discard"], "HEAD is not a checkpoint");
}

#[test]
fn squash_preserves_the_tree_and_creates_an_ordinary_boundary() {
    let repo = Repository::new();
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    repo.git(&["branch", "other"]);
    repo.git(&["tag", "saved"]);
    repo.succeeds(
        &["--squash", "fix", "db query performance"],
        "squashed v0..v2 as \"fix db query performance\"",
    );
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]),
        "fix db query performance"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]), repo.base);
    assert_eq!(repo.git(&["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(repo.git(&["rev-parse", "other"]), repo.checkpoints[2]);
    assert_eq!(repo.git(&["rev-parse", "saved"]), repo.checkpoints[2]);
    repo.refuses(&["--undo"], "HEAD is not a checkpoint");
    repo.refuses(&["--redo"], "nothing to redo");
}

#[test]
fn new_commits_and_external_branch_changes_invalidate_redo() {
    let repo = Repository::new();
    repo.succeeds(&["--undo"], "v2 undone");
    let clean = repo.v(&[]);
    assert_eq!(clean.status.code(), Some(0));
    assert_eq!(clean.stdout, "\x1b[34m✔\x1b[0m\n".as_bytes());
    repo.succeeds(&["--redo"], "v2 restored");
    repo.succeeds(&["--undo"], "v2 undone");
    fs::write(repo.root.join("file"), "different timeline").unwrap();
    repo.succeeds(&[], "v2");
    repo.refuses(&["--redo"], "nothing to redo");
    repo.succeeds(&["--undo"], "v2 undone");
    let position = repo.head();
    repo.git(&["commit", "--allow-empty", "-m", "external"]);
    repo.git(&["reset", "--hard", &position]);
    repo.refuses(&["--redo"], "nothing to redo");
}

#[test]
fn branch_switching_preserves_branch_specific_redo() {
    let repo = Repository::new();
    repo.git(&["branch", "other", &repo.base]);
    repo.succeeds(&["--undo"], "v2 undone");
    repo.git(&["switch", "other"]);
    repo.refuses(&["--redo"], "nothing to redo");
    repo.git(&["switch", "main"]);
    repo.succeeds(&["--redo"], "v2 restored");
}

#[test]
fn dirty_repositories_are_never_cleaned_automatically() {
    for action in ["--undo", "--redo", "--discard", "--squash"] {
        for kind in ["unstaged", "staged", "untracked"] {
            let repo = Repository::new();
            match kind {
                "untracked" => fs::write(repo.root.join("extra"), "keep").unwrap(),
                _ => {
                    fs::write(repo.root.join("file"), "keep").unwrap();
                    if kind == "staged" {
                        repo.git(&["add", "file"]);
                    }
                }
            }
            let args = if action == "--squash" {
                vec![action, "message"]
            } else {
                vec![action]
            };
            repo.refuses(&args, "must be clean");
        }
    }
}

#[test]
fn ignored_files_cannot_be_overwritten_by_redo() {
    let repo = Repository::new();
    fs::write(repo.root.join("ignored"), "committed").unwrap();
    repo.git(&["add", "-f", "ignored"]);
    repo.git(&["commit", "-m", "v3"]);
    repo.succeeds(&["--undo"], "v3 undone");
    fs::write(repo.root.join("ignored"), "private").unwrap();
    repo.refuses(&["--redo"], "overwritten");
    assert_eq!(
        fs::read_to_string(repo.root.join("ignored")).unwrap(),
        "private"
    );
    fs::remove_file(repo.root.join("ignored")).unwrap();
    repo.succeeds(&["--redo"], "v3 restored");
}

#[test]
fn history_rejects_detached_and_merges_but_allows_root_squash() {
    let repo = Repository::new();
    repo.git(&["checkout", "--detach"]);
    repo.refuses(&["--undo"], "");
    repo.git(&["switch", "main"]);
    repo.git(&["branch", "other", &repo.base]);
    repo.git(&["switch", "other"]);
    repo.git(&["commit", "--allow-empty", "-m", "side"]);
    repo.git(&["switch", "main"]);
    repo.git(&["merge", "--no-ff", "other", "-m", "v3"]);
    repo.refuses(&["--undo"], "linear");
    repo.refuses(&["--discard"], "linear");
    repo.refuses(&["--squash", "message"], "linear");

    let repo = Repository::new();
    repo.git(&["checkout", "--orphan", "root"]);
    repo.git(&["commit", "-m", "v0"]);
    repo.refuses(&["--undo"], "no parent");
    repo.refuses(&["--discard"], "no parent");
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    repo.succeeds(&["--squash", "message"], "squashed v0..v0 as \"message\"");
    assert_eq!(repo.git(&["log", "-1", "--format=%P"]), "");
    assert_eq!(repo.git(&["rev-parse", "HEAD^{tree}"]), tree);
}

#[test]
fn history_from_a_subdirectory_uses_the_enclosing_ordinary_repository() {
    let repo = Repository::new();
    repo.git(&["config", "--unset", "v.created"]);
    let nested = repo.root.join("nested");
    fs::create_dir(&nested).unwrap();
    let output = repo.command(env!("CARGO_BIN_EXE_v"), &nested, &["--undo"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(repo.head(), repo.checkpoints[1]);
    assert!(!nested.join(".git").exists());
    fs::write(repo.root.join("file"), "outside dirty").unwrap();
    let output = repo.command(env!("CARGO_BIN_EXE_v"), &nested, &["--redo"]);
    assert!(!output.status.success());
    assert_eq!(repo.head(), repo.checkpoints[1]);
}

#[test]
fn ongoing_operations_and_locked_refs_leave_redo_unchanged() {
    let repo = Repository::new();
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_START",
        "rebase-apply",
        "rebase-merge",
        "sequencer",
    ] {
        let path = repo.root.join(".git").join(marker);
        fs::write(&path, &repo.base).unwrap();
        repo.refuses(&["--undo"], "unsupported repository state");
        fs::remove_file(path).unwrap();
    }
    let lock = repo.root.join(".git/refs/heads/main.lock");
    fs::write(&lock, "").unwrap();
    repo.refuses(&["--undo"], "cannot lock ref");
    fs::remove_file(lock).unwrap();
    repo.succeeds(&["--undo"], "v2 undone");
}

#[test]
fn squash_after_undo_invalidates_its_old_redo_path() {
    let repo = Repository::new();
    repo.succeeds(&["--undo"], "v2 undone");
    repo.succeeds(&["--squash", "fix"], "squashed v0..v1 as \"fix\"");
    repo.refuses(&["--redo"], "nothing to redo");
}

#[test]
fn invalid_arguments_do_not_touch_the_repository() {
    let repo = Repository::new();
    repo.refuses(&["--squash", "v3"], "checkpoint subject");
    repo.refuses(&["--squash"], "message");
    repo.refuses(&["--undo", "--redo"], "no arguments");
}

#[test]
fn failed_squash_and_expired_reflog_do_not_guess_a_redo_path() {
    let repo = Repository::new();
    repo.succeeds(&["--undo"], "v2 undone");
    repo.refuses(&["--squash", "v3"], "checkpoint subject");
    repo.succeeds(&["--redo"], "v2 restored");
    repo.succeeds(&["--undo"], "v2 undone");
    repo.git(&["reflog", "expire", "--expire=now", "--all"]);
    repo.refuses(&["--redo"], "nothing to redo");
}

#[cfg(unix)]
#[test]
fn rejected_ref_transaction_changes_neither_files_nor_redo() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Repository::new();
    let hook = repo.root.join(".git/hooks/reference-transaction");
    fs::write(&hook, "#!/bin/sh\nif [ \"$1\" = prepared ]; then\n echo 'transaction rejected' >&2\n exit 1\nfi\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    repo.refuses(&["--undo"], "transaction rejected");
    fs::remove_file(hook).unwrap();
    repo.succeeds(&["--undo"], "v2 undone");
    repo.succeeds(&["--redo"], "v2 restored");
}

#[test]
fn history_works_in_a_linked_worktree() {
    let repo = Repository::new();
    let directory = TempDir::new().unwrap();
    let linked = directory.path().join("linked");
    repo.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);
    let output = repo.command(env!("CARGO_BIN_EXE_v"), &linked, &["--undo"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(repo.head(), repo.checkpoints[2]);
    assert_eq!(fs::read_to_string(linked.join("file")).unwrap(), "v1");
    let output = repo.command(env!("CARGO_BIN_EXE_v"), &linked, &["--redo"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(linked.join("file")).unwrap(), "v2");
}

#[test]
fn single_checkpoint_squash_and_literal_checkpoint_flags() {
    let repo = Repository::new();
    repo.succeeds(&["--discard"], "v0..v2 discarded");
    repo.succeeds(&["--redo"], "v0 restored");
    repo.succeeds(
        &["--squash", "v0..v1 is just text"],
        "squashed v0..v0 as \"v0..v1 is just text\"",
    );
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]),
        "v0..v1 is just text"
    );
    fs::write(repo.root.join("file"), "literal flag message").unwrap();
    repo.succeeds(&["--", "--undo"], "v3");
    assert_eq!(repo.git(&["log", "-1", "--format=%b"]), "--undo");
}

#[test]
fn fresh_repository_undo_redo_undo_undo_then_root_squash() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_owned();
    let repo = Repository {
        _directory: directory,
        root,
        base: String::new(),
        checkpoints: Vec::new(),
    };
    for version in 0..5 {
        fs::write(
            repo.root.join(format!("file-{version}.txt")),
            format!("version {version}\n"),
        )
        .unwrap();
        repo.succeeds(&[], &format!("v{version}"));
    }
    let original_tip = repo.head();
    repo.succeeds(&["--undo"], "v4 undone");
    repo.succeeds(&["--redo"], "v4 restored");
    assert_eq!(repo.head(), original_tip);
    repo.succeeds(&["--undo"], "v4 undone");
    repo.succeeds(&["--undo"], "v3 undone");
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    repo.succeeds(
        &["--squash", "combine checkpoints"],
        "squashed v0..v2 as \"combine checkpoints\"",
    );
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1");
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]),
        "combine checkpoints"
    );
    assert_eq!(repo.git(&["log", "-1", "--format=%P"]), "");
    assert_eq!(repo.git(&["rev-parse", "HEAD^{tree}"]), tree);
    for version in 0..3 {
        assert_eq!(
            fs::read_to_string(repo.root.join(format!("file-{version}.txt"))).unwrap(),
            format!("version {version}\n")
        );
    }
    assert!(!repo.root.join("file-3.txt").exists());
    assert!(!repo.root.join("file-4.txt").exists());
    let squashed = repo.head();
    let redo = repo.v(&["--redo"]);
    assert_eq!(redo.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&redo.stderr).contains("nothing to redo"));
    assert_eq!(repo.head(), squashed);
}
