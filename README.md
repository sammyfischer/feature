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

## Todo list

- use git2 for `feature update --skip` (currently doesn't work)
- consider using `.git/config` for feature config too
- rethink base/protected branches in the config. it's confusing and feels redundant
- simplify errors. the error enum isn't that useful and they could all just be strings
- precommit commands? (e.g. installing/modifying)
- submodule workflow support
- fully interactive tui for merge conflicts?
  - in-memory rebase?
