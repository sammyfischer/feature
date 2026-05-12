# Config

Features has some config in dedicated toml files, and some in the git config file.

## Toml Config

The toml config files are meant to store semantic config for the repository. These config options affect all users on the team and may want to be standardized per repo. For that reason, it's separated into a dedicated config file and is recommended to be tracked by version control.

The config file is called `feature.toml` and should be located in the root of the repo.

In case you have several personal projects and want the same config for all, feature also supports a global config file located at `<config>/feature.config.toml`, where `<config>` is your platforms standard config dir. This file has lower precedence than the local config file.

Use `feature config create` to create a project config file with all defaults. Use `feature config create -g` to do the same with a global config file. Each command outputs the location of the newly created file. It's not recommended to leave this as-is. Customize the values you want and delete keys you want to leave as default.

> Note: arrays in different config levels overwrite each other. They don't attempt to append or combine in any way. This means that if you generate a default config at the project level, which contains an empty array for `protect`, then none of the branch names in your global config will be protected.

## Git Config

Feature stores personal preferences in the git config file. These are options that don't affect other developers such as formatting of terminal output.

Feature also respects some of git's own config options.

Below is a full example of git config values that feature uses.
If the variable has a default value, it's set in the example.
If the value is a default git option, it will contain "(builtin)" in the comment above.
If the value is a default git option that feature doesn't use, it will contain "(recommended)" in the comment above.

```toml
# ========================================
#         Default git options that
#            feature also reads
# ========================================

[advice]
    # Whether to show hints in status output.
    statusHints = yes

    # Whether to show advice on how to resolve conflicts when repo is
    # in a conflicted state.
    resolveConflict = yes

[status]
    # Whether to show untracked files in status output.
    showUntrackedFiles = yes

# ========================================
#     Default git options that feature
#     doesn't read, but may be useful
# ========================================

[format]
    # The default pretty format to be used by commands that support it.
    # "git log" uses this value, so it will affect the output of
    # "feature graph" if you don't specify a custom format for that.
    # The value here is not default, but it's a more useful one in my opinion.
    pretty = format:%C(auto)%h%d %C(reset)%s %C(dim)(%an, %ar)

    # Tip: this is a simpler value built in to git
    pretty = oneline

[log]
    # Whether to abbreviate commit hashes in log output. This affects "git log"
    # output, which may affect "feature graph" output.
    abbrevCommit = yes

# ========================================
#       Feature-specific options
# ========================================

[feature]
    # The name to use if branch template contains "%(user)". If not specified,
    # and the template contains "%(user)", creating a branch will error.
    user = username

# Config for the end command
[feature "end"]
    # Whether to try deleting the branch from remote when calling "feature end".
    remote = no

# Config for the list command
[feature "list"]
    # Whether to show the hash column.
    hash = yes

    # Whether to show the upstream column.
    upstream = yes

    # Whether to show the base column.
    base = yes

# Config for the show command
[feature "show"]
    # How much of the commit message to show. Valid values: none, subject, full.
    message = full

    # Whether to show diff summary.
    summary = yes

    # Whether to show diff patch.
    patch = no

    # When to page output. Valid values: auto, always, never.
    paging = auto

# Config for the status command
[feature "status"]
    # Whether to show submodule info in status output
    showModules = yes

# Config for the sync command
[feature "sync"]
    # Whether to automatically prune during the sync command.
    prune = yes

# Various formatting options used by feature
[feature "format"]
    # The pretty format passed to the underlying "git log" call. Passed in as
    # "git log --pretty=<value>". When unset, defaults to the "format.pretty"
    # builtin option. See "man git-log" for possible values. This isn't the
    # default option, but one that I find nice.
    graph = format:%C(auto)%h%d %C(green)%an %C(blue)%ar %C(reset)%s

    # Tip: this is a simpler value built in to git
    graph = oneline

    # The formatting to use for absolute timestamps. Uses strftime format
    # specifier.
    date = %b %d, %Y at %I:%M %p

    # Tip: use this for 24-hour time
    date = %b %d, %Y at %H:%M

    # Tip: and this for numeric dates
    date = %Y-%m-%d %H:%M

    # Whether to use relative or absolute times. This option isn't respected in
    # places where it doesn't make sense to. For example, when creating a new
    # commit, the timestamp will always be absolute, since the commit just
    # occured.
    relative = no
```
