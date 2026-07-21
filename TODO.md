# Todo List

## Housekeeping

Passive (keep in mind when writing/editing code)

- use `NOT_ON_BRANCH_MSG` everywhere
- make sure all printing/term code is in `cli`, backend logic is in `core`

High priority

- paging
  - accurate pager resolution: GIT_PAGER -> core.pager -> PAGER -> less -FR -> stdout
  - all paged output goes through this, instead of manually filtering diffs through delta
  - respect git's `pager.<command>` options (e.g. show)
- per-command config
  - add global feature options like `feature.relativeTime`, `feature.paging`
  - add override for each command like `feature.stash.relativeTime`, `feature.tags.paging`

Medium priority

- `wip` command
  - create parent commits containing staged and unstaged changes, so that popping will restore correctly
  - add options to show to view only staged/unstaged changes in a wip (i.e. diff against each a particular parent)
- clearer naming
  - every cli subcommand is a struct called `Args`, they should be renamed to contain the command name e.g. `StartArgs`
- error types in core
  - should return a custom error type rather than strings
  - should probably not depend on anyhow, but should support it

Low priority

- support `commit.template` config option
  - template file that is used when -m is unspecified
- custom zsh completions, maybe fish too
- show
  - option for different display methods for merge commits (currently only supports first-parent)
- add git stash support to wip command
  - i.e. allow wip command to operate on `refs/stash`
  - allow `wip mv` to move between git stash and feature wips

## Features

- `ignore/exclude`
  - really simple command to add things to `.gitignore` or `.git/info/exclude` from the command line
  - an `unignore` command would probably be useful too, fetches completions directly from the file
- `squash`
  - squashes all commits on a feature branch into one
- auto merge/rebase
  - when branches have diverged, preventing a push, check if a merge/rebase would result in conflicts, then do it automatically
  - use default git push config to determine whether to merge or rebase
- simplified worktree command
  - `feature wt add <BRANCH>` would create a worktree checked-out to the branch, create new branch if it doesn't exist already
  - would also have `rm`, `list`, maybe `mv`
