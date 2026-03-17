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
    cargo test --test {{target}}
  else
    echo "Testing all"
    cargo test
  fi

# format with dprint
fmt:
  dprint fmt

# lint with clippy
lint:
  cargo clippy

install:
  cargo install --path .

uninstall:
  cargo uninstall feature
