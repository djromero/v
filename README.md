# `v`

Run `v`, keep all and carry on.

```shell
$ git status
...
modified:   src/git.rs
modified:   src/main.rs

$ v
✔ v18

$ git status
nothing to commit, working tree clean

$ codex e "fix the build error"
$ v
✔ v19
```

When you’re done, use `v` or `git` to tidy up the history.

## In a nutshell

- Run `v` to stage local changes and create a Git commit.
- The first successful bootstrap commit has the subject `v0`. Later commits will use `v1`, `v2`, and so on.
- Optional arguments become the commit body.
- A clean invocation prints only a blue ANSI `✔`, creates no commit, and exits successfully with status 0.
- All versioning happens in the local Git repository. `v` does not push changes or create Git tags.

## Requirements and installation

- Git must be available on `PATH`, with an author identity configured.
- Rust and Cargo are required to build from source.
- `HOME`, or `USERPROFILE` when `HOME` is absent, must be set.
- From the source checkout, install with:

  ```sh
  cargo install --path .
  ```

## Usage

- Create a version without a message:

  ```sh
  v
  ```

- Add a commit body using separate words or a quoted message:

  ```sh
  v boost flux capacitor
  ```

- Arguments are joined with spaces. The generated version remains the commit subject, and the supplied message is a separate body paragraph.
- Run `v --help` or `v -h` for basic usage and the history commands.
- The first argument selects a command when it is `--undo` (`-u`), `--redo` (`-r`), `--squash` (`-s`), `--discard` (`-d`), or `--help` (`-h`). Otherwise, arguments remain commit-body text, including `--version`. Use `v -- <message...>` for a literal message starting with a reserved flag.

## Undo, redo, squash, and discard

```sh
v --undo                         # Step back one checkpoint
v --redo                         # Restore the next undone checkpoint
v --squash fix query performance  # Combine the current checkpoint run
v --discard                      # Remove the current checkpoint run
```

These commands require a clean index and working tree across the entire repository, including no untracked non-ignored files. Stash, delete, or reset changes yourself before retrying. They never clean up uncommitted work for you.

A checkpoint run is the consecutive sequence of commits with subjects such as `v0`, `v1`, and `v2` at the branch tip. The nearest ordinary commit is its base. Undo restores the previous commit and its files. Discard restores the base. Neither crosses the base or removes a root commit. A run without an ordinary base cannot be discarded; squashing it creates a new root commit.

Redo restores one original checkpoint at a time, including after discard. The path is linear and recorded in the branch's Git reflog, so it survives separate invocations and branch switches. A new commit, successful squash, or external history change on that branch ends the old redo path. Reflog expiry or missing objects can make recovery unavailable.

Squash replaces the entire run with one ordinary commit using the supplied subject and the current file contents. It requires a nonblank, single-line message that is not itself a checkpoint subject. It does not accept a range; all words after `--squash` are the message. Undo and redo do not reverse a squash.

History commands use the nearest enclosing repository even from a subdirectory and affect its entire working tree. They require an attached branch and linear checkpoint history. Ongoing Git operations, submodules, shallow history, replacement commits, and sparse or assume-unchanged index entries are refused. Ignored files are preserved, and an operation fails if the target would overwrite them. Squash creates the replacement commit directly with Git's `commit-tree`; commit hooks and automatic commit signing are not run for that replacement.

See [the v2 specification](docs/v-spec-v2.md) for the full behavior.

## Repository selection

- Discovery checks the current directory and its ancestors for `.git`, stopping at the nearest repository or after checking the home directory when encountered. Git worktrees with `.git` files are supported.
- A repository is recognized as managed by `v` when its local Git config has `v.created = true`. The marker is neither committed nor inherited from global config.
- Inside a marked repository, `v` reuses that repository and stages changes from the invocation directory.
- When no repository is found, `v` runs `git init` in the current directory and records the marker before attempting a `v0` commit.
- Inside a subdirectory of an ordinary Git repository, this creates a separate nested repository with its own marker and `v0` history.
- At an ordinary repository's root, `v` uses the existing repository without running `git init` or adding a marker. With `n` commits reachable from `HEAD`, the next commit is `v{n+1}`, followed by `v{n+2}` on the next changed invocation. Subdirectories still follow the ordinary-repository rule above.
- A nearer ordinary repository takes precedence over a marked ancestor. Repositories created by older versions without the marker are treated as ordinary Git repositories.

## Staging and commit scope

- Tracked files within the invocation directory and its descendants receive updates and deletions, including tracked hidden files and deleted directories.
- New visible files and directories in the invocation directory are staged recursively, respecting Git ignore rules.
- New dotfiles and dotdirectories directly inside the invocation directory are skipped, except for `.gitignore`. Hidden files beneath a visible directory can still be included when Git does not ignore them.
- Changes outside the invocation directory are not newly staged by `v`. **Any changes already staged anywhere in the repository are included in the commit**, even when `v` runs from a subdirectory.
- `v` uses the existing Git index and normal `git commit` behavior, including configured hooks. A failed commit can leave staged changes in place.

## Version numbering

- At an ordinary repository's root, numbering always uses the count of commits reachable from `HEAD`, plus one, regardless of the latest commit subject.
- In a marked repository, if the latest commit subject is exactly `v` followed by ASCII digits, `v` increments that number. For example, `v19` becomes `v20`.
- Otherwise, a marked repository also uses the reachable commit count plus one. For example, `v0` followed by a manual commit produces `v3` next.
- Numbering follows the current branch's `HEAD`; it does not search for the highest version across branches or tags.
- A clean invocation creates no commit and does not consume a version number.
- Version numbers must fit in an unsigned 64-bit integer. Invalid or overflowing numbers produce an error.

## Failures and current limitations

- Git output is captured. Success prints `✔ vN` to standard output, with only the checkmark colored green using ANSI escapes, and exits with status 0.
- Failures print a red `ｘ` followed by the error in the default color to standard error. Git failures preserve Git's exit code; other errors exit with status 1.
- An empty directory reports `directory is empty` before initializing Git.
- Bootstrap requires content that Git can commit, or changes already staged. An empty directory, or one containing only skipped or ignored new files, cannot produce `v0`.
- The repository marker is written before the first commit succeeds. If bootstrap fails without creating a commit, later invocations take the checkpoint path and fail at `git log`, even after files are added.
- To recover after resolving the failure, stage the intended files and create the initial commit manually, then resume using `v`:

  ```sh
  git add -- path/to/file
  git commit -m v0
  v
  ```

## Development

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
