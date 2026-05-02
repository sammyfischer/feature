# Feature

A cli that enhances git.

## Install

Clone the repo and run `cargo install --path .` from the projects root. If you have just, run `just install`.

## Docs

- [List of commands](./docs/commands.md)
- [Config files](./docs/config.md)

## What is feature?

Feature's main purposes are:

- to simplify existing git commands
- to automate more complex tasks, like pruning merged branches
- to prettify and simplify command outputs

Feature uses the concept of a base branch in a lot of places. A base branch is the branch which a feature branch started from, and is intended to be merged back into when complete.

Feature uses these base branches automatically in places where it makes sense. For example, `feature update` rebases the current branch onto its base, no arguments needed. `feature prune` checks branches against their base to see if they can be safely deleted.

While feature's functionality is generally meant to work with the concept of feature and base branches, there are some commands that are useful in general:

- `start` and `commit` take all trailing command line args and put them together to form a branch name or commit message, respectively.
- `commit`, `status`, and `list` print a customized outupt that is much more detailed, compact, and colorful than git's default output

## Highlights

> I'm using a bash alias to use `ft` instead of `feature`

Commit

![Commit terminal output](screenshots/commit.png)

Status

![Status terminal output](screenshots/status.png)

First push without any cli options

![First-time push terminal output](screenshots/push.png)

## Feature workflow

Here's a summary of the feature workflow:

1. Switch to a base branch.
2. Start a feature branch with `feature start …`.
   - tip: instead of switching to the branch first, you can use `feature start --from <base> …`
3. Begin implementing the feature.
4. If it's a new day, check `feature st` to remember where you were and what changes you have.
5. Finish and commit with `feature commit …`.
6. If some time has passed, or you know that there are new changes on the base branch, run `feature update`.
7. Push changes to remote with `feature push`.
8. Use your repository hosting service (GitHub, Gitlab, etc.) to bring the changes into the base branch.
9. Switch back to the base branch with `git switch <base>`.
10. Update and clean up branches with `feature sync`.

## Todo list

### Housekeeping

- completions
  - read `--git-dir` and `--work-tree` to generate completions for the correct repo
  - add support for `--flag=value` syntax
  - complete all static values (flag names, subcmd names, enum values, true/false)
- update is buggy and weird
  - tests need to be way more rigorous so I can get this to work once and for all
- split config into cosmetic and semantic
  - cosmetic config can go in git config
  - semantic config (default remote, protected branches) can stay in feature config, but there doesn't need to be a global one
- show
  - handle merge commits in different ways (currently shows diff against first parent)
- improve test environment
  - make one home dir in each test, create all repos and dirs inside it
- run git gc every now and then
  - maybe in write commands like sync

### Features

- abbreviations
  - most commands should accept the shortest possible prefix to uniquely identify its target
  - branch targets, e.g. `feature update <base>`, `feature start --from <base>`
    - search refs/heads/* only
    - `feature list` should bold the unique prefix of each branch
  - make `feature commit --to` only accpet branches
  - upon ambiguity, print candidates and tell the user to run the command again
  - `feature show` takes a revision, currently uses `revparse_single`, not sure if this can accept 2 char hash prefixes
- start: arbitary variable substitutions
  - map keys to values in config, use those keys in branch name template replacement
- undo
  - uses reflog, undoes latest change
- stash
  - more intuitive options to stash (--all => workdir/index, --unstaged => workdir, --staged => index)
  - action should be a flag, not positional (and should --push by default)
  - concatenate args as message
  - pretty output
- submodule aware output
  - status
  - list
- mod (submodule commands)
  - `ft mod sync` - run sync in all modules
  - `ft mod start` - start a branch with the same name in each module
- worktree
  - open an interactive menu to pick a branch and create a worktree from it
  - or use specified branch in command line
- diff
  - basic options: --all (default), --staged, --unstaged
  - one arg: diff arg to workdir
  - two args: diff arg 1 to arg 2
  - summary mode, prints like status output (print patch by default)
- reflog
  - view reflog for a branch, select one to restore to that state
