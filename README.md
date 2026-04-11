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

## Feature workflow

Here's a summary of the feature workflow:

1. Switch to a base branch. Optionally, tell feature that it's a base with `feature config append bases <branch_name>`, or edit the config file directly.
2. Start feature branch with `feature start ...`.
3. Begin implementing the feature
4. If it's a new day, check `feature st` to remember where you were and what changes you have.
5. Finish and commit with `feature commit ...`.
6. If some time has passed, or you know that there are new changes on the base branch, run `feature update`.
7. Push changes to remote with `feature push`.
8. Use your repository hosting service (GitHub, Gitlab, etc.) to bring the changes into the base branch.
9. Switch back to the base branch with `git switch <base>`
10. Update all bases with `feature sync`.
11. Clean up branches with `feature prune`.

## Todo list

### Housekeeping

- fix: if a merge is active, commit should correctly select both parent commits
  - something similar for rebase
  - cherry picks work differently, maybe prevent commits during active cherry pick
- support non-utf8 strings with lossy conversions

### Features

- in-progress operations
  - bisect
- add timestamps to some outputs maybe
- global flags
  - `--git-dir`
  - `--worktree`
  - `--config` project config file location, not for user config
- status
  - show current worktree if applicable
  - advice line
    - for ongoing operations, a short tip about what to do next could be nice
- configurable advice
  - like git, have advice config to turn off long advice messages
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
  - sync/prune all modules
  - create a single branch in all modules for features whose work will span across them
- worktree
  - open an interactive menu to pick a branch and create a worktree from it
  - or use specified branch in command line
- diff
  - interactive menu to select two things to diff (branch, commit, stash, etc.)
  - summary mode, prints like status output (print patch by default)
- reflog
  - view reflog for a branch, select one to restore to that state
- blame
