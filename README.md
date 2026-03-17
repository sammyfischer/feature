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

The global config file exists in your platforms standard location. There you can customize your own general preferences.

Use `feature config create` to create a project config file with all defaults. Use `feature config create -g` to do the same with a global config file. Each command outputs the location of the newly created file.

## Database

The database is a simple text file located at `.git/feature` in your project. Currently, it just maps feature branches to their base branches. Using feature commands (e.g. start and prune) will update the database as needed. If you create a feature branch directly with git, you can add the branch to the database manually with `feature db add <base_name> <branch>` (if you omit branch, it defaults to the current branch).

## Todo list / roadmap

- use git2
  - remaining: push, start, sync, update, is_merged()
- support feature commands in child dirs of a git dir (search upward)
- precommit support?
- submodule workflow support
