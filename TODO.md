# Todo List

## Housekeeping

- use `NOT_ON_BRANCH_MSG` everywhere
- separate core module/crate
  - move all common/backend functionality into a separate module (should replace util module)
  - cli module becomes just a frontend, calls common functions
  - config and data modules should be part of core
  - all terminal printing/formatting should be handled in cli, with common code in a `display` module
- clearer naming
  - every cli subcommand is a struct called `Args`, they should be renamed to contain the command name e.g. `StartArgs`
- cli group commands should be organized in modules
  - config/project commands are currently in one file, they should be split up
  - `config_command.rs` should probably be renamed to `config.rs`
- paging
  - accurate pager resolution: GIT_PAGER -> core.pager -> PAGER -> less -FR -> stdout
- per-command config
  - add global feature options like `feature.relativeTime`, `feature.paging`
  - add override for each command like `feature.stash.relativeTime`, `feature.tags.paging`
- support `commit.template` config option
  - template file that is used when -m is unspecified
- custom zsh completions, maybe fish too
- show
  - option for different display methods for merge commits (currently only supports first-parent)

## Features

- `squash`
  - squashes all commits on a feature branch into one
- `stash`
  - create parent commits containing staged, unstaged, and untracked. use to build diffs and restore changes correctly
  - possibly rename command to `wip`, since it's designed for per-feature, in-progress work
  - could be an option to operate on the default stash, `refs/stash`
  - sync should check each stash and delete all dangling stash references
- `wip`
  - quick creation of wip commits
  - standard message format: "WIP on <branch>: message"
  - tbh this isn't hard to do with regular commits or stashes
- auto merge/rebase
  - when branches have diverged, preventing a push, check if a merge/rebase would result in conflicts, then do it automatically
  - use default git push config to determine whether to merge or rebase
- simplified worktree command
  - `feature wt add <BRANCH>` would create a worktree checked-out to the branch, create new branch if it doesn't exist already
  - would also have `rm`, `list`, maybe `mv`
