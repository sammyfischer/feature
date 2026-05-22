#!/bin/bash

file="$1"
type="$2"
obj="$3"

old_msg="$(cat "$file")"

case "$type" in
  message)
    prefix="from command line:"
    ;;
  template)
    prefix="from template:"
    ;;
  merge)
    prefix="from MERGE_MSG:"
    ;;
  squash)
    prefix="from SQUASH_MSG:"
    ;;
  commit)
    prefix="from commit ${obj:0:7}:"
    ;;
  *)
    echo "Unrecognized commit type: $type"
    exit 1
esac

printf "%s" "$prefix $old_msg" > "$file"
