# Todo List

## Housekeeping

- push non-current branch
  - currently some of the code assumes pushes occur on current branch, but the cli supports specifying any branch
- support remaining commit hooks. order:
  - prepare-commit-msg (processing on msg, before editor is invoked)
  - commit-msg (post-processing on msg, after editor)
  - pre-commit
  - post-commit
  - post-rewrite (after amend/rebase only)
- completions
  - known bug: `--no-upstream=...` doesn't autocomplete boolean values (this is true for all bool flags that require '=')
  - read `--git-dir` and `--work-tree` to generate completions for the correct repo
  - add support for `--flag=value` syntax
  - custom zsh completions, maybe fish too
- update is buggy and weird
  - tests need to be way more rigorous so I can get this to work once and for all
- split config into cosmetic and semantic
  - cosmetic config can go in git config
  - semantic config (default remote, protected branches) can stay in feature config, but there doesn't need to be a global one
- show
  - handle merge commits in different ways (currently shows diff against first parent)
- improve test environment
  - make one home dir in each test, create all repos and dirs inside it
- run git gc every now and then
  - maybe in write commands like sync

## Features

- release / version tags
  - maybe a single command to create and push a tag
- start: better user substitution
  - using git user.name isn't very good since it often has capitalization and spaces
  - needs a dedicated config variable, "feature.user" in git config
- undo
  - uses reflog, undoes latest change
- stash
  - more intuitive options to stash (--all => workdir/index, --unstaged => workdir, --staged => index)
  - action should be a flag, not positional (and should --push by default)
  - concatenate args as message
  - pretty output
- submodule aware output
  - status
  - list
- mod (submodule commands)
  - `ft mod sync` - run sync in all modules
  - `ft mod start` - start a branch with the same name in each module
  - each module can have its own feature config
- worktree
  - open an interactive menu to pick a branch and create a worktree from it
  - or use specified branch in command line
- reflog
  - view reflog for a branch, select one to restore to that state
