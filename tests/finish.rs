use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

struct Workspace {
    _temp: TempDir,
    outer: PathBuf,
    inner: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let outer = temp.path().to_owned();
        let inner = outer.join("some folder/workspace");
        fs::create_dir_all(&inner).unwrap();
        let workspace = Self {
            _temp: temp,
            outer,
            inner,
        };
        workspace.git(&workspace.outer, &["init", "-b", "main"]);
        fs::write(workspace.outer.join("project"), "original").unwrap();
        fs::write(workspace.inner.join("old"), "delete later").unwrap();
        workspace.git(&workspace.outer, &["add", "."]);
        workspace.git(&workspace.outer, &["commit", "-m", "Project base"]);
        fs::write(workspace.inner.join("file"), "first").unwrap();
        workspace.v(&workspace.inner, &[]);
        workspace
    }

    fn command(&self, program: &str, cwd: &Path, args: &[&str]) -> Output {
        Command::new(program)
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.outer)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "V Test")
            .env("GIT_AUTHOR_EMAIL", "v@example.test")
            .env("GIT_COMMITTER_NAME", "V Test")
            .env("GIT_COMMITTER_EMAIL", "v@example.test")
            .output()
            .unwrap()
    }

    fn git(&self, cwd: &Path, args: &[&str]) -> String {
        checked(self.command("git", cwd, args))
    }

    fn v(&self, cwd: &Path, args: &[&str]) -> String {
        checked(self.command(env!("CARGO_BIN_EXE_v"), cwd, args))
    }

    fn refuse(&self, cwd: &Path, args: &[&str], error: &str) {
        let outer_head = self.git(&self.outer, &["rev-parse", "HEAD"]);
        let inner_head = self.git(&self.inner, &["rev-parse", "HEAD"]);
        let index = fs::read(self.outer.join(".git/index")).unwrap();
        let output = self.command(env!("CARGO_BIN_EXE_v"), cwd, args);
        assert!(!output.status.success(), "unexpected success: {:?}", output);
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(error),
            "{:?}",
            output
        );
        assert_eq!(outer_head, self.git(&self.outer, &["rev-parse", "HEAD"]));
        assert_eq!(inner_head, self.git(&self.inner, &["rev-parse", "HEAD"]));
        assert_eq!(index, fs::read(self.outer.join(".git/index")).unwrap());
        assert!(self.inner.join(".git").is_dir());
    }
}

fn checked(output: Output) -> String {
    assert!(output.status.success(), "{:?}", output);
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end_matches('\n')
        .to_owned()
}

#[test]
fn done_squashes_root_checkpoints_and_preserves_unrelated_outer_changes() {
    let w = Workspace::new();
    let base = w.git(&w.outer, &["rev-parse", "HEAD"]);
    fs::remove_file(w.inner.join("old")).unwrap();
    fs::write(w.inner.join("file"), b"second\0binary\xff").unwrap();
    w.v(&w.inner, &[]);
    fs::write(w.outer.join("project"), "staged").unwrap();
    w.git(&w.outer, &["add", "project"]);
    fs::write(w.outer.join("project"), "unstaged").unwrap();
    fs::write(w.outer.join("untracked"), "private").unwrap();
    w.v(&w.inner, &["--done", "Add", "flux capacitor"]);
    assert!(!w.inner.join(".git").exists());
    assert_eq!(
        w.git(&w.outer, &["log", "--format=%s"]),
        "Add flux capacitor\nProject base"
    );
    assert_eq!(w.git(&w.outer, &["rev-parse", "HEAD^"]), base);
    assert_eq!(w.git(&w.outer, &["show", "HEAD:project"]), "original");
    assert_eq!(w.git(&w.outer, &["show", ":project"]), "staged");
    assert_eq!(
        fs::read_to_string(w.outer.join("project")).unwrap(),
        "unstaged"
    );
    let file = w.command(
        "git",
        &w.outer,
        &["show", "HEAD:some folder/workspace/file"],
    );
    assert_eq!(file.stdout, b"second\0binary\xff");
    assert!(!w
        .git(&w.outer, &["ls-tree", "-r", "HEAD"])
        .contains("160000"));
    assert_eq!(
        w.git(&w.outer, &["diff", "HEAD", "--", "some folder/workspace"]),
        ""
    );
    assert_eq!(
        w.git(&w.outer, &["ls-files", "some folder/workspace/old"]),
        ""
    );
    let repeated = w.command(env!("CARGO_BIN_EXE_v"), &w.inner, &["--done"]);
    assert!(!repeated.status.success());
}

