#!/bin/bash

file="$1"

msg="$(cat "$file")"
printf "fix: %s" "$msg" > "$file"
