# list recipes
[default]
help:
  just --list

# run app
run:
  cargo run

# format with dpring
fmt:
  dprint fmt

# lint with clippy
lint:
  cargo clippy
