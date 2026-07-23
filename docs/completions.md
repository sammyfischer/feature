# Shell Completions

# Install

Use the command `feature completions <shell>` to print completions for your shell. You can redirect this output into a file and install it for your particular shell, or evaluate the output directly in your shell config file.

In bash, the simplest way to get completions is to add this to your `.bashrc`:

```bash
eval "$(feature completions bash)"
```

If you have a directory where you typically store completions, e.g. (`~/bash_completion.d`), then you can send the output to a file in that directory:

```bash
feature completions bash > ~/bash_completion.d/feature
```

If you use the `bash-completion` utility, run this:

```bash
feature completions bash > ~/.local/share/bash-completion/completions/feature.bash
```

To enable completions with a bash alias, create another file in the same directory called `<alias>.bash` with these contents:

```bash
source ~/.local/share/bash-completion/completions/feature.bash
complete -F _feature -o nosort -o bashdefault -o default <alias>
```

# Supported Shells

Most major shells currently support static completions (via auto-generated scripts by `clap_complete`). Bash is the only shell that supports dynamic completions (via a fully handwritten script), but dynamic completions in zsh are planned. Other shells may or may not receive dynamic completion support in the future.

*Static completions* refer to completions that are constant, and independent of any context. These include names of options, subcommands, and enum values.

*Dynamic completions* refer to completions that depend on context. In the case of feature, this is any completion that comes from the git repository, e.g. branch names.

# Notes

Completions are designed to be reasonably intuitive, and are implemented on a best-effort basis.
For example, when completing wipspecs, the completions provided will always be branch names.
These are semantically valid, but no attempt is made to complete wip indices.

There are other cases where simplifications like this may be made, but they should still be intuitive, as long as you familiarize yourself with the commands using the help menus.

## Completing Nested Commands

The command `feature project each ...` takes an entire command as an argument, much like `sudo`. To get completions on the provided command, you need the `bash-completion` utility.

For example, if you press tab at the end of this command:

```bash
feature project each git switch m
```

It will autocomplete branch names as `git switch` would normally.

The one caveat is that these completions will be based on the *current* repository.
If your current repo has a branch named "main", it may complete that in the command, without regard for whether the projects have a branch with that name.

It will also complete filenames relative to the current directory, but resolve them relative to the directory they are run in (i.e. each project dir).

There's no reasonable or consistent way to resolve this, so just be aware that completions here may be misleading.

## Equals Sign Syntax

When completing for a file or dir name, completions may not work properly if the name starts with an '='. I don't currently plan on fixing this since it's rare and would make the completion script significantly more complicated.

This is because the '=' sign is considered a word separator in bash, and the script makes decisions based on whether the previous or current word is an '='.
