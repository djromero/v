# `v` v3 specification

Status: implemented. This extends v1 and v2; the discard rules
below supersede v2 for repositories created by `v`.

## Finish a local workspace

```
v --done
v --done <message...>
```

Done requires invocation at the root of a repository with local `v.created = true`,
a real local `.git` directory, a clean index and working tree (including no
untracked non-ignored files), and an enclosing Git repository. Fail otherwise.
Do not bootstrap repositories. Refuse detached branches, ongoing Git operations,
submodules, linked worktrees, shallow/replaced history, or nonlinear local history.
Ignored files may remain and are never promoted unless already committed.

Promote all commits reachable from local HEAD, oldest first, into the nearest
outer repository on its current attached branch, which must already have a commit. Preserve each complete commit
message. Prefix every local path with the workspace's path in the outer project.
Each promoted tree replaces that entire subtree, including deletions; paths
outside it remain as in outer HEAD. Do not create a gitlink or cherry-pick paths
at the outer root. Commit IDs and authorship may change.

If HEAD ends in a checkpoint run, a message is required. Apply squash semantics
to that entire run, including root v0, before promotion. The message must be
nonblank, single-line, and not a checkpoint subject; words are joined with spaces.
Previously named/squashed commits remain separate. Earlier unsquashed checkpoints
before a named commit are rejected rather than assigned an invented message.
If HEAD is already named, no squash is needed; an optional message is unused.
Only the current branch's reachable history is promoted, not other branches or
abandoned undo/redo commits.

Prepare the squashed result and promoted commits without changing local history.
Validate and publish the outer branch as one ref update, so failures during
planning or rejected ref updates cannot leave a partial promotion. Preserve
unrelated outer staged and unstaged changes. Refuse staged changes inside the
workspace and outer submodule entries. Lock the outer index and branch while
publishing; reject concurrent changes. No working files are rewritten by done.
This uses Git plumbing and does not run normal commit hooks; ref transaction
hooks apply.

After successful promotion and outer index update, remove the local `.git`.
Leave all working files and ignored files intact. Success prints `✔ promoted N
commit(s) and removed local repository` on one line. A repeated done fails because
there is no longer a local `v` repository. A cleanup failure after publishing must
report that promotion succeeded and must not invite blindly promoting again.

## Discard and close

Require a clean nearest repository before any action. For ordinary repositories,
retain v2 discard behavior, including redo and failure without an ordinary base.

For a repository marked `v.created = true`, require a real, unshared local `.git`
directory. If HEAD has a checkpoint run and an ordinary base, restore that base
using the existing discard operation. If the run reaches the root (including only
v0), leave files unchanged. If HEAD is already an ordinary commit, leave files
unchanged. Then remove the local `.git`, even if there was nothing to squash or
discard. The outer repository, if any, is never committed or reset.

Removing `.git` exposes remaining files to the outer repository's ordinary
tracking and ignore rules; it does not restore the outer repository's file
contents. There is no local redo after cleanup. Unsupported states or dirty state
fail before cleanup. From a subdirectory, discard still acts on the entire nearest
repository, as in v2. Success includes `removed local repository`.

## Verification

Cover root checkpoints, multiple named commits plus a pending run, required and
invalid messages, path prefixes, deletions, binary files and file modes, unchanged
outer paths, unrelated staged/unstaged changes, ordinary/local repository scope,
dirty state, unsupported history, locked/rejected publication, cleanup, and
ordinary-repository discard/redo compatibility.
