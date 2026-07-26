#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <about.toml> <deny.toml>" >&2
  exit 2
fi

source_config=$1
output_config=$2
temporary_config="${output_config}.tmp"

trap 'rm -f "$temporary_config"' EXIT

awk '
  BEGIN {
    in_list = 0
    found = 0
  }
  /^[[:space:]]*accepted[[:space:]]*=/ {
    sub(/accepted[[:space:]]*=/, "allow =")
    print "[licenses]"
    print
    in_list = 1
    found = 1
    next
  }
  in_list {
    print
    if ($0 ~ /^[[:space:]]*\][[:space:]]*$/) {
      exit
    }
  }
  END {
    if (!found) {
      exit 1
    }
  }
' "$source_config" > "$temporary_config"

cat >> "$temporary_config" <<'EOF'
confidence-threshold = 0.8

[licenses.private]
ignore = false
EOF

mv "$temporary_config" "$output_config"
