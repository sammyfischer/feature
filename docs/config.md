# Config

Feature supports both a project/local config file and a global/user config file.

The project file takes precedence and is designed to be tracked by version control. It should contain project-specific config for all developers on the team to use, e.g. names of base branches and common formatting options.

The global config file exists in your platforms standard location. There you can customize your own general preferences.

Use `feature config create` to create a project config file with all defaults. Use `feature config create -g` to do the same with a global config file. Each command outputs the location of the newly created file. It's not recommended to leave this as-is. Customize the values you want and delete keys where you want to use the lower-level config values instead.

> The default config includes an empty array for protected branches. When configs get layered and merged together, empty arrays will overwrite the entire array set at lower levels. In other words, if you have protected branches in your global config, and your project contains an empty `protect` array, then no branches will be protected.
