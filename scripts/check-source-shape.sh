#!/usr/bin/env sh
set -eu

usage() {
  cat <<'USAGE'
usage:
  scripts/check-source-shape.sh

Checks first-party Rust files in the repository for:
  - files over 500 lines
  - meaningless numeric filenames with a 3+ digit prefix (e.g. 001_foo.rs)

Files can be opted out when they are generated and explicitly documented with a
top-of-file marker comment such as `@generated`.
USAGE
}

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
max_lines=500
failed=0

if [ "$#" -gt 0 ]; then
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unsupported option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
fi

is_generated_file() {
  file=$1
  if grep -Eqm1 '^[[:space:]]*(//|/\*|#)[[:space:]]*@generated|Generated file' "$file"; then
    return 0
  fi
  return 1
}

is_numeric_prefix_file() {
  base=$(basename -- "$1")
  case "$base" in
    [0-9][0-9][0-9]*)
      return 0
      ;;
  esac
  return 1
}

check_file() {
  file=$1

  if is_generated_file "$file"; then
    return 0
  fi

  line_count=$(wc -l < "$file" | tr -d ' ')
  if [ "$line_count" -gt "$max_lines" ]; then
    echo "source-shape: over $max_lines lines ($line_count): $file"
    failed=1
  fi

  if is_numeric_prefix_file "$file"; then
    echo "source-shape: numeric-prefix filename: $file"
    failed=1
  fi
}

while IFS= read -r file; do
  check_file "$file"
done <<EOF
$(find "$repo_root" \
  \( -path "$repo_root/.git" \
    -o -path "$repo_root/.codegraph" \
    -o -path "$repo_root/target" \
    -o -path "$repo_root/vendor" \
  \) -prune \
  -o -type f -name '*.rs' -print)
EOF

if [ "$failed" -ne 0 ]; then
  echo "scripts/check-source-shape.sh: failed" >&2
  exit 1
fi

echo "scripts/check-source-shape.sh: ok"
