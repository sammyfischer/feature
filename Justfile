set shell := ["bash", "-c"]

# list recipes
[default]
help:
  just --list

# run app (forwards args)
run *args:
  cargo run -- {{args}}

# run tests
test target="":
  #!/usr/bin/env bash
  if [ -n "{{target}}" ]; then
    echo "Testing {{target}}"
    cargo test --test {{target}} --all-features
  else
    echo "Testing all"
    cargo test --all-features
  fi

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

# generate schema
schema:
  just run config schema > resources/config.schema.json

# generate shell completions
comp:
  just run completions bash > resources/completions.bash
  just run completions elvish > resources/completions.elvish
  just run completions fish > resources/completions.fish
  just run completions powershell > resources/completions.pwsh
  just run completions zsh > resources/completions.zsh

# build test container
container-build:
  podman build -t localhost/feature-test .

# run tests in container
container-test:
  #!/usr/bin/env bash
  docker=$(which docker || which podman)
  "$docker" compose run --rm test

# check for compliance, generate up-to-date resources
release:
  just fmt
  just lint
  cargo check
  just container-build
  just container-test
  just schema

tag:
  #!/usr/bin/env bash
  set -euo pipefail
  
  version=$(grep -m1 '^version' Cargo.toml | sed 's/.*= *"\(.*\)"/\1/')
  if [[ -z "$version" ]]; then
    echo "Error: could not parse version from Cargo.toml" >&2
    exit 1
  fi

  tag="v${version}"
  git tag "$tag"
  echo "Created tag: $tag"

install:
  cargo install --path .

uninstall:
  cargo uninstall feature

# sets up the project (installs pre-commit hook)
init:
  #!/usr/bin/env bash
  cp pre-commit.sh .git/hooks/pre-commit
  chmod 775 .git/hooks/pre-commit
