use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Output, Stdio};
use std::thread::{self, JoinHandle};

use crate::cli::Arguments;
use crate::git::{command_error, Git};
use crate::version::is_version_subject;

struct Commit {
    id: String,
    subject: String,
    parents: Vec<String>,
}

impl Commit {
    fn read(git: &Git, id: &str) -> io::Result<Self> {
        let text = git.text(["show", "--no-patch", "--format=%H%n%P%n%s", id, "--"])?;
        let mut lines = text.splitn(3, '\n');
        Ok(Self {
            id: lines.next().unwrap_or_default().to_owned(),
            parents: lines
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            subject: lines.next().unwrap_or_default().to_owned(),
        })
    }

    fn parent(&self) -> io::Result<&str> {
        match self.parents.as_slice() {
            [parent] => Ok(parent),
            [] => Err(io::Error::other("checkpoint has no parent to restore")),
            _ => Err(io::Error::other("checkpoint history must be linear")),
        }
    }
}

pub fn run(action: &Arguments) -> io::Result<String> {
    // Git itself resolves the nearest enclosing repository, including worktrees.
    let location = Git::new(&std::env::current_dir()?);
    let root = location.text(["rev-parse", "--show-toplevel"])?;
    let git = Git::new(Path::new(&root));
    let reference = git.output(["symbolic-ref", "--quiet", "HEAD"])?;
    if reference.status.code() == Some(1) {
        return Err(io::Error::other(
            "history commands require an attached local branch",
        ));
    }
    if !reference.status.success() {
        return Err(command_error(reference));
    }
    let branch = String::from_utf8(reference.stdout)
        .map_err(io::Error::other)?
        .trim_end_matches('\n')
        .to_owned();
    if !branch.starts_with("refs/heads/") {
        return Err(io::Error::other(
            "history commands require an attached local branch",
        ));
    }
    let head = Commit::read(&git, "HEAD")?;
    validate_repository(&git)?;
    let local_discard = matches!(action, Arguments::Discard) && crate::finish::is_local(&git)?;
    if local_discard {
        crate::finish::validate_removable(Path::new(&root), &git)?;
        if !is_version_subject(&head.subject) || run_base(&git, &head)?.0.is_none() {
            return crate::finish::discard_local(Path::new(&root), &git, &branch, &head.id);
        }
    }
    let record = git.text(["reflog", "show", "-1", "--format=%H %gs", &branch, "--"])?;

    let (target, reason, message) = match action {
        Arguments::Undo => {
            if !is_version_subject(&head.subject) {
                return Err(io::Error::other("HEAD is not a checkpoint"));
            }
            let parent = head.parent()?.to_owned();
            let tip = redo_tip(&git, &record, &head.id)?.unwrap_or_else(|| head.id.clone());
            (
                parent,
                format!("v:undo {tip}"),
                format!("{} undone", head.subject),
            )
        }
        Arguments::Redo => {
            let tip = redo_tip(&git, &record, &head.id)?
                .ok_or_else(|| io::Error::other("nothing to redo"))?;
            let path = path_to(&git, &tip, &head.id)?;
            let next = path
                .last()
                .ok_or_else(|| io::Error::other("nothing to redo"))?;
            (
                next.id.clone(),
                format!("v:redo {tip}"),
                format!("{} restored", next.subject),
            )
        }
        Arguments::Discard | Arguments::Squash(_) => {
            let (base, oldest) = run_base(&git, &head)?;
            let range = format!("{}..{}", oldest.subject, head.subject);
            if let Arguments::Squash(message) = action {
                let tree = git.text(["rev-parse", &format!("{}^{{tree}}", head.id)])?;
                let mut arguments = vec!["commit-tree", &tree];
                if let Some(base) = base.as_deref() {
                    arguments.extend(["-p", base]);
                }
                arguments.extend(["-m", message]);
                let target = git.text(arguments)?;
                (
                    target,
                    "v:squash".to_owned(),
                    format!("squashed {range} as {}", quoted(message)),
                )
            } else {
                let base =
                    base.ok_or_else(|| io::Error::other("checkpoint has no parent to restore"))?;
                let tip = redo_tip(&git, &record, &head.id)?.unwrap_or_else(|| head.id.clone());
                (
                    base,
                    format!("v:discard {tip}"),
                    format!("{range} discarded"),
                )
            }
        }
        Arguments::Checkpoint(_) | Arguments::Done(_) | Arguments::Help => unreachable!(),
    };

    // Prepare locks and verifies both the branch and symbolic HEAD before touching files.
    // The reflog message and branch movement commit together; there is no separate redo file.
    let mut transaction = RefTransaction::prepare(&git, &branch, &head.id, &target, &reason)?;
    if git.text(["reflog", "show", "-1", "--format=%H %gs", &branch, "--"])? != record {
        return Err(io::Error::other(
            "branch history changed during history operation",
        ));
    }
    validate_repository(&git)?;
    protect_ignored_files(&git, &target)?;
    git.run([
        "read-tree",
        "--no-recurse-submodules",
        "-n",
        "-m",
        "-u",
        &head.id,
        &target,
    ])?;
    git.run([
        "read-tree",
        "--no-recurse-submodules",
        "-m",
        "-u",
        &head.id,
        &target,
    ])?;
    if let Err(error) = transaction.commit() {
        // A rejected ref transaction must not leave the target's index and files behind.
        git.run([
            "read-tree",
            "--no-recurse-submodules",
            "-m",
            "-u",
            &target,
            &head.id,
        ])?;
        return Err(error);
    }
    if local_discard {
        let removed = crate::finish::discard_local(Path::new(&root), &git, &branch, &target)
            .map_err(|error| {
                io::Error::other(format!(
                    "discard succeeded, but local repository cleanup failed: {error}"
                ))
            })?;
        return Ok(format!("{message}; {removed}"));
    }
    Ok(message)
}

