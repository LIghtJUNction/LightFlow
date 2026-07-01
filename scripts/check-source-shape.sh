#!/usr/bin/env sh
set -eu

usage() {
  cat <<'USAGE'
usage:
  scripts/check-source-shape.sh
  scripts/check-source-shape.sh --self-test

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
self_test=false

if [ "$#" -gt 0 ]; then
  case "$1" in
    --self-test)
      self_test=true
      ;;
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

run_self_test() {
  tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/lightflow-source-shape.XXXXXX")
  trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

  ok_file="$tmp_dir/semantic_name.rs"
  long_file="$tmp_dir/large_semantic_name.rs"
  numeric_file="$tmp_dir/001_generated_name.rs"
  generated_file="$tmp_dir/999_generated.rs"

  printf 'fn main() {}\n' > "$ok_file"
  awk 'BEGIN { for (i = 0; i < 501; i++) print "// line" }' > "$long_file"
  printf 'fn main() {}\n' > "$numeric_file"
  printf '//@generated\n' > "$generated_file"
  awk 'BEGIN { for (i = 0; i < 501; i++) print "// generated line" }' >> "$generated_file"

  check_file "$ok_file"
  [ "$failed" -eq 0 ] || return 1

  check_file "$long_file" >/dev/null
  [ "$failed" -eq 1 ] || return 1

  failed=0
  check_file "$numeric_file" >/dev/null
  [ "$failed" -eq 1 ] || return 1

  failed=0
  check_file "$generated_file"
  [ "$failed" -eq 0 ] || return 1

  echo "scripts/check-source-shape.sh --self-test: ok"
}

if [ "$self_test" = true ]; then
  run_self_test
  exit $?
fi

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
