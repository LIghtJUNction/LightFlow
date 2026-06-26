#!/usr/bin/env sh
set -eu

usage() {
  cat <<'USAGE'
usage:
  scripts/check-source-shape.sh

Checks first-party source files under src/ for:
  - files over 500 lines
  - meaningless numeric filenames with a 3+ digit prefix (e.g. 001_foo.rs)

Files can be opted out when they are generated and explicitly documented with a
top-of-file marker comment such as `@generated`.
USAGE
}

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
src_root=$repo_root/src
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

if [ ! -d "$src_root" ]; then
  echo "Source root not found: $src_root" >&2
  exit 1
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
$(find "$src_root" -type f -name '*.rs')
EOF

if [ "$failed" -ne 0 ]; then
  echo "scripts/check-source-shape.sh: failed" >&2
  exit 1
fi

echo "scripts/check-source-shape.sh: ok"