pub(crate) fn validate_repository(git: &Git) -> io::Result<()> {
    validate_state(git)?;
    let status = git.text([
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--ignore-submodules=none",
    ])?;
    if !status.is_empty() {
        return Err(io::Error::other(
            "working tree and index must be clean; stash, delete, or reset changes first",
        ));
    }
    validate_index(git)
}

pub(crate) fn validate_state(git: &Git) -> io::Result<()> {
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_START",
        "rebase-apply",
        "rebase-merge",
        "sequencer",
        "shallow",
        "info/grafts",
    ] {
        let path = git.text(["rev-parse", "--path-format=absolute", "--git-path", name])?;
        if Path::new(&path).exists() {
            return Err(io::Error::other(format!(
                "unsupported repository state: {name}"
            )));
        }
    }
    if !git
        .text(["for-each-ref", "--format=%(refname)", "refs/replace/"])?
        .is_empty()
    {
        return Err(io::Error::other("replacement commits are not supported"));
    }
    Ok(())
}

pub(crate) fn validate_index(git: &Git) -> io::Result<()> {
    let entries = git.output(["ls-files", "-v", "-z"])?;
    if !entries.status.success() {
        return Err(command_error(entries));
    }
    if entries.stdout.split(|byte| *byte == 0).any(|entry| {
        entry
            .first()
            .is_some_and(|tag| *tag == b'S' || tag.is_ascii_lowercase())
    }) {
        return Err(io::Error::other(
            "sparse or assume-unchanged index entries are not supported",
        ));
    }
    let entries = git.text(["ls-files", "--stage", "-z"])?;
    if entries
        .split('\0')
        .any(|entry| entry.starts_with("160000 "))
    {
        return Err(io::Error::other(
            "history commands do not support submodules",
        ));
    }
    Ok(())
}

fn protect_ignored_files(git: &Git, target: &str) -> io::Result<()> {
    let ignored = git.output([
        "ls-files",
        "--others",
        "--ignored",
        "--exclude-standard",
        "-z",
    ])?;
    if !ignored.status.success() {
        return Err(command_error(ignored));
    }
    if ignored.stdout.is_empty() {
        return Ok(());
    }
    let tree = git.output(["ls-tree", "-r", "--name-only", "-z", target])?;
    if !tree.status.success() {
        return Err(command_error(tree));
    }
    // Compare raw Git paths, including ancestor collisions (a file replacing a directory).
    for ignored in ignored
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let ignored = ignored.strip_suffix(b"/").unwrap_or(ignored);
        for tracked in tree
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            let overlaps = |child: &[u8], parent: &[u8]| {
                child == parent
                    || child
                        .strip_prefix(parent)
                        .is_some_and(|suffix| suffix.starts_with(b"/"))
            };
            if overlaps(ignored, tracked) || overlaps(tracked, ignored) {
                return Err(io::Error::other(
                    "ignored files would be overwritten or removed",
                ));
            }
        }
    }
    Ok(())
}

