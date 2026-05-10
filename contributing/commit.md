# Committing

Use [conventional commit](https://www.conventionalcommits.org) messages to help write changelogs and decide on version changes. Most importantly, use `!` after the type/scope to denote breaking changes. You can include the "BREAKING CHANGE" footer in addition if more info is needed.

## Types

The types used are:

- feat
- fix
- test
- docs
- dev
- style

These are generally in order of priority. If changes in a commit span multiple of these categories, use the highest one applicable. Although, it's good practice to split them up as much as possible.

Refactors, formatting, and linting fall under the "style" type.

The "dev" type means anything related only to developer workflow. Some examples are changes to scripts (justfile, pre-commit) and changes to github workflows.

## Scopes

The scopes used are:

- cli command names (e.g. commit, push)
- config
- module names (e.g. templater, completions)

Changes in the `data` module fall under config. Most of the time, changes to completions will be related to the single command being modified, in which case you should use the command name as the scope. Only use "completions" when the commit is entirely related to completions and spans multiple commands.

Other changes don't need a scope.
