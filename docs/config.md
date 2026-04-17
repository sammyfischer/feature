# Config

Feature supports both a project/local config file and a global/user config file.

The project file takes precedence and is designed to be tracked by version control. It should contain project-specific config for all developers on the team to use, e.g. names of base branches and branch name template. It should not contain command output preferences, e.g. advice config and date formatting.

The global config file exists in your platforms standard location. There you can customize your own general preferences.

Use `feature config create` to create a project config file with all defaults. Use `feature config create -g` to do the same with a global config file. Each command outputs the location of the newly created file. It's not recommended to leave this as-is. Customize the values you want and delete keys you want to leave as default.

> Note: arrays in different config levels overwrite each other. They don't attempt to append or combine in any way. This means that if you generate a default config at the project level, which contains an empty array for `protect`, then none of the branche names in your global will be protected. This also means that if you have `bases = ["main", "dev"]` in your global, and only `["main"]` in your project-level, then "dev" will not be considered a base.

## Command config

A few feature commands have their own config options. These will be in thier own sections to organize the options. Again, use `feature config create` to see all available options.

## Format

The format section contains formatting options that may or may not be used in multiple commands.

The option `format.relative` displays time in a relative format, e.g. "5 minutes ago" only in places where it doesn't semantically make sense to display it in a particular way. For example, `feature status` always displays a relative time because it's more useful to how recently the branch was modified. `feature commit` always shows an absolute timestamp, because the relative time would always be a few seconds ago. `feature show`, on the other hand, will respect the option because neither format is more applicable than the other.

## Advice

The advice section contains options to disable certain tips that commands might output. They don't necessarily this advice for every command, only in commands where they're not specifically relevant. For example, if you run `feature update` and hit a rebase conflict, feature will output the advice no matter what, because it's directly related to the result of the command. Specifying `advice.rebase = false` will, however, disable rebase conflict advice in `feature status` (though it will still tell you that you are in a rebase conflict state).

The default options for advice generally follow this heuristic:

- advice is disabled for extremely common scenarios
- advice is disabled for scenarios that the user entered intentionally (e.g. git bisect)
