#!/usr/bin/env bash
# T28: the product implementation, release path, and website are Rust-only.
set -euo pipefail

failures=0

fail() {
  printf 'check-no-legacy-js: %s\n' "$*" >&2
  failures=$((failures + 1))
}

legacy_paths=(
  compiler
  runtime
  byg
  fuzz
  bench
  package.json
  jsr.json
  publish.js
  .npmignore
  .github/workflows/publish.yml
  porf
  porf.cmd
  logo.png
  test262/compare.js
  test262/fails.cjs
  test262/generateHistoricalData.js
  test262/index.js
  test262/missingHarness.js
  test262/read.js
  test262/history.json
  test262/harness.js
  test262/harness-wasm-aot.js
)

for path in "${legacy_paths[@]}"; do
  # Check the working tree as well as the index: a pre-commit deletion should
  # pass, while an untracked attempt to restore a retired path should fail.
  if [ -L "$path" ]; then
    fail "legacy path exists: $path"
  elif [ -d "$path" ]; then
    legacy_file="$(find "$path" -mindepth 1 \( -type f -o -type l \) -print -quit)"
    if [ -n "$legacy_file" ]; then
      fail "legacy root contains a file: $legacy_file"
    fi
  elif [ -e "$path" ]; then
    fail "legacy path exists: $path"
  fi
done

# Git must answer every query below: outside a checkout, or for a checkout git
# refuses (for example "dubious ownership"), it exits above 1 and this check
# fails instead of finding nothing.
git_status=0
tracked_files="$(git ls-files)" || git_status=$?
if [ "$git_status" -ne 0 ]; then
  fail "git ls-files failed with status $git_status"
fi
tracked_targets="$({
  printf '%s\n' "$tracked_files" | grep -E '(^|/)target/' | while IFS= read -r tracked; do
    if [ -e "$tracked" ] || [ -L "$tracked" ]; then
      printf '%s\n' "$tracked"
    fi
  done
} || true)"
if [ -n "$tracked_targets" ]; then
  fail "nested build/cache artifacts are tracked"
  printf '%s\n' "$tracked_targets" >&2
fi

workflow_files=()
while IFS= read -r path; do
  workflow_files+=("$path")
done < <(
  find .github/workflows -maxdepth 1 -type f \
    \( -name '*.yml' -o -name '*.yaml' \) -print 2>/dev/null \
    | sort
)

if [ "${#workflow_files[@]}" -gt 0 ]; then
  workflow_matches="$({
    grep -HEni \
      '(^|[^[:alnum:]_-])(node|deno|npm|npx|bun|jsr)([^[:alnum:]_-]|$)|uses:[^#]*setup-(node|deno|bun)' \
      "${workflow_files[@]}" || true
  })"
  workflow_commands="$(printf '%s\n' "$workflow_matches" | python3 scripts/check-benchmark-workflow-tools.py)"
  if [ -n "$workflow_commands" ]; then
    fail "first-party workflows invoke retired JavaScript tooling"
    printf '%s\n' "$workflow_commands" >&2
  fi
fi

# Tracked and untracked, non-ignored files, as ripgrep would see them, without
# requiring ripgrep: a missing tool must not silently pass this check.
# git grep exits 1 for no match and above 1 when it cannot search.
git_status=0
removed_entrypoint_references="$(
  git grep --untracked -nE \
    'runtime/index\.js|compiler/wrap\.js|test262/(compare\.js|fails\.cjs|generateHistoricalData\.js|index\.js|missingHarness\.js|read\.js|history\.json)' \
    -- \
    crates \
    scripts \
    README.md \
    CONTRIBUTING.md \
    AGENTS.md \
    ':(exclude,glob)**/check-no-legacy-js.sh'
)" || git_status=$?
if [ "$git_status" -gt 1 ]; then
  fail "git grep for removed entrypoint references failed with status $git_status"
fi
if [ -n "$removed_entrypoint_references" ]; then
  fail "current source or developer documentation references removed entrypoints"
  printf '%s\n' "$removed_entrypoint_references" >&2
fi

git_status=0
git ls-files --error-unmatch index.html >/dev/null 2>&1 || git_status=$?
if [ "$git_status" -gt 1 ]; then
  fail "git ls-files for index.html failed with status $git_status"
fi
if [ "$git_status" -eq 0 ]; then
  website_imports="$({
    grep -Eni \
      '(fetch|import|src=)[^[:cntrl:]]*(compiler|runtime)[^[:cntrl:]]*\.js' \
      index.html || true
  })"
  if [ -n "$website_imports" ]; then
    fail "website imports or fetches retired compiler/runtime JavaScript"
    printf '%s\n' "$website_imports" >&2
  fi
fi

if [ "$failures" -ne 0 ]; then
  exit 1
fi

printf 'check-no-legacy-js: ok\n'
