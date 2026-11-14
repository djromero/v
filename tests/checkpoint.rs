use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

#[test]
fn creates_a_checkpoint_from_the_repository_root() {
    let fixture = Repository::bootstrap();
    fs::write(fixture.project.join("README.md"), "root checkpoint\n").unwrap();

    let output = fixture.v(&fixture.project, ["root", "message"]);
    assert!(output.status.success(), "{}", diagnostics(&output));

    assert_eq!(fixture.latest_message(), "v1\nroot message\n\n");
    assert_eq!(fixture.head_file("README.md"), "root checkpoint\n");
}

#[test]
fn creates_a_checkpoint_from_a_nested_directory() {
    let fixture = Repository::bootstrap();
    let nested = fixture.project.join("notes");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("idea.txt"), "nested checkpoint\n").unwrap();

    let output = fixture.v(&nested, ["nested", "message"]);
    assert!(output.status.success(), "{}", diagnostics(&output));

    assert_eq!(fixture.latest_message(), "v1\nnested message\n\n");
    assert_eq!(fixture.head_file("notes/idea.txt"), "nested checkpoint\n");
}

#[test]
fn commits_gitignore_without_staging_ignored_paths() {
    let fixture = Repository::bootstrap();
    fs::write(fixture.project.join(".gitignore"), "__xplain/\n").unwrap();
    let ignored = fixture.project.join("__xplain");
    fs::create_dir(&ignored).unwrap();
    fs::write(ignored.join("details.md"), "ignored\n").unwrap();

    let output = fixture.v(&fixture.project, []);
    assert!(output.status.success(), "{}", diagnostics(&output));

    assert_eq!(fixture.latest_message(), "v1\n\n");
    assert_eq!(fixture.head_file(".gitignore"), "__xplain/\n");
    assert_eq!(fixture.git_text(["ls-files", "__xplain"]), "");
}

#[test]
fn commits_deleted_files_and_directories_with_the_next_version() {
    for path in ["obsolete.txt", "obsolete/nested.txt", ".gitignore"] {
        let fixture = Repository::bootstrap();
        let file = fixture.project.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "remove me\n").unwrap();
        let added = fixture.v(&fixture.project, []);
        assert!(added.status.success(), "{}", diagnostics(&added));

        if path.starts_with("obsolete/") {
            fs::remove_dir_all(file.parent().unwrap()).unwrap();
        } else {
            fs::remove_file(&file).unwrap();
        }

        let output = fixture.v(&fixture.project, ["remove", "obsolete", "content"]);
        assert!(output.status.success(), "{}", diagnostics(&output));
        assert_eq!(fixture.latest_message(), "v2\nremove obsolete content\n\n");
        assert_eq!(fixture.git_text(["ls-files", "--", path]), "");
        assert_eq!(fixture.git_text(["status", "--porcelain"]), "");
        assert_eq!(
            fixture.git_text(["diff-tree", "--no-commit-id", "--name-status", "-r", "HEAD"]),
            format!("D\t{path}\n")
        );
    }
}

#[test]
fn commits_deletions_in_an_empty_nested_directory_without_staging_outside_changes() {
    let fixture = Repository::bootstrap();
    let nested = fixture.project.join("notes");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("idea.txt"), "temporary\n").unwrap();
    let added = fixture.v(&nested, []);
    assert!(added.status.success(), "{}", diagnostics(&added));

    fs::remove_file(nested.join("idea.txt")).unwrap();
    fs::write(nested.join(".private"), "keep private\n").unwrap();
    fs::write(fixture.project.join("README.md"), "outside edit\n").unwrap();
    fs::write(fixture.project.join("outside.txt"), "outside addition\n").unwrap();

    let output = fixture.v(&nested, []);
    assert!(output.status.success(), "{}", diagnostics(&output));
    assert_eq!(fixture.latest_message(), "v2\n\n");
    assert_eq!(fixture.git_text(["ls-files", "--", "notes"]), "");
    assert_eq!(fixture.head_file("README.md"), "initial\n");
    assert_eq!(fixture.git_text(["ls-files", "--", "outside.txt"]), "");
    assert_eq!(fixture.git_text(["diff", "--cached", "--name-only"]), "");
}

#[test]
fn uses_the_commit_count_when_the_latest_subject_is_not_a_version() {
    let fixture = Repository::bootstrap();
    let manual = fixture.git(["commit", "--allow-empty", "-m", "manual checkpoint"]);
    assert!(manual.status.success(), "{}", diagnostics(&manual));
    fs::write(
        fixture.project.join("README.md"),
        "checkpoint after manual commit\n",
    )
    .unwrap();

    let output = fixture.v(&fixture.project, []);
    assert!(output.status.success(), "{}", diagnostics(&output));

    assert_eq!(fixture.latest_message(), "v3\n\n");
    assert_eq!(fixture.git_text(["rev-list", "--count", "HEAD"]), "3\n");
}

struct Repository {
    _root: TempDir,
    home: std::path::PathBuf,
    project: std::path::PathBuf,
}

impl Repository {
    fn bootstrap() -> Self {
        let root = TempDir::new().unwrap();
        let home = root.path().join("home");
        let project = home.join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("README.md"), "initial\n").unwrap();

        let fixture = Self {
            _root: root,
            home,
            project,
        };
        let output = fixture.v(&fixture.project, []);
        assert!(output.status.success(), "{}", diagnostics(&output));
        fixture
    }

    fn v<const N: usize>(&self, directory: &Path, arguments: [&str; N]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_v"))
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

    fn latest_message(&self) -> String {
        let output = self.git(["log", "-1", "--format=%s%n%b"]);
        assert!(output.status.success(), "{}", diagnostics(&output));
        String::from_utf8(output.stdout).unwrap()
    }

    fn head_file(&self, path: &str) -> String {
        let specification = format!("HEAD:{path}");
        let output = self.git(["show", &specification]);
        assert!(output.status.success(), "{}", diagnostics(&output));
        String::from_utf8(output.stdout).unwrap()
    }

    fn git<const N: usize>(&self, arguments: [&str; N]) -> Output {
        Command::new("git")
            .args(arguments)
            .current_dir(&self.project)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "V Test")
            .env("GIT_AUTHOR_EMAIL", "v@example.test")
            .env("GIT_COMMITTER_NAME", "V Test")
            .env("GIT_COMMITTER_EMAIL", "v@example.test")
            .output()
            .unwrap()
    }

    fn git_text<const N: usize>(&self, arguments: [&str; N]) -> String {
        let output = self.git(arguments);
        assert!(output.status.success(), "{}", diagnostics(&output));
        String::from_utf8(output.stdout).unwrap()
    }
}

fn diagnostics(output: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
