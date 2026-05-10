# Shell Completions

# Install

Use the command `feature completions <shell>` to print completions for your shell. You can redirect this output into a file and install it for your particular shell, or evaluate the output directly in your shell config file.

Examples in bash:

```bash
# run this once in the command line
feature completions bash > ~/bash_completion.d/feature

# or add this to your bashrc
eval "$(feature completions bash)"
```

## Terminology

Here are some terms that will may used in this document

**Static completions** - Shell completions that are known at compile time. This includes flags, subcommands, and enum values.

**Dynamic completions** - Shell completions that cannot be known at compile time. Some examples are git branches and remote names.

**Positional arg, positional** - A command line argument that is interpreted based on order.

**Flag** - A command line argument that is interpreted based on its name, and may appear in different orders relative to other arguments.

## Supported Shells

All major shells currently support static completions (via auto-generated scripts by `clap_complete`). Bash is the only shell that supports dynamic completions (via a fully handwritten script), but dynamic completions in zsh are planned. Other shells may or may not receive dynamic completion support in the future.

## Exhaustiveness

When invoking completions, there are 2 cases where the completions aren't necessarily exhaustive:

1. the argument is positional *and* its completions are dynamic
2. the argument is positional and its values are arbitrary

To understand the first case, take this example, where the '#' represents the cursors current position:

```bash
feature show #
```

The true list of possible values is every flag supported by `show` plus every revspec in the repository (excluding hashes and permutations like `HEAD^1`).

In order to reduce the number of values, invoking completions here will only show revspecs. If you want to see possible flags, type a '-' first.

```bash
feature show -#
```

This will filer the list to flags only. The case where a revspec starts with a '-' is not supported and must be typed manually.

To understand the second case, take this example:

```bash
feature commit #
```

The full list of possible values are flags and arbitrary words that make up the commit message. Of course that makes an infinite set of values, so when completing for arbitary words the completion script will always return an empty list.

For this case, a list of options will be shown if the current word is empty (like in the example) or if it starts with a '-'. Otherwise, no completions will be shown.

Any commands that *only* have static completions will show the exhaustive list.

## Correctness of Completions

Completions being provided doesn't imply that they are semantically valid in that position. Some shortcuts are made in the completion script to reduce its complexity.

One example is when completing for commands that take arbitrary words, like `commit` and `start`. If you type something like:

```bash
feature start -- branch name #
```

And your cursor is at the '#', invoking completions will show you the list of options that `start` takes. This is of course not representative of how the arg will be used. It will be taken literally and included in the branch name, since it appears after a "--" or after the first positional (in this case it's both, but in general only one of those conditions needs to be true).

The completion script only checks for a few things:

- the subcommand (e.g. `start`, `config get`)
- the previous word
- the current word

Every decision is based on these values.

To make sure commands do what you expect, you should:

- put all flags before positionals for a particular command
- understand how positionals will be interpreted for a particular command
- know whether a flag you're using takes a value

In other words, check the help outputs.

# '=' Syntax

All flags can have their value specified with an '=', like this:

```bash
feature start --from="main" ...
```

Some flags require this syntax. For example:

```bash
# valid
feature show --no-summary=false main
feature list -S=false main

# invalid
feature list --no-summary false main
feature list -S false main
```

This is because these options in particular can be used without a value at all (which implies true). Parsing this would be ambiguous otherwise ("false" could be a valid revspec).

These commands *do* support completions with the '=' syntax, since it's the only valid syntax.

> Note: Completions for these args are somewhat buggy. They'll be fixed eventually.
