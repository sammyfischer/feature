# Removed Features

There are some features that got removed because they became redundant later on. I decided to list them here and show the alternative solution.

## Log

The log command used to just run `git log` with a custom format. It was also configurable in feature's config.

I found that git has config for this: `format.pretty`. It sets the default pretty format for all relevant commands, not just log. To get the same format that feature defaulted to, use this:

```bash
git config --global format.pretty 'format:%C(auto)%h%d %C(reset)%s %C(dim)(%an, %ar)'
```

It's also useful to specify `log.abbrevCommit`, since full commit hashes just clutter the output and are never practical.

```bash
git config --global log.abbrevCommit true
```

## Graph

The graph command was an easy way to run `git log --graph`. Like log, it had a custom default format and could be configured in feature's config.

An easy way to implement this is with git aliases. If you make an alias called "graph" you can run "git graph". Since it just expands to git log, you can even add additional arguments (specify a range of commits, add "--all", etc.).

To get the same default behavior as feature, use this:

```bash
git config --global alias.graph "log --graph --pretty='format:%C(auto)%h%d %C(green)%an %C(blue)%ar %C(reset)%s'"
```
