# `v`

`v` is a command-line tool for making quick checkpoints of a directory. Run it in a directory with files you want to save: `v` creates a local Git repository and saves a numbered commit without asking for a commit message. Run it again as you make changes to save more checkpoints. Undo experiments as you go, then squash checkpoints into a named commit when you’re ready.

A **checkpoint** is a Git commit with a subject such as `v0`, `v1`, or `v2`. Everything stays in your local repository: `v` does not push changes or create Git tags.

For example, in a directory containing your work:

```sh
v                               # Create a local repository and save the first checkpoint
# Make more changes…
v                               # Save another checkpoint
v --undo                        # Restore the previous checkpoint and its files
v --redo                        # Restore the checkpoint you just undid
v --squash "Add flux capacitor" # Combine the current checkpoint run into one commit
```

You can also use `v` with existing Git repositories. At an ordinary repository’s root it uses that repository; in a subdirectory it creates a separate local repository. Inside a repository already created by `v`, it reuses that repository. See [what gets saved](#what-gets-saved) for repository selection, staged changes, and dotfile handling.

## Install

You need Git on `PATH` with an author identity configured, plus Rust and Cargo to build from source.

From a checkout of this repository:

```sh
cargo install --path .
```

## Create checkpoints

In the directory you want to checkpoint, run:

```sh
v
```

There is no separate setup step: for a directory outside an existing repository, the first run initializes Git and saves `v0`. The directory needs at least one file Git can commit. Continue working, then run `v` whenever you want another checkpoint.

A successful save prints `✔ vN`, where `vN` is the new commit’s subject. If there is nothing to commit, `v` prints only a blue `✔` and succeeds without creating a commit or consuming a number.

You can add a note using separate words or a quoted message:

```sh
v try a simpler parser
v "Handle empty input"
```

The note becomes the commit body; the numbered subject stays automatic. A new repository starts at `v0`. Existing repositories use their history to choose the number; see [version numbering](#detailed-behavior).

Run `v --help` (or `v -h`) for a command summary.

## What gets saved

`v` stages changes from the directory where you run it, then commits the repository’s staged changes:

- Updates and deletions of tracked files are staged within that directory and its descendants, including tracked hidden files and deleted directories.
- New visible files and directories are staged recursively, respecting Git ignore rules.
- New dotfiles and dotdirectories directly in the current directory are skipped, except for `.gitignore`. Hidden files beneath a visible directory can still be included when Git does not ignore them. Stage skipped files yourself if you want them included.
- Changes outside the current directory are not newly staged. **Anything already staged anywhere in the selected repository is included in the commit.**

Which repository is selected depends on where you run `v`:

| Where you run `v` | What happens |
| --- | --- |
| Outside an existing repository | Initializes a repository in the current directory and attempts the first checkpoint. |
| At the root of an ordinary Git repository | Uses that repository. |
| In a subdirectory of an ordinary Git repository | Creates a separate nested repository in the current directory. |
| At the root of, or within, a repository created by `v` | Uses that repository, unless a nearer repository takes precedence. |

Here, an **ordinary repository** means one without `v`’s local creation marker. Running `v` at an ordinary repository’s root does not add that marker or change its subdirectory behavior.

Checkpoint creation uses normal `git commit` behavior, including configured hooks. A failed commit can leave staged changes in place.

## Undo, redo, squash, and discard

A **checkpoint run** is the consecutive sequence of checkpoint commits at the tip of your branch. The nearest ordinary commit is its **base**:

```text
ordinary commit → checkpoint → checkpoint → checkpoint (HEAD)
     base         └────────── current run ────────────┘
```

| Command | Effect on history and files |
| --- | --- |
| `v --undo` / `v -u` | Steps back one checkpoint and restores the previous commit’s files. |
| `v --redo` / `v -r` | Restores the next undone or discarded checkpoint and its files. |
| `v --squash "Fix the parser"` / `v -s "Fix the parser"` | Replaces the entire current run with one ordinary commit, keeping the current file contents. |
| `v --discard` / `v -d` | Restores the current run’s base when present; also removes `.git` for a repository created by `v`. |

**All four commands require a clean index and working tree across the entire repository**, including no untracked non-ignored files. Stash, delete, or reset uncommitted changes yourself before retrying.

Unlike checkpoint creation, undo, redo, squash, and discard always use the nearest enclosing repository and affect its entire working tree, even from a subdirectory.

Undo stops at the base and cannot remove a root commit. In an ordinary repository, discard requires an ordinary base. In a repository created by `v`, discard closes the repository by removing `.git`: it restores the base when a checkpoint run has one, and otherwise leaves working files unchanged. Local redo is unavailable after `.git` is removed.

**Squash cannot be undone or redone with `v`.** It combines the entire run, accepts no range, and requires a nonblank, single-line message that is not itself a checkpoint subject such as `v3`. Its result becomes the new ordinary history boundary.

Redo restores one checkpoint at a time. Creating a new commit, successfully squashing, or changing that branch’s history externally ends the old redo path.

## Finish a nested workspace

When a directory has its own repository created by `v` inside your project:

```sh
v --done "Add flux capacitor"  # Squash remaining checkpoints, promote history, remove local .git
# Or, if everything has already been squashed:
v --done
```

Done transfers all commits on the local branch to the nearest outer repository,
keeping named commits separate and preserving their messages. Files stay under
this directory’s path in the outer project, including committed deletions. The
local `.git` is removed only after promotion succeeds. Working files remain.

Run done at the local repository root with a clean index and working tree. If
checkpoints still need squashing, provide a message. A message squashes only the
remaining checkpoint run; earlier named commits retain their messages. Earlier
unsquashed checkpoints before a named commit must be resolved first.

Unrelated outer staged and unstaged changes are preserved and excluded from the
promoted commits. Staged outer changes within this directory cause an error.
Done requires an outer repository with an existing commit and an attached branch;
it refuses linked/shared local worktrees and nonlinear local history. Promotion
uses Git plumbing rather than ordinary commit hooks.

To abandon the local workspace instead, use `v --discard`. It requires clean local
state, restores the current checkpoint run’s base when one exists, and removes
local `.git`. A root checkpoint run (including only `v0`) or an already named HEAD
keeps its files. Removing `.git` lets the outer repository see those files again;
it does not restore their original contents in the outer project.

## Troubleshooting

### The first checkpoint fails

The first checkpoint needs content that Git can commit. An empty directory reports `directory is empty` before Git is initialized. A directory containing only ignored or skipped new files cannot produce `v0` either.

If initialization writes the repository marker but no commit is created, later invocations fail at `git log`, even after you add files. Resolve the original failure (for example, configure your Git identity), then stage the intended files and create the first commit manually:

```sh
git add -- path/to/file
git commit -m v0
v
```

### A history command refuses to run

Check for uncommitted changes anywhere in the repository, including untracked files. History commands also require an attached branch and linear checkpoint history; additional unsupported states are listed below.

Redo may be unavailable if a new commit or history change ended its path, or if Git has expired the required reflog entries or removed the commit objects.

## Detailed behavior

<details>
<summary>Repository discovery and the creation marker</summary>

- Discovery checks the current directory and its ancestors for `.git`, stopping at the nearest repository or after checking the home directory when encountered. Git worktrees with `.git` files are supported.
- A repository is recognized as created by `v` when its local Git config has `v.created = true`. The marker is neither committed nor inherited from global config.
- When initializing a repository, `v` writes this marker before attempting the first `v0` commit.
- A nearer ordinary repository takes precedence over a marked ancestor. Repositories created by older versions without the marker are treated as ordinary Git repositories.
- Checkpoint creation requires `HOME`, or `USERPROFILE` when `HOME` is absent, to be set.

</details>

<details>
<summary>Version numbering</summary>

- At an ordinary repository’s root, the next number is the count of commits reachable from `HEAD`, plus one, regardless of the latest commit subject. With `n` reachable commits, successive changed invocations create `v{n+1}`, `v{n+2}`, and so on.
- In a marked repository, if the latest commit subject is exactly `v` followed by ASCII digits, `v` increments that number. For example, `v19` becomes `v20`.
- Otherwise, a marked repository also uses the reachable commit count plus one. For example, `v0` followed by a manual commit produces `v3` next.
- Numbering follows the current branch’s `HEAD`; it does not search for the highest version across branches or tags.
- A clean invocation creates no commit and does not consume a version number.
- Version numbers must fit in an unsigned 64-bit integer. Invalid or overflowing numbers produce an error.

</details>

<details>
<summary>Command arguments</summary>

Arguments for a checkpoint note are joined with spaces and written as a separate body paragraph.

Only the first argument selects a command: `--undo` (`-u`), `--redo` (`-r`), `--squash` (`-s`), `--discard` (`-d`), `--done`, or `--help` (`-h`). Otherwise, arguments are treated as checkpoint notes. Use `--` to save a literal note starting with a reserved flag:

```sh
v -- --undo
```

Undo, redo, discard, and help accept no additional arguments. All words after `--squash` or `--done` form its commit message; text such as `v0..v1` has no range meaning.

</details>

<details>
<summary>History boundaries, recovery, and unsupported states</summary>

- A checkpoint is identified by a subject consisting of `v` followed by ASCII digits, regardless of which program created it. Checkpoint numbers need not be consecutive.
- In an ordinary repository, a run without an ordinary base cannot be discarded. In a repository created by `v`, discard removes local `.git` and leaves that run’s files intact. Squashing a root run creates a new root commit.
- Redo restores the original commits along one linear path recorded in the branch’s Git reflog. The path survives separate invocations and branch switches; returning to an unchanged branch can resume its path. A clean checkpoint invocation or failed command does not consume redo steps. Reflog expiry or missing objects can make recovery unavailable.
- History commands refuse ongoing Git operations, submodules, shallow history, replacement commits, and sparse or assume-unchanged index entries.
- Ignored files are preserved. An operation fails if the target would overwrite them.
- Squash creates the replacement commit directly with Git’s `commit-tree`. Commit hooks and automatic commit signing are not run for that replacement.

See [the v2 specification](docs/v-spec-v2.md) for history navigation and [the v3 specification](docs/v-spec-v3.md) for done and local discard cleanup.

</details>

<details>
<summary>Output and exit codes</summary>

Git output is captured. Successful checkpoint creation prints `✔ vN` to standard output, with only the checkmark colored green using ANSI escapes. A clean invocation prints only a blue `✔`. Both exit with status 0.

Successful history commands print a green checkmark followed by an action summary and exit with status 0.

Failures print a red `ｘ` followed by the error in the default color to standard error. Git failures preserve Git’s exit code; other errors exit with status 1.

</details>

## Development

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
