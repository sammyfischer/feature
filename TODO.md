# Todo List

## Housekeeping

- support `commit.template` config option
  - template file that is used when -m is unspecified
- custom zsh completions, maybe fish too
- show
  - handle merge commits in different ways (currently shows diff against first parent)
- improve test environment
  - make one home dir in each test, create all repos and dirs inside it
- run git gc every now and then
  - maybe in write commands like sync

## Features

- auto merge/rebase
  - when branches have diverged, preventing a push, check if a merge/rebase would result in conflicts, then do it automatically
  - use default git push config to determine whether to merge or rebase
- release / version tags
  - maybe a single command to create and push a tag
- stash
  - more intuitive options to stash (--all => workdir/index, --unstaged => workdir, --staged => index)
  - action should be a flag, not positional (and should --push by default)
  - concatenate args as message
  - pretty output
- simplified worktree command
  - `feature wt add <BRANCH>` would create a worktree checked-out to the branch, create new branch if it doesn't exist already
  - would also have `rm`, `list`, maybe `mv`
