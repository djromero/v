use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitStatus;

use crate::git::Git;

pub fn bootstrap(git: &Git, directory: &Path, message: Option<&str>) -> io::Result<bool> {
    if fs::read_dir(directory)?.next().transpose()?.is_none() {
        return Err(io::Error::other("directory is empty"));
    }

    git.run(["init"])?;
    git.run([
        "config",
        "--local",
        crate::repository::CREATED_CONFIG_KEY,
        "true",
    ])?;
    stage_visible_entries(git, directory)?;
    git.commit("v0", message)
}

pub fn checkpoint(
    git: &Git,
    directory: &Path,
    subject: &str,
    message: Option<&str>,
) -> io::Result<bool> {
    stage_visible_entries(git, directory)?;
    git.commit(subject, message)
}

fn stage_visible_entries(git: &Git, directory: &Path) -> io::Result<ExitStatus> {
    let mut entries = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    entries.retain(|name| !name.as_encoded_bytes().starts_with(b"."));
    entries.sort();

    let mut eligible = Vec::with_capacity(entries.len() + 1);
    if directory.join(".gitignore").is_file() {
        eligible.push(OsString::from(".gitignore"));
    }
    for entry in entries {
        if !git.is_ignored(&entry)? {
            eligible.push(entry);
        }
    }

    // Files and directories removed from disk are absent from `entries`.
    // Keep tracked updates scoped to the invocation directory, even when empty.
    let tracked = git.output(["ls-files", "-z", "--", "."])?;
    if !tracked.status.success() {
        return Err(crate::git::command_error(tracked));
    }
    let status = if tracked.stdout.is_empty() {
        tracked.status
    } else {
        git.run(["add", "--update", "--", "."])?
    };
    if !status.success() || eligible.is_empty() {
        return Ok(status);
    }

    let arguments = ["add", "--all", "--"]
        .into_iter()
        .map(OsString::from)
        .chain(eligible);
    git.run(arguments)
}
