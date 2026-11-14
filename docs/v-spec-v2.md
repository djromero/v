# `v` v2 specification

Status: implemented. This document defines the v2 history commands.
[The v3 specification](v-spec-v3.md) supersedes discard behavior for repositories created by `v` and adds `--done`.

You want checkpoints now. You can undo, redo, squash, or discard them when you need to.

## Scope

Provide `--undo` (`-u`), `--redo` (`-r`), `--squash` (`-s`), and `--discard` (`-d`). Provide `--help` (`-h`) to explain basic checkpoint behavior and these four commands. Keep the existing checkpoint behavior and output for ordinary invocations.

History navigation is linear. When the next action is not obvious, fail with a useful error instead of guessing, opening an editor, prompting for a choice, or resolving conflicts automatically.

## Shared model

A checkpoint is a commit whose entire subject is `v` followed by ASCII digits. This follows the existing naming convention; it does not prove which program created the commit. Version subjects are reserved for checkpoints.

The current checkpoint run is the contiguous sequence of checkpoint commits ending at `HEAD`, bounded by the nearest ordinary commit or the repository root. Each checkpoint in the run must have exactly one parent, except a root checkpoint, which has none. Version numbers need not be consecutive. Commands identify checkpoints by ancestry, not by looking up their version numbers.

In the examples below, the run is `v0`, `v1`, `v2`, and its base is `22f2ad9`:

```sh
$ git log --oneline
f687155 (HEAD -> master) v2
574094c v1
82f2fa1 v0
22f2ad9 (origin/master, origin/HEAD) Stable fixes
6f64308 Handle non-version commit subjects
```

The base is determined by commit ancestry and subjects, independently of remote-tracking refs. The commands never fetch, push, or move other branches or tags.

## Command parsing and repository scope

```text
v --undo
v --redo
v --discard
v --squash <message...>
v --help
v -- <checkpoint message...>
```

Command flags and their short aliases are recognized only as the first argument. Help accepts no additional arguments, prints plain usage text, and exits with status 0 without inspecting or changing a repository. It requires neither Git nor a configured home directory. Undo, redo, and discard accept no additional arguments. Squash requires a nonblank, single-line message and always combines the entire current checkpoint run. Partial squash and range selection are outside v2.

For squash, all words after `--squash` form the message and are joined with spaces as with existing checkpoint messages. Text such as `v0..v1` has no special range meaning. `v -- <message...>` explicitly creates a checkpoint with literal message text, including text starting with a reserved flag. Other invocations retain the existing argument-as-message behavior.

History commands require an existing repository and an attached local branch. They operate on the entire branch and repository working tree, even from a subdirectory. They use the nearest enclosing Git repository and never bootstrap a nested repository. Ordinary checkpoint invocations retain their existing repository-selection rules.

## Preconditions and failure behavior

Before changing history, require a clean index and working tree across the entire repository, including no untracked non-ignored files. This applies to all four commands: undo, redo, squash, and discard. If the repository is dirty, fail without changing it. The user is responsible for stashing, deleting, or resetting changes before retrying. Refuse an ongoing merge, rebase, cherry-pick, revert, or bisect. Do not stash, stage user changes, delete untracked files, or initialize a repository to satisfy a history command.

Ignored files may remain, but an operation must fail if restoring the target tree would overwrite or remove them. Unsupported repository states, merge commits in the affected run, missing objects or concurrent branch changes must also fail.

Validate the complete requested action before changing the branch or working tree. A failed operation must leave the branch, index, working tree, and redo position unchanged.

Undo, redo, and discard restore tracked files to the target commit. They do not leave the removed changes in the working tree or clean up uncommitted work on the user's behalf.

## Undo

`v --undo` removes exactly the checkpoint at `HEAD` from the current branch and restores its parent's tracked tree. Record enough information to restore the exact removed commit later.

```sh
$ v --undo
✔ v2 undone
$ git log -1 --oneline
574094c (HEAD -> master) v1
```

Here, “undone” means removed from the branch tip. Do not delete the commit object or erase its reflog entry.

Repeated undo steps move backward through the same checkpoint run. Undoing its earliest checkpoint moves to the ordinary base commit. A further undo fails and must not remove the base.

If `HEAD` is an ordinary commit, fail with `HEAD is not a checkpoint`. If a checkpoint is the repository's root commit, fail because there is no parent to restore. Removing the root and returning to an unborn branch is outside v2.

## Redo

`v --redo` restores exactly the next checkpoint removed by a successful undo or discard on the current branch. Restore the original commit object and tracked tree, without making a replacement commit.

```sh
$ v --redo
✔ v2 restored
$ git log -1 --oneline
f687155 (HEAD -> master) v2
```

