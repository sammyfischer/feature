# Feature

A command line wrapper for git.

Feature makes it easier to work with feature branches. It:

- simplifies common sequences of commands into single commands
- improves common usages of existing commands (committing, creating branches, viewing git log)
- automatically tracks feature branches and their base branch

## Install

Clone the repo and run `cargo install --path .` from the projects root. If you have just, run `just install`.

## Config

Feature supports both a project/local config file and a global/user config file.

The project file takes precedence and is designed to be tracked by version control. It should contain project-specific config for all developers on the team to use, e.g. names of base branches and common formatting options.

The global config file exists in your platforms standard location. There you can customize your own general preferences.

Use `feature config create` to create a project config file with all defaults. Use `feature config create -g` to do the same with a global config file. Each command outputs the location of the newly created file. It's not recommended to leave this as-is. Customize the values you want and delete keys where you want to use the lower-level config values instead.

> The default config includes an empty array for protected branches. When configs get layered and merged together, empty arrays will overwrite the entire array set at lower levels. In other words, if you have protected branches in your global config, and your project contains an empty `protect` array, then no branches will be protected.

## Base branches

Feature uses the concept of a base branch in a lot of places. A base branch is a branch that feature branches are typically based off of, and subsequently merged back into when the feature is complete.

In order to start feature branches, you need to set base branches in the config (by default, "main" is a base branch, so you don't need to add that on a fresh feature installation).

You can add a base branch by running:

```bash
feature config append bases <branch_name>
```

> Hint: `append` and `remove` are the subcommands feature uses to modify arrays in the config. `bases` is the key being modified.

Base branches should be an exact reflection of their remote counterpart. They're not meant to be directly committed to. All work should be done on another branch and rebased/merged onto the base from the remote server.

## Feature workflow

Here's a summary of the feature workflow:

1. Switch to a base branch. Optionally, tell feature that it's a base with `feature config append bases <branch_name>`.
2. Start feature branch with `feature start ...`.
3. If it's a new day, check `feature st` to remember where you are and what changes you have.
4. Implement feature and commit with `feature commit ...`.
5. If some time has passed, or you know that there are new changes on the base branch, run `feature update`.
6. Push changes to remote with `feature push`.
7. Use your repository hosting service (GitHub, Gitlab, etc.) to bring the changes into the base branch.
8. Update all bases with `feature sync`.
9. Switch off of the feature branch and run `feature prune` to clean up merged branches.

As implied by some of the steps, feature is generally designed to complement central remotes where multiple people work from. Using it with a local repo is less useful, but some of the commands (start, commit, log, graph) will still be very useful.

## Todo list

Housekeeping

- fix `feature update --skip`, or consider removing it (along with continue and abort)
- support non-utf8 strings with lossy conversions

Features

- add `feature start --from` to start from a particular base
- status
  - show upstream ahead/behind (blue)
  - show base ahead/behind (magenta)
  - make branch name green (instead of blue)
  - show current worktree if applicable
  - submodules
- list
  - highlight current branches for each worktree in cyan
  - submodules
  - custom python function in config dir to build each line
- stash
  - more intuitive options to stash (--all => workdir/index, --unstaged => workdir, --staged => index)
  - action should be a flag, not positional (and should --push by default)
  - concatenate args as message
  - stashes could be given easier-to-type names (refs/stashes/name), this may affect compatibility with regular git stash commands
  - pretty output
- mod (submodule commands)
  - sync/prune all modules
  - create a single branch in all modules for features whose work will span across them
- interactive tui for rebase
  - would only activate upon conflicts
  - make changes in the editor and save them
  - menu pops up with conflicted files, use arrow keys and space to toggle them as resolved/unresolved
  - enter to amend commit and continue
  - keybinds to skip and abort (something hard to accidentally press e.g. ctrl+q)
