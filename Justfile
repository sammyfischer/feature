set shell := ["bash", "-c"]

# list recipes
[default]
help:
  just --list

# run app (forwards args to app, not cargo)
run *ARGS:
  cargo run -- {{ARGS}}

# run tests
test:
  cargo test --package feature --test mod

# format with dprint
fmt:
  dprint fmt

# lint with clippy
lint:
  cargo clippy
