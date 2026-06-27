# Todo List

## Housekeeping

- support `commit.template` config option
  - template file that is used when -m is unspecified
- custom zsh completions, maybe fish too
- show
  - option for different display methods for merge commits (currently only supports first-parent)

## Features

- `squash`
  - squashes all commits on a feature branch into one
- `stash`
  - custom stash mechanism, no interop with git stash
  - stashes are stored per branch, e.g. `refs/feature/stashes/branch-name`
    - in other words, each branch gets its own independent stash that works like the normal `refs/stash`
    - stash commands operate on the current branch, or take the branch as an argument
    - could be an option to operate on the default stash, `refs/stash`
- `wip`
  - quick creation of wip commits
  - standard message format: "WIP on <branch>: message"
  - tbh this isn't hard to do with regular commits or stashes
- `version` (`ver`)
  - working with semver tags
  - maybe a single command to create and push a tag
  - increment by major, minor, or patch
  - parse commits messages to determine what to increment
  - list all semver tags, sorted by most recent
  - `version --log` calls git log on commits since last version
  - feature config option: require annotated tags
    - this would make creating tags error if no message is provided
- auto merge/rebase
  - when branches have diverged, preventing a push, check if a merge/rebase would result in conflicts, then do it automatically
  - use default git push config to determine whether to merge or rebase
- simplified worktree command
  - `feature wt add <BRANCH>` would create a worktree checked-out to the branch, create new branch if it doesn't exist already
  - would also have `rm`, `list`, maybe `mv`
