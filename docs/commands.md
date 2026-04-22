# Commands

## General info

You can run `feature <command> --help` to get info about a particular command, or `feature --help` to get list the available commands.

Many commands support support a `--dry-run` option, which doesn't modify the repo but displays command output as if the command were run. Use `--help` to check. Dry-run mode may still fetch remote-tracking branches.

## Start

```bash
feature start my new branch
feature start --from dev my new branch
feature start --stay create but dont switch
```

Starts a new feature branch. Must be called from a known base branch.

This is similar to calling `git switch -c`, except that:

- feature will automatically detect the starting branch as its base
- feature will take all trailing command line args and string them together as the branch name
- you can specify a custom template for all branch names to follow
  - view `feature start --help` for detailed info

Using the `--stay` option is similar to calling `git branch …`.

## Commit

```bash
feature commit implement some changes
feature commit --amend
feature commit --to feature1 separate concerns
```

Commits staged changes to the current branch.

With `--to`, attempts to resolve the argument to a ref and commits to that instead.

With `--amend`, amends the most recent commit by adding the staged changes, and optionally replaces the commit message.

These are similar to `git commit` except that:

- command line args are concatenated as the commit message
- running with `--amend` doesn't require a commit message, and will instead reuse the existing message
- you can commit anywhere using `--to`
- it displays a summary of files changed by the commit, and the authorship info used for the commit
  - for an amend, it displays only the amended changes, not the total changes from its parent commit
  - for a merge commit, it displays all the changes brought into the target branch by the merge (i.e. diff against its first parent)

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
- it performs checks against the upstream and base, if they exist
  - these checks ensure that new commits are reflected in the branch before you push
  - feature automatically fetches the upstream and base to ensure the latest commits are being checked
  - if an automatic fast-forward can be done safely, then it does that and continues with the push
  - if the branches have diverged, stops and asks the user to bring in the changes manually
  - `--force` skips these checks

## Sync

```bash
feature sync
```

Fetches all branches from all remotes (pruning upstreams that no longer exist), fast-forwards all local branches with upstreams, and then prunes merged branches.

It's similar to running:

1. `git fetch --all -p`
2. `git pull` on every branch
3. `feature prune`

Feature only fast-forwards branches. It checks that the local copy is a direct ancestor of the remote copy, then updates the reference of the branch. If a branch can't be fast-forwarded, it's left as-is.

Feature won't update the current branch if there are changes in the working directory, but it will still attempt to sync other branches.

## Prune

```bash
feature prune
```

Deletes all local feature branches that have been merged into their base.

Feature will not delete a branch if any of the following conditions are met:

- the branch has no know base branch
- the branch has never been pushed to remote (i.e. there is no `remote` variable in the branch's git config)
- the branch is not a direct ancestor of (or equal to) its base
  - in other words, if the branch is diverged from or ahead of its base, which means it includes commits not in the base

Similar to running:

```bash
branch="$1"
base="$2"

branch_tip=$(git rev-parse "$branch")
base_tip=$(git rev-parse "$base")

# ignore if there's no known remote/upstream
if ! git config "branch.$branch.remote"; then
  exit 0
fi

# delete if they point to the same commit
if [ "$branch_tip" = $"base_tip"]; then
  git branch -D "$branch"
  exit 0
fi

# delete if branch is a direct ancestor
if git merge-base --is-ancestor branch base; then
  git branch -D "$branch"
fi
```

on each `(branch, base)` pair. Note that this script does not cover branch iteration, or determining which base belongs to which branch.

## Status

```bash
feature status
feature st
```

Prints the current status of the repo. This includes the current branch, the commit it points to, git username and email, and a summary of staged/unstaged changes.

If applicable, displays any active state the repo is in (e.g. merge conflicts, cherry-pick conflicts) and extra info about the state (e.g. a list of conflicted files).

This similar to `git status`, except that:

- it displays more info
- it's more compact
- it's more colorful

## List

```bash
feature list
feature ls
```

Lists all local branches (not just feature branches).

This is similar to `git branch` except that:

- it shows the base branch, if present
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

## Show

```bash
feature show
feature show main --no-summary
feature show 9fe6b04 --message=subject
```

View details of a particular commit. You can disable different parts of the output with the command line options and config file options, e.g. hiding the patch diff. You can customize the timestamp formatting in the `[format]` section of the config file.

By default, shows HEAD. You can pass in anything that can be resolved to a commit, e.g. branch names, tag names, and `HEAD^1`.

For commits with multiple parents, the diff output will be against the first parent. For merge commits, the first parent is always the branch being merged into (i.e. the current branch at the time of the merge). In other words, the diff shows the changes that were brought into the branch by the merge, rather than the changes made specifically in that commit.

For the stash commit (`feature show refs/stash`), the first parent is the HEAD at the time the stash was created. In other words, the diff shows all the changes that were stashed.

This command is similar to `git show` except:

- the output is in the style of other feature commands

## Config

```bash
feature config …
```

Subcommands related to feature config files. Use `feature config --help` to see the its subcommands. View details of each subcommand with `feature config <subcommand> --help`.

## Base

```bash
feature base main
feature base main --branch feature-branch
```

Sets the base branch of `branch`. If no `branch` is specified, uses the current branch.

The base branch is metadata used solely by feature. It only accepts short local branch names, e.g. `main`. It doesn't accept `origin/main` or `refs/heads/main`, for example. It will automatically determine if the branch has an upstream, and use that if available.
