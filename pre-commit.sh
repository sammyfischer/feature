#!/bin/bash
set -euxo pipefail

# get staged files (filter only added, copied, modified)
staged=$(git diff --cached --name-only --diff-filter=ACM)

if [ -z "$staged" ]; then
  exit 0
fi

# format staged files
echo "$staged" | xargs dprint fmt
# add staged files to apply new formatting changes
echo "$staged" | xargs git add

# get staged rust files
staged_rs=$(echo "$staged" | grep '\.rs$' || true)

# lint and test if there are staged rust files
if [ -n "$staged_rs" ]; then
  cargo clippy
  cargo test
fi
