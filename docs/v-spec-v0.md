# `v`

Zero effort versioning of simple content.

## The tool

Command line tool written in rust wrapping the installed `git` binary

## Magic Zero Arguments Interface

The tool is always invoked as `v` without arguments.

"the directory" means the current directory where the user is running `v`.

### Bootstrap
The directory is not under version control. 
No `.git` directory up the directory tree until the home directory.

`v` --> `git init` + `git add *` + `git commit -m "v0"

If there's a message as argument use it as commit body.

### Checkpoint

The directory is under version control. 
There is a `.git` directory up the directory tree until the home directory.

`v` -> `git add *` + `git commit -m "v{N+1}"

Where {n+1} is the last commit version number + 1.
Extract the number from last commit (ie vN -> N) and increment it (N + 1).

If there's a message as argument use it as commit body.

## Usage Example

```shell
$ ls
README.md 
main.go

$ v experimental launcher
$ git log
commit 3817d8a05d261093c230b46e8f4f5a41db3312af (HEAD -> master)
Author: Julián Romero <julian.romero@jobandtalent.com>
Date:   Sun Sep 6 22:15:13 2026 +0200

    v0

    experimental launcher
$ git log
On branch master
nothing to commit, working tree clean
$ echo 'v0 is done' >> README.md
$ v
$ git log
commit 26ea37af75984bd75ebf7d12195025f8b613fa8d
Author: Julián Romero <julian.romero@jobandtalent.com>
Date:   Sun Sep 6 16:01:39 2026 +0200

    v1

commit 3817d8a05d261093c230b46e8f4f5a41db3312af (HEAD -> master)
Author: Julián Romero <julian.romero@jobandtalent.com>
Date:   Sun Sep 6 22:15:13 2026 +0200

    v0

    experimental launcher
$ git log
On branch master
nothing to commit, working tree clean
```


