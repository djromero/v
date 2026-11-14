# `v`

Zero effort versioning of simple content.
Command line tool written in rust wrapping the installed `git` binary

## v1 Spec

### Repo creation

Favor working in the current directory unless the current git repository was created with `v`.

Add some metadata when the repository is created with `v`.
If current directory:
* is not a git repository -> init as git repository
* is under `v` control -> use the current git repository
* is the root of an ordinary git repository -> reuse it without running `git init` or adding `v` metadata; count commits reachable from `HEAD` and commit as `v{n+1}`, then `v{n+2}` on the next changed invocation
* is a subdirectory under git control but not a `v` repository -> init a new nested git repository in the current directory