#[test]
fn done_preserves_multiple_named_commits_and_squashes_only_the_tail() {
    let w = Workspace::new();
    w.v(&w.inner, &["--squash", "First feature"]);
    fs::write(w.inner.join("file"), "second").unwrap();
    w.v(&w.inner, &[]);
    w.v(&w.inner, &["--squash", "Second feature"]);
    fs::write(w.inner.join("file"), "third").unwrap();
    w.v(&w.inner, &[]);
    w.refuse(&w.inner, &["--done"], "require --done <message>");
    w.v(&w.inner, &["--done", "Third feature"]);
    assert_eq!(
        w.git(&w.outer, &["log", "--format=%s"]),
        "Third feature\nSecond feature\nFirst feature\nProject base"
    );
    assert_eq!(
        w.git(&w.outer, &["show", "HEAD^:some folder/workspace/file"]),
        "second"
    );
    assert_eq!(w.git(&w.outer, &["status", "--porcelain"]), "");
}

#[test]
fn done_without_message_promotes_named_history_and_preserves_commit_body() {
    let w = Workspace::new();
    w.v(&w.inner, &["--squash", "First"]);
    w.git(
        &w.inner,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "Second",
            "-m",
            "Body with details",
        ],
    );
    let message = w.git(&w.inner, &["log", "-1", "--format=%B"]);
    w.v(&w.inner, &["--done"]);
    assert_eq!(w.git(&w.outer, &["log", "-1", "--format=%B"]), message);
    assert_eq!(w.git(&w.outer, &["rev-list", "--count", "HEAD"]), "3");
}

#[test]
fn done_and_discard_reject_dirty_local_repositories() {
    for action in ["--done", "--discard"] {
        for kind in ["staged", "unstaged", "untracked"] {
            let w = Workspace::new();
            let name = if kind == "untracked" { "extra" } else { "file" };
            fs::write(w.inner.join(name), "keep me").unwrap();
            if kind == "staged" {
                w.git(&w.inner, &["add", "file"]);
            }
            let args = if action == "--done" {
                vec![action, "Feature"]
            } else {
                vec![action]
            };
            w.refuse(&w.inner, &args, "must be clean");
            assert_eq!(fs::read_to_string(w.inner.join(name)).unwrap(), "keep me");
        }
    }
}

#[test]
fn done_requires_a_local_root_an_outer_repo_and_a_message_for_checkpoints() {
    let w = Workspace::new();
    for message in ["", " ", "v42", "invalid\nmessage"] {
        w.refuse(&w.inner, &["--done", message], "message");
    }
    w.refuse(&w.inner, &["--done"], "require --done <message>");
    w.refuse(
        &w.outer,
        &["--done", "Feature"],
        "root of a local v repository",
    );
    let sub = w.inner.join("sub");
    fs::create_dir(&sub).unwrap();
    w.refuse(&sub, &["--done", "Feature"], "root of a local v repository");
    w.git(&w.inner, &["config", "v.created", "false"]);
    w.refuse(
        &w.inner,
        &["--done", "Feature"],
        "root of a local v repository",
    );
    w.git(&w.inner, &["config", "v.created", "true"]);
    fs::remove_dir_all(w.inner.join(".git")).unwrap();
    w.git(&w.outer, &["add", "."]);
    w.git(&w.outer, &["commit", "-m", "Track workspace"]);
    w.git(&w.outer, &["config", "v.created", "true"]);
    let output = w.command(env!("CARGO_BIN_EXE_v"), &w.outer, &["--done", "Feature"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("enclosing Git repository"));
}

#[test]
fn done_does_not_publish_partial_history_on_a_lock_or_hook_failure() {
    let w = Workspace::new();
    w.v(&w.inner, &["--squash", "First"]);
    w.git(&w.inner, &["commit", "--allow-empty", "-m", "Second"]);
    let lock = w.outer.join(".git/refs/heads/main.lock");
    fs::write(&lock, "").unwrap();
    w.refuse(&w.inner, &["--done"], "cannot lock ref");
    fs::remove_file(lock).unwrap();
    let lock = w.outer.join(".git/index.lock");
    fs::write(&lock, "").unwrap();
    w.refuse(&w.inner, &["--done"], "exists");
    fs::remove_file(lock).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let hook = w.outer.join(".git/hooks/reference-transaction");
        fs::write(
            &hook,
            "#!/bin/sh\nif [ \"$1\" = prepared ]; then echo 'promotion rejected' >&2; exit 1; fi\n",
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        w.refuse(&w.inner, &["--done"], "promotion rejected");
        fs::remove_file(hook).unwrap();
    }
    w.v(&w.inner, &["--done"]);
    assert_eq!(w.git(&w.outer, &["rev-list", "--count", "HEAD"]), "3");
}

#[test]
fn discard_closes_root_checkpoints_named_heads_and_runs_with_a_base() {
    for state in ["v0", "root run", "named", "based run"] {
        let w = Workspace::new();
        let outer_head = w.git(&w.outer, &["rev-parse", "HEAD"]);
        if state == "named" || state == "based run" {
            w.v(&w.inner, &["--squash", "First"]);
        }
        if state == "root run" || state == "based run" {
            fs::write(w.inner.join("file"), "second").unwrap();
            w.v(&w.inner, &[]);
        }
        fs::write(w.inner.join(".git/info/exclude"), "ignored\n").unwrap();
        fs::write(w.inner.join("ignored"), "private").unwrap();
        let output = w.v(&w.inner, &["--discard"]);
        assert!(output.contains("removed local repository"));
        assert!(!w.inner.join(".git").exists());
        let expected = if state == "root run" {
            "second"
        } else {
            "first"
        };
        assert_eq!(fs::read_to_string(w.inner.join("file")).unwrap(), expected);
        assert_eq!(
            fs::read_to_string(w.inner.join("ignored")).unwrap(),
            "private"
        );
        assert_eq!(w.git(&w.outer, &["rev-parse", "HEAD"]), outer_head);
    }
}

#[cfg(unix)]
#[test]
fn promotion_preserves_modes_symlinks_and_unusual_paths() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let w = Workspace::new();
    let executable = w.inner.join("run\nscript");
    fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    symlink("file", w.inner.join("link")).unwrap();
    fs::write(w.inner.join(".gitignore"), "private\n").unwrap();
    fs::write(w.inner.join("private"), "do not promote").unwrap();
    w.v(&w.inner, &[]);
    w.v(&w.inner, &["--done", "Scripts"]);
    let tree = w.git(&w.outer, &["ls-tree", "-r", "HEAD"]);
    assert!(tree.contains("100755 blob"));
    assert!(tree.contains("120000 blob"));
    assert!(!tree.contains("workspace/private"));
    assert_eq!(
        fs::read_to_string(w.inner.join("private")).unwrap(),
        "do not promote"
    );
    assert_eq!(
        w.git(&w.outer, &["diff", "HEAD", "--", "some folder/workspace"]),
        ""
    );
}