After undoing `v2` and then `v1`, the next two redos restore `v1` and then `v2`. Each target must be the direct child of the current commit on the recorded path. Undo after a partial redo moves the cursor backward on that same path.

This is checkpoint redo, not a pull or a generic search for descendant commits. Never choose an arbitrary child because its subject resembles the next version.

Redo must survive separate CLI invocations and be scoped to the branch. Use recorded operation identity and commit IDs to distinguish this path from unrelated reflog entries. Git reflogs may supply recovery information, but reflog order alone must not decide what to restore. The exact storage mechanism is an implementation choice.

If the path cannot be established uniquely, a required commit object is unavailable, or the current branch no longer matches the expected position, fail. Reflog expiry and Git object cleanup can make recovery unavailable; indefinite recovery is not promised.

A successful new checkpoint or squash abandons the old redo path. External history changes on that branch invalidate it, even if the branch later returns to a familiar commit. A clean ordinary `v` invocation, a failed command, or temporarily switching to another branch does not consume redo steps. Returning to an unchanged branch may resume its own path. There is no command to choose an abandoned alternative timeline.

At the end of the path, or without a recorded path, fail with `nothing to redo`.

## Squash the current run

`v --squash <message...>` replaces the entire current checkpoint run with one ordinary commit. Its parent is the run's base when one exists; otherwise it is a new root commit with no parents. Its tree is the previous `HEAD` tree, and its subject is the supplied message. The resulting index and working tree remain clean.

Starting from the initial example:

```sh
$ v --squash fix db query performance
✔ squashed v0..v2 as "fix db query performance"
$ git log --oneline
<new-hash> (HEAD -> master) fix db query performance
22f2ad9 (origin/master, origin/HEAD) Stable fixes
6f64308 Handle non-version commit subjects
```

The message must not itself be a checkpoint subject such as `v3`. Squashing establishes an ordinary history boundary, so checkpoint navigation must not treat its result as another checkpoint.

A run containing one checkpoint may be squashed into an ordinary commit. Fail if there is no current checkpoint run. A run starting at the repository root can be squashed, including a single root checkpoint. Undo and redo navigate checkpoints only; they do not reverse a squash. After squash, the result is an ordinary commit, so `--undo` fails at that boundary. The old redo path is invalidated only after squash succeeds.

## Discard

`v --discard` removes the entire current checkpoint run from the branch and restores its ordinary base commit and tracked tree. It does not remove earlier ordinary history or uncommitted files.

Starting again from the initial example:

```sh
$ v --discard
✔ v0..v2 discarded
$ git log -1 --oneline
22f2ad9 (HEAD -> master, origin/master, origin/HEAD) Stable fixes
$ git status --short
```

Record the removed run as the linear redo path. Subsequent `--redo` invocations restore `v0`, then `v1`, then `v2`, one per invocation. If discard occurs while partway through an undo/redo path, retain its already known forward tail rather than replacing it with a shorter path.

Fail if there is no current checkpoint run or no ordinary base to restore. A discard with uncommitted changes fails without discarding them.

## Output and exit codes

Capture Git output as in the current CLI. Successful history actions print the messages shown above to standard output, with only `✔` colored green using ANSI escapes. Subjects and messages use the default terminal color. Display unusual message characters safely on one line.

A successful action exits with status 0. Invalid input, missing undo/redo targets, and ambiguous or unsupported operations print a red ANSI `ｘ` followed immediately by an error in the default color to standard error and exit nonzero. Preserve a failing delegated Git command's exit code where applicable; validation failures exit with status 1.

Ordinary `v` still prints a green checkmark and the created version, or only a blue checkmark with status 0 when nothing is committed.

## Acceptance scenarios

- Undo and redo restore exact commit IDs and tracked trees across separate invocations, including when the branch is ahead of its remote.
- Repeated undo/redo traverses one path in order, stops at its ends, and never crosses the ordinary base.
- Discard restores the ordinary base and allows stepwise redo through the full removed run, including an existing forward tail.
- A new checkpoint or successful squash invalidates redo; no-op checkpoints and failed commands preserve it.
- Squash combines the entire current run, preserves the final tree, uses the supplied subject, and establishes an ordinary commit boundary that undo cannot cross.
- Dirty state anywhere in the repository, unsupported history, invalid arguments, and unavailable recovery data fail without changing user state.
- History commands from subdirectories act on the enclosing repository, create no nested repository, and leave other branches unchanged.
- Existing checkpoint creation, clean success, colors, and error exit codes continue to work.

## Git references

[Git reflog documentation](https://git-scm.com/docs/git-reflog) describes local reference history and expiry. This explains the recovery limits; the command behavior above is the contract for `v`.
