# Commands

## General info

You can run `feature <command> --help` to get info about a particular command, or `feature --help` to get list the available commands.

Many commands support support a `--dry-run` option. Use `--help` to check.

## Start

```bash
feature start my new branch
```

Starts a new feature branch. Must be called from a known base branch.

This is similar to calling `git switch -c`, except that:

- feature will automatically detect the current branch as its base
- feature will take all trailing command line args and string them together as the branch name
- you can specify a custom template for all branch names to follow
  - view `feature start --help` for detailed info

## Commit

```bash
feature commit implemented some changes
feature commit --amend
```

Commits staged changes to the current branch.

With `--amend`, amends the most recent commit by adding the staged changes, and optionally replaces the commit message.

These are similar to `git commit` except that:

- command line args are concatenated as the commit message
- running with `--amend` doesn't require a commit message, and will instead reuse the existing message
- feature displays a summary of files changed by the commit
  - for an amend, it displays only the amended changes, not the total changes from its parent commit

## Update

```bash
feature update
feature update main
```

Updates the current branch with its base.

This is similar to `git rebase` except that:

- it automatically detects the base branch when possible

## Push

```bash
feature push
```

Pushes this branch to remote.

This is similar to `git push` except that:

- you never need to specify the upstream with `-u`
  - if it's your first push, it will push to the default remote with the same name
  - on subsequent pushes, it uses the existing upstream name

## Sync

```bash
feature sync
```

Fetches all branches from all remotes (pruning upstreams that no longer exist), then updates each base branch. It's similar to running `git fetch --all -p`, then `git pull` on each base branch.

Feature only fast-forwards branches. It checks that the local copy is a direct ancestor of the remote copy, then updates the reference of the branch.

## Prune

```bash
feature prune
```

Deletes all local feature branches that have been merged into their base.

If a branch doesn't have a known base, it won't be deleted. Branches are only deleted if they're a direct ancestor of, or exactly the same as, their base branch. These branches can easily be restored by checking out to the commit they pointed to and recreating them.

## Status

```bash
feature status
feature st
```

Prints the current status of the repo. This includes the current branch, the commit it points to, git username and email, and a summary of staged/unstaged changes.

This similar to `git status`, except that:

- it displays more info
- the info is more compact
- it's more colorful

## List

```bash
feature list
feature ls
```

Lists all local branches (not just feature branches).

This is similar to `git branch` except that:

- it shows the feature-base-branch, if present
- it displays as a table
- it's has simpler config options
  - columns can be hidden with cli or config file options
  - git branch output can be customized, but there's no clear documentation for the available field names
- it's more colorful

## Log

```bash
feature log
```

Lists commits with their commit message and author info at the end.

This command is *equivalent* to:

```bash
git log --all --pretty:'format:%C(auto)%h%d %C(reset)%s %C(dim)(%an, %ar)'
```

The main benefit is that there is a preconfigued default, and you can customize the format in a config file unlike with git.

## Graph

```bash
feature graph
```

Prints a graph of commits.

This command runs:

```bash
git log --graph --all --pretty='format:%C(auto)%h%d %C(green)%an %C(blue)%ar %C(reset)%s'
```

Like log, you can customize the format. Unlike log, this will truncate each line to the terminal width, so one commit will never take up multiple lines (unless you resize the terminal).

## Config

```bash
feature config ...
```

Subcommands to edit feature config files. Use `feature config --help` to see the details.

## Base

```bash
feature base main
```

Tells feature which base branch to use for the current branch. If another arg is specified after the base, it's interpreted as the feature branch.
