use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    home: PathBuf,
    project: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let home = root.path().join("home");
        let project = home.join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("README.md"), "initial\n").unwrap();
        Self {
            _root: root,
            home,
            project,
        }
    }

    fn command(&self, executable: &str, directory: &Path, arguments: &[&str]) -> Output {
        Command::new(executable)
            .args(arguments)
            .current_dir(directory)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "V Test")
            .env("GIT_AUTHOR_EMAIL", "v@example.test")
            .env("GIT_COMMITTER_NAME", "V Test")
            .env("GIT_COMMITTER_EMAIL", "v@example.test")
            .output()
            .unwrap()
    }

    fn git(&self, directory: &Path, arguments: &[&str]) -> String {
        checked(self.command("git", directory, arguments))
    }

    fn v(&self, directory: &Path) {
        checked(self.command(env!("CARGO_BIN_EXE_v"), directory, &[]));
    }

    fn ordinary_repository(&self, directory: &Path) {
        self.git(directory, &["init"]);
        self.git(directory, &["add", "."]);
        self.git(directory, &["commit", "-m", "ordinary history"]);
    }

    fn nested(&self, parent: &Path) -> PathBuf {
        let nested = parent.join("notes");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("idea.txt"), "idea\n").unwrap();
        nested
    }
}

fn checked(output: Output) -> String {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn bootstrap_records_local_metadata_and_checkpoints_reuse_it() {
    let fixture = Fixture::new();
    fixture.v(&fixture.project);
    assert_eq!(
        fixture.git(
            &fixture.project,
            &["config", "--local", "--get", "v.created"]
        ),
        "true\n"
    );
    fs::write(fixture.project.join("README.md"), "changed\n").unwrap();
    fixture.v(&fixture.project);
    assert_eq!(
        fixture.git(&fixture.project, &["log", "--format=%s"]),
        "v1\nv0\n"
    );
    assert_eq!(fixture.git(&fixture.project, &["ls-files"]), "README.md\n");
}

#[test]
fn an_ordinary_ancestor_gets_a_separate_nested_repository() {
    let fixture = Fixture::new();
    fixture.ordinary_repository(&fixture.project);
    let original_head = fixture.git(&fixture.project, &["rev-parse", "HEAD"]);
    let nested = fixture.nested(&fixture.project);
    fixture.v(&nested);
    assert!(nested.join(".git").is_dir());
    assert_eq!(fixture.git(&nested, &["log", "--format=%s"]), "v0\n");
    assert_eq!(fixture.git(&nested, &["ls-files"]), "idea.txt\n");
    assert_eq!(
        fixture.git(&fixture.project, &["rev-parse", "HEAD"]),
        original_head
    );
    let parent_marker = fixture.command(
        "git",
        &fixture.project,
        &["config", "--local", "--get", "v.created"],
    );
    assert_eq!(parent_marker.status.code(), Some(1));
    fs::write(nested.join("idea.txt"), "changed\n").unwrap();
    fixture.v(&nested);
    assert_eq!(fixture.git(&nested, &["log", "--format=%s"]), "v1\nv0\n");
}

#[test]
fn a_marked_ancestor_is_reused_without_creating_a_nested_repository() {
    let fixture = Fixture::new();
    fixture.v(&fixture.project);
    let nested = fixture.nested(&fixture.project);
    fixture.v(&nested);
    assert!(!nested.join(".git").exists());
    assert_eq!(
        fixture.git(&fixture.project, &["log", "--format=%s"]),
        "v1\nv0\n"
    );
    assert_eq!(
        fixture.git(&fixture.project, &["show", "HEAD:notes/idea.txt"]),
        "idea\n"
    );
}

#[test]
fn an_unmarked_repository_shadows_a_marked_ancestor() {
    let fixture = Fixture::new();
    fixture.v(&fixture.project);
    let ordinary = fixture.nested(&fixture.project);
    fixture.ordinary_repository(&ordinary);
    let nested = fixture.nested(&ordinary);
    fixture.v(&nested);
    assert!(nested.join(".git").is_dir());
    assert_eq!(fixture.git(&nested, &["log", "--format=%s"]), "v0\n");
    assert_eq!(
        fixture.git(&ordinary, &["log", "--format=%s"]),
        "ordinary history\n"
    );
    assert_eq!(
        fixture.git(&fixture.project, &["log", "--format=%s"]),
        "v0\n"
    );
}

#[test]
fn global_metadata_does_not_mark_an_ordinary_repository() {
    let fixture = Fixture::new();
    fs::write(fixture.home.join(".gitconfig"), "[v]\ncreated = true\n").unwrap();
    fixture.ordinary_repository(&fixture.project);
    let nested = fixture.nested(&fixture.project);
    fixture.v(&nested);
    assert!(nested.join(".git").is_dir());
    assert_eq!(fixture.git(&nested, &["log", "--format=%s"]), "v0\n");
}

#[test]
fn an_ordinary_repository_root_counts_commits_without_initializing_or_marking_it() {
    let fixture = Fixture::new();
    fixture.ordinary_repository(&fixture.project);
    // A version-like subject must not override the ordinary repository's count.
    fixture.git(&fixture.project, &["commit", "--allow-empty", "-m", "v99"]);
    // Reinitializing would copy this newly configured template into .git.
    let template = fixture.home.join("template");
    fs::create_dir(&template).unwrap();
    fs::write(template.join("init-sentinel"), "must not be copied\n").unwrap();
    fixture.git(
        &fixture.project,
        &[
            "config",
            "--local",
            "init.templateDir",
            template.to_str().unwrap(),
        ],
    );
    fs::write(fixture.project.join("README.md"), "changed\n").unwrap();
    fixture.v(&fixture.project);
    assert_eq!(
        fixture.git(&fixture.project, &["log", "--format=%s"]),
        "v3\nv99\nordinary history\n"
    );
    assert!(!fixture.project.join(".git/init-sentinel").exists());
    let marker = fixture.command(
        "git",
        &fixture.project,
        &["config", "--local", "--get", "v.created"],
    );
    assert_eq!(marker.status.code(), Some(1));

    let clean = fixture.command(env!("CARGO_BIN_EXE_v"), &fixture.project, &[]);
    assert!(clean.status.success());
    assert_eq!(clean.stdout, "\x1b[34m✔\x1b[0m\n".as_bytes());
    assert!(clean.stderr.is_empty());
    fs::write(fixture.project.join("README.md"), "changed again\n").unwrap();
    fixture.v(&fixture.project);
    assert_eq!(
        fixture.git(&fixture.project, &["log", "--format=%s"]),
        "v4\nv3\nv99\nordinary history\n"
    );
    let nested = fixture.nested(&fixture.project);
    fixture.v(&nested);
    assert!(nested.join(".git").is_dir());
    assert_eq!(fixture.git(&nested, &["log", "--format=%s"]), "v0\n");
}

#[test]
fn a_marked_worktree_with_a_git_file_is_reused() {
    let fixture = Fixture::new();
    fixture.v(&fixture.project);
    let worktree = fixture.home.join("worktree");
    fixture.git(
        &fixture.project,
        &["worktree", "add", "-b", "notes", worktree.to_str().unwrap()],
    );
    assert!(worktree.join(".git").is_file());
    let nested = fixture.nested(&worktree);
    fixture.v(&nested);
    assert!(!nested.join(".git").exists());
    assert_eq!(fixture.git(&worktree, &["log", "--format=%s"]), "v1\nv0\n");
    assert_eq!(
        fixture.git(&fixture.project, &["log", "--format=%s"]),
        "v0\n"
    );
}
