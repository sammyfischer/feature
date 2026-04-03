set shell := ["bash", "-c"]

# list recipes
[default]
help:
  just --list

# run app (forwards args to app, not cargo)
run *args:
  cargo run -- {{args}}

# run tests
test target="":
  #!/usr/bin/env bash
  if [ -n "{{target}}" ]; then
    echo "Testing {{target}}"
    cargo test --quiet --test {{target}}
  else
    echo "Testing all"
    cargo test --quiet
  fi

# format with dprint
fmt:
  dprint fmt

# lint with clippy
lint:
  cargo clippy

# compliance checks
check: fmt lint test

install:
  cargo install --path .

uninstall:
  cargo uninstall feature

# sets up the project (installs pre-commit hook)
init:
  #!/bin/bash
  cp pre-commit.sh .git/hooks/pre-commit
  chmod 775 .git/hooks/pre-commit
