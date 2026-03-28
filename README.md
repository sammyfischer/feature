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

The project file takes precedence and is designed to be tracked by version control. It should contain project-specific config for all developers on the team to use, e.g. names of base branches and the repository trunk.

> It's recommended to include `trunk`, `default_remote`, and `bases` in the project config if it's tracked by version control.

The global config file exists in your platforms standard location. There you can customize your own general preferences.

Use `feature config create` to create a project config file with all defaults. Use `feature config create -g` to do the same with a global config file. Each command outputs the location of the newly created file.

## Base branches

Feature uses the concept of a base branch in a lot of places. A base branch is a branch that feature branches are typically based off of, and subsequently merged back into when the feature is complete.

In order to start feature branches, you need to set base branches in the config (by default, "main" is a base branch, so you don't need to add that on a fresh feature installation).

You can add a base branch by running:

```bash
feature config append bases <branch_name>
```

> Hint: `append` and `remove` are the subcommands feature uses to modify arrays in the config. `bases` is the key being modified.

Base branches should be an exact reflection of their remote counterpart. They're not meant to be directly committed into. All work should be done on another branch and rebased/merged onto the base from the remote server.

Base branches are meant to reflect protected branches on services like GitHub. For this reason, the sync command force-updates local bases from their remote, since the remote is the single source of truth.

> The sync command force updates `refs/heads/<branch>` and `refs/remotes/<remote>/<branch>`

## Feature workflow

Here's a summary of the feature workflow:

1. Switch to a base branch. Optionally, tell feature that it's a base with `feature base ...`.
2. Start feature branch with `feature start ...`.
3. Implement feature and commit with `feature commit ...`.
4. If some time has passed, or you know that there are new changes on the base branch, run `feature update`.
5. Push changes to remote with `feature push`.
6. Use your repository hosting service (GitHub, Gitlab, etc.) to bring the changes into the base branch.
7. Sync all local bases with `feature sync`.
8. Switch off of the feature branch and run `feature prune` to clean up all merged branches.

As implied by some of the steps, feature is generally designed to complement central remotes where multiple people work from. Using it with a local repo is less useful, but some of the commands (start, commit, log, graph) will still be very useful.

## Todo list

Housekeeping

- use utf8 ellipsis (`\u2026`) to truncate text in graph
- fix `feature update --skip`, or consider removing it (along with continue and abort)
- simplify errors
  - most of them can just be strings
  - main function should return Result<(), String>
  - some functions can return a CliError, but only as an easy way to check the error type. they should be converted to a string for output
  - consider panicking for fatal errors, this would help debug
- rethink base/protected branches
  - protected branches are a decent way to protect non-bases from auto-deletions
  - maybe there's a better way

Features

- custom log/graph output
- submodule utilities
  - `feature mod ...`
  - sync/prune all modules
  - automatically commit to parent when committing to a module
  - `feature status` that intelligently displays module statuses too
  - create a single branch in all modules for features whose work will span across packages
  - config file to alias commands in packages, like pnpm workspaces
    - e.g. different test command for frontend and backend
- interactive tui for rebase
  - would only activate upon conflicts
  - make changes in the editor and save them
  - menu pops up with conflicted files, use arrow keys and space to toggle them as resolved/unresolved
  - enter to amend commit and continue
  - keybinds to skip and abort (something hard to accidentally press e.g. ctrl+q)
