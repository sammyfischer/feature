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

schema:
  just run config schema > feature-config.schema.json

# format with dprint
fmt:
  dprint fmt --diff

# lint with clippy
lint:
  cargo clippy --all-targets --all-features -- -D warnings

# compliance checks
check:
   just fmt
   just lint
   cargo check
   just test

install:
  cargo install --path .

uninstall:
  cargo uninstall feature

# sets up the project (installs pre-commit hook)
init:
  #!/usr/bin/env bash
  cp pre-commit.sh .git/hooks/pre-commit
  chmod 775 .git/hooks/pre-commit