fn run_base(git: &Git, head: &Commit) -> io::Result<(Option<String>, Commit)> {
    if !is_version_subject(&head.subject) {
        return Err(io::Error::other("HEAD is not a checkpoint"));
    }
    let mut oldest = Commit::read(git, &head.id)?;
    loop {
        if oldest.parents.is_empty() {
            return Ok((None, oldest));
        }
        let parent = Commit::read(git, oldest.parent()?)?;
        if !is_version_subject(&parent.subject) {
            return Ok((Some(parent.id), oldest));
        }
        oldest = parent;
    }
}

// Returned in reverse order: the last element is the next checkpoint to redo.
fn path_to(git: &Git, tip: &str, head: &str) -> io::Result<Vec<Commit>> {
    let mut path = Vec::new();
    let mut next = tip.to_owned();
    while next != head {
        let commit = Commit::read(git, &next)?;
        if !is_version_subject(&commit.subject) {
            return Err(io::Error::other(
                "recorded redo path no longer matches this branch",
            ));
        }
        next = commit.parent()?.to_owned();
        path.push(commit);
    }
    Ok(path)
}

fn redo_tip(git: &Git, record: &str, head: &str) -> io::Result<Option<String>> {
    let words = record.split_whitespace().collect::<Vec<_>>();
    let [current, action, tip] = words.as_slice() else {
        return Ok(None);
    };
    if *current != head || !matches!(*action, "v:undo" | "v:redo" | "v:discard") {
        return Ok(None);
    }
    if !matches!(tip.len(), 40 | 64) || !tip.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(io::Error::other("invalid recorded redo path"));
    }
    path_to(git, tip, head)?;
    Ok(Some((*tip).to_owned()))
}

fn quoted(message: &str) -> String {
    format!("{message:?}")
}

/// A prepared native Git ref transaction. Dropping it aborts and releases its locks.
pub(crate) struct RefTransaction {
    child: Child,
    stdout: BufReader<ChildStdout>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
}

impl RefTransaction {
    pub(crate) fn prepare(
        git: &Git,
        branch: &str,
        old: &str,
        new: &str,
        reason: &str,
    ) -> io::Result<Self> {
        let mut child = git
            .command()
            .args(["update-ref", "--create-reflog", "-m", reason, "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut stderr = child.stderr.take().unwrap();
        let stderr = Some(thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes)?;
            Ok(bytes)
        }));
        let mut transaction = Self {
            child,
            stdout,
            stderr,
        };
        let commands = format!("start\nupdate {branch} {new} {old}\nprepare\n");
        transaction.send(&commands)?;
        transaction.acknowledge("start: ok")?;
        transaction.acknowledge("prepare: ok")?;
        // Updating the checked-out branch also locks HEAD for its reflog update.
        // Check its symbolic target after prepare, in case checkout raced planning.
        if git.text(["symbolic-ref", "--quiet", "HEAD"])? != branch {
            return Err(io::Error::other(
                "current branch changed during history operation",
            ));
        }
        Ok(transaction)
    }

    fn send(&mut self, command: &str) -> io::Result<()> {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(command.as_bytes())
    }

    fn acknowledge(&mut self, expected: &str) -> io::Result<()> {
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        if line.trim_end() == expected {
            return Ok(());
        }
        self.child.stdin.take();
        let status = self.child.wait()?;
        let stderr = self
            .stderr
            .take()
            .unwrap()
            .join()
            .map_err(|_| io::Error::other("cannot read Git error"))??;
        Err(command_error(Output {
            status,
            stdout: line.into_bytes(),
            stderr,
        }))
    }

    pub(crate) fn commit(&mut self) -> io::Result<()> {
        self.send("commit\n")?;
        self.acknowledge("commit: ok")?;
        self.child.stdin.take();
        let status = self.child.wait()?;
        let stderr = self
            .stderr
            .take()
            .unwrap()
            .join()
            .map_err(|_| io::Error::other("cannot read Git error"))??;
        if !status.success() {
            return Err(command_error(Output {
                status,
                stdout: Vec::new(),
                stderr,
            }));
        }
        Ok(())
    }
}

impl Drop for RefTransaction {
    fn drop(&mut self) {
        // Closing stdin lets update-ref abort its prepared transaction normally.
        self.child.stdin.take();
        let _ = self.child.wait();
        if let Some(stderr) = self.stderr.take() {
            let _ = stderr.join();
        }
    }
}