#[test]
fn staged_outer_workspace_changes_are_rejected() {
    let w = Workspace::new();
    // Stage a workspace deletion in the outer index without changing local files.
    w.git(
        &w.outer,
        &[
            "update-index",
            "--force-remove",
            "some folder/workspace/old",
        ],
    );
    w.refuse(
        &w.inner,
        &["--done", "Feature"],
        "staged changes inside the workspace",
    );
}

#[test]
fn cleanup_respects_local_locks_and_shared_worktrees() {
    for action in ["--done", "--discard"] {
        let w = Workspace::new();
        w.v(&w.inner, &["--squash", "Feature"]);
        let lock = w.inner.join(".git/index.lock");
        fs::write(&lock, "").unwrap();
        w.refuse(&w.inner, &[action], "exists");
        fs::remove_file(lock).unwrap();
        let linked = w.outer.join("linked");
        w.git(
            &w.inner,
            &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
        );
        w.refuse(&w.inner, &[action], "shared with other worktrees");
        let output = w.command(env!("CARGO_BIN_EXE_v"), &linked, &[action]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("linked worktrees"));
    }
}

#[test]
fn done_rejects_detached_ongoing_and_nonlinear_history() {
    let w = Workspace::new();
    let branch = w.git(&w.inner, &["symbolic-ref", "--short", "HEAD"]);
    w.git(&w.inner, &["checkout", "--detach"]);
    w.refuse(&w.inner, &["--done", "Feature"], "attached local branch");
    w.git(&w.inner, &["switch", &branch]);
    fs::write(
        w.inner.join(".git/MERGE_HEAD"),
        w.git(&w.inner, &["rev-parse", "HEAD"]),
    )
    .unwrap();
    w.refuse(
        &w.inner,
        &["--done", "Feature"],
        "unsupported repository state",
    );
    fs::remove_file(w.inner.join(".git/MERGE_HEAD")).unwrap();
    w.v(&w.inner, &["--squash", "Feature"]);
    w.git(&w.inner, &["branch", "side"]);
    w.git(&w.inner, &["commit", "--allow-empty", "-m", "Main"]);
    w.git(&w.inner, &["switch", "side"]);
    w.git(&w.inner, &["commit", "--allow-empty", "-m", "Side"]);
    w.git(&w.inner, &["switch", &branch]);
    w.git(&w.inner, &["merge", "--no-ff", "side", "-m", "Merged"]);
    w.refuse(&w.inner, &["--done"], "linear local history");
}

#[test]
fn done_detects_an_already_published_workspace_after_cleanup_failure() {
    let w = Workspace::new();
    w.v(&w.inner, &["--squash", "Feature"]);
    // Simulate the durable publication receipt left before .git cleanup.
    fs::write(
        w.inner.join(".git/v-done"),
        w.git(&w.outer, &["rev-parse", "HEAD"]),
    )
    .unwrap();
    w.refuse(&w.inner, &["--done"], "already promoted");
}

#[test]
fn discard_from_a_local_subdirectory_closes_the_nearest_repository() {
    let w = Workspace::new();
    let sub = w.inner.join("sub");
    fs::create_dir(&sub).unwrap();
    w.v(&sub, &["--discard"]);
    assert!(!w.inner.join(".git").exists());
    assert!(w.outer.join(".git").is_dir());
    assert_eq!(fs::read_to_string(w.inner.join("file")).unwrap(), "first");
}
