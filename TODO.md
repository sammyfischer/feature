# Todo List

## Housekeeping

- support remaining commit hooks. order:
  - prepare-commit-msg (processing on msg, before editor is invoked)
  - commit-msg (post-processing on msg, after editor)
  - pre-commit
  - post-commit
  - post-rewrite (after amend/rebase only)
- parallelism
  - subproject and submodule iteration can be done in parallel
  - this would affect status, list, sync, prune, and proj list
  - buffer output of each call, then print in sections at the end
  - sync and prune in particular should return a list of actions (update/delete + branch name + info), the calling function should handle output
- custom zsh completions, maybe fish too
- update is buggy and weird
  - tests need to be way more rigorous so I can get this to work once and for all
- show
  - handle merge commits in different ways (currently shows diff against first parent)
- improve test environment
  - make one home dir in each test, create all repos and dirs inside it
- run git gc every now and then
  - maybe in write commands like sync

## Features

- projects
  - `feature project each` should support filter by project names
- submodules
  - `feature sync` should checkout modules to expected commit
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
