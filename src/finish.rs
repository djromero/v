use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::git::{command_error, Git};
use crate::history::{validate_index, validate_repository, validate_state, RefTransaction};
use crate::version::is_version_subject;

pub(crate) fn is_local(git: &Git) -> io::Result<bool> {
    let output = git.output(["config", "--local", "--bool", "--get", "v.created"])?;
    match output.status.code() {
        Some(0) => Ok(output.stdout == b"true\n"),
        Some(1) => Ok(false),
        _ => Err(command_error(output)),
    }
}

pub(crate) fn validate_removable(root: &Path, git: &Git) -> io::Result<()> {
    let metadata = fs::symlink_metadata(root.join(".git"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::other(
            "cleanup requires a real local .git directory; linked worktrees are unsupported",
        ));
    }
    let actual = git.text(["rev-parse", "--absolute-git-dir"])?;
    if fs::canonicalize(actual)? != fs::canonicalize(root.join(".git"))? {
        return Err(io::Error::other(
            "Git directory does not match the local .git directory",
        ));
    }
    let worktrees = git.text(["worktree", "list", "--porcelain"])?;
    if worktrees
        .lines()
        .filter(|line| line.starts_with("worktree "))
        .count()
        != 1
    {
        return Err(io::Error::other(
            "cannot remove a repository shared with other worktrees",
        ));
    }
    Ok(())
}

pub(crate) fn remove_local(root: &Path) -> io::Result<String> {
    fs::remove_dir_all(root.join(".git"))?;
    Ok("removed local repository".to_owned())
}

/// Close a clean repository without racing an index or branch writer.
pub(crate) fn discard_local(
    root: &Path,
    git: &Git,
    branch: &str,
    head: &str,
) -> io::Result<String> {
    let mut index_lock = IndexLock::acquire(&root.join(".git/index"))?;
    let transaction = RefTransaction::prepare(git, branch, head, head, "v:discard-close")?;
    validate_removable(root, git)?;
    validate_repository(git)?;
    drop(transaction);
    let result = remove_local(root)?;
    index_lock.active = false;
    Ok(result)
}

fn attached(git: &Git) -> io::Result<String> {
    let output = git.output(["symbolic-ref", "--quiet", "HEAD"])?;
    if output.status.code() == Some(1) {
        return Err(io::Error::other("done requires an attached local branch"));
    }
    if !output.status.success() {
        return Err(command_error(output));
    }
    let branch = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let branch = branch.trim_end_matches('\n').to_owned();
    if !branch.starts_with("refs/heads/") {
        return Err(io::Error::other("done requires an attached local branch"));
    }
    Ok(branch)
}

struct Commit {
    tree: String,
    message: Vec<u8>,
    checkpoint: bool,
}

fn commits(git: &Git, message: Option<&str>) -> io::Result<Vec<Commit>> {
    let ids = git.text(["rev-list", "--reverse", "HEAD"])?;
    let mut previous = String::new();
    let mut commits = Vec::new();
    for id in ids.lines() {
        let parents = git.text(["show", "-s", "--format=%P", id])?;
        if parents != previous {
            return Err(io::Error::other("done requires linear local history"));
        }
        previous = id.to_owned();
        let tree_entries = git.text(["ls-tree", "-r", id])?;
        if tree_entries
            .lines()
            .any(|entry| entry.starts_with("160000 "))
        {
            return Err(io::Error::other(
                "done does not support submodules in local history",
            ));
        }
        let subject = git.text(["show", "-s", "--format=%s", id])?;
        let raw = git.output(["cat-file", "commit", id])?;
        if !raw.status.success() {
            return Err(command_error(raw));
        }
        let start = raw
            .stdout
            .windows(2)
            .position(|bytes| bytes == b"\n\n")
            .ok_or_else(|| io::Error::other("invalid commit message"))?
            + 2;
        commits.push(Commit {
            tree: git.text(["rev-parse", &format!("{id}^{{tree}}")])?,
            message: raw.stdout[start..].to_vec(),
            checkpoint: is_version_subject(&subject),
        });
    }
    let first_pending = commits
        .iter()
        .rposition(|commit| !commit.checkpoint)
        .map_or(0, |position| position + 1);
    if first_pending < commits.len() {
        let message = message
            .ok_or_else(|| io::Error::other("pending checkpoints require --done <message>"))?;
        let tree = commits.last().unwrap().tree.clone();
        commits.truncate(first_pending);
        commits.push(Commit {
            tree,
            message: message.as_bytes().to_vec(),
            checkpoint: false,
        });
    }
    if commits.iter().any(|commit| commit.checkpoint) {
        return Err(io::Error::other(
            "earlier unsquashed checkpoints precede a named commit; squash them before done",
        ));
    }
    Ok(commits)
}

fn input(git: &Git, args: &[&str], bytes: &[u8]) -> io::Result<String> {
    let mut child = git
        .command()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Drain output concurrently with input, including large path lists/messages.
    let mut stdin = child.stdin.take().unwrap();
    let bytes = bytes.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output()?;
    let written = writer
        .join()
        .map_err(|_| io::Error::other("cannot write Git input"))?;
    if !output.status.success() {
        return Err(command_error(output));
    }
    written?;
    String::from_utf8(output.stdout)
        .map(|text| text.trim_end_matches('\n').to_owned())
        .map_err(io::Error::other)
}

