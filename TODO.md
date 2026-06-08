# Todo List

## Housekeeping

- support `commit.template` config option
  - template file that is used when -m is unspecified
- custom zsh completions, maybe fish too
- show
  - option for different display methods for merge commits (currently only supports first-parent)

## Features

- auto merge/rebase
  - when branches have diverged, preventing a push, check if a merge/rebase would result in conflicts, then do it automatically
  - use default git push config to determine whether to merge or rebase
- release / version tags
  - maybe a single command to create and push a tag
  - command to create a new tag incremented by major, minor, or patch
- stash
  - more intuitive options to stash (--all => workdir/index, --unstaged => workdir, --staged => index)
  - if action is optional, it should be a flat (--push by default), otherwise positional is fine
  - concatenate args as message
  - pretty output
- simplified worktree command
  - `feature wt add <BRANCH>` would create a worktree checked-out to the branch, create new branch if it doesn't exist already
  - would also have `rm`, `list`, maybe `mv`