fn replace_subtree(git: &Git, prefix: &str, tree: &str) -> io::Result<()> {
    let scope = format!(":(top,literal){prefix}");
    let paths = git.output(["ls-files", "-z", "--", &scope])?;
    if !paths.status.success() {
        return Err(command_error(paths));
    }
    input(
        git,
        &["update-index", "--force-remove", "-z", "--stdin"],
        &paths.stdout,
    )?;
    git.run(["read-tree", &format!("--prefix={prefix}/"), tree])?;
    Ok(())
}

struct IndexLock {
    path: PathBuf,
    file: File,
    active: bool,
}

impl IndexLock {
    fn acquire(index: &Path) -> io::Result<Self> {
        let path = index.with_extension("lock");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            path,
            file,
            active: true,
        })
    }
}

impl Drop for IndexLock {
    fn drop(&mut self) {
        if self.active {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn done(message: Option<&str>) -> io::Result<String> {
    let root = std::env::current_dir()?;
    let local = Git::new(&root);
    if !root.join(".git").exists() || !is_local(&local)? {
        return Err(io::Error::other(
            "done requires the root of a local v repository",
        ));
    }
    validate_removable(&root, &local)?;
    validate_repository(&local)?;
    let local_branch = attached(&local)?;
    let local_head = local.text(["rev-parse", "HEAD"])?;
    let history = commits(&local, message)?;
    let parent = root
        .parent()
        .ok_or_else(|| io::Error::other("no outer repository"))?;
    let location = Git::new(parent);
    let outer_root = location
        .text(["rev-parse", "--show-toplevel"])
        .map_err(|_| io::Error::other("done requires an enclosing Git repository"))?;
    let outer_root = Path::new(&outer_root);
    let outer = Git::new(outer_root);
    validate_state(&outer)?;
    validate_index(&outer)?;
    let branch = attached(&outer)?;
    let head = outer.text(["rev-parse", "HEAD"])?;
    let prefix = root
        .strip_prefix(outer_root)
        .map_err(io::Error::other)?
        .to_str()
        .ok_or_else(|| io::Error::other("workspace path must be UTF-8"))?;
    let scope = format!(":(top,literal){prefix}");
    let staged = outer.output(["diff", "--cached", "--quiet", &head, "--", &scope])?;
    if !staged.status.success() {
        return Err(io::Error::other(
            "outer index has staged changes inside the workspace",
        ));
    }
    let journal = root.join(".git/v-done");
    if let Ok(target) = fs::read_to_string(&journal) {
        if outer
            .output(["merge-base", "--is-ancestor", target.trim(), &head])?
            .status
            .success()
        {
            return Err(io::Error::other(
                "workspace was already promoted; remove its local .git to finish cleanup",
            ));
        }
    }
    let index = PathBuf::from(outer.text([
        "rev-parse",
        "--path-format=absolute",
        "--git-path",
        "index",
    ])?);
    let mut index_lock = IndexLock::acquire(&index)?;
    let mut local_index_lock = IndexLock::acquire(&root.join(".git/index"))?;
    let local_transaction =
        RefTransaction::prepare(&local, &local_branch, &local_head, &local_head, "v:done")?;
    validate_repository(&local)?;
    validate_state(&outer)?;
    validate_index(&outer)?;
    let staged = outer.output(["diff", "--cached", "--quiet", &head, "--", &scope])?;
    match staged.status.code() {
        Some(0) => {}
        Some(1) => {
            return Err(io::Error::other(
                "outer index has staged changes inside the workspace",
            ))
        }
        _ => return Err(command_error(staged)),
    }
    // Objects are copied without creating refs or FETCH_HEAD in the outer repo.
    outer.run([
        "fetch",
        "--no-tags",
        "--no-write-fetch-head",
        "--no-recurse-submodules",
        root.to_str().unwrap(),
        &local_head,
    ])?;
    let temporary = tempfile::tempdir_in(index.parent().unwrap())?;
    let planned_index = temporary.path().join("planned-index");
    let planning = Git::with_index(outer_root, &planned_index);
    planning.run(["read-tree", &head])?;
    let mut tip = head.clone();
    for commit in &history {
        replace_subtree(&planning, prefix, &commit.tree)?;
        let tree = planning.text(["write-tree"])?;
        tip = input(
            &outer,
            &["commit-tree", &tree, "-p", &tip, "-F", "-"],
            &commit.message,
        )?;
    }
    // Preserve the actual index outside the promoted subtree, including staged work.
    let final_index = temporary.path().join("final-index");
    if index.exists() {
        fs::copy(&index, &final_index)?;
    }
    let final_git = Git::with_index(outer_root, &final_index);
    if !final_index.exists() {
        final_git.run(["read-tree", "--empty"])?;
    }
    replace_subtree(&final_git, prefix, &history.last().unwrap().tree)?;
    index_lock.file.write_all(&fs::read(final_index)?)?;
    index_lock.file.sync_all()?;
    let mut transaction = RefTransaction::prepare(&outer, &branch, &head, &tip, "v:done")?;
    validate_repository(&local)?;
    fs::write(&journal, &tip)?;
    transaction.commit()?;
    fs::rename(&index_lock.path, &index).map_err(|error| {
        io::Error::other(format!(
            "promotion succeeded, but outer index update failed: {error}; do not repeat promotion"
        ))
    })?;
    index_lock.active = false;
    drop(local_transaction);
    remove_local(&root).map_err(|error| {
        io::Error::other(format!(
            "promotion succeeded, but local .git cleanup failed: {error}; do not repeat promotion"
        ))
    })?;
    local_index_lock.active = false;
    Ok(format!(
        "promoted {} commit(s) and removed local repository",
        history.len()
    ))
}
