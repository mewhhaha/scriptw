#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
guard="$repo_root/scripts/check-readme-status-artifacts.sh"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

fail() {
  printf 'check-readme-status-artifacts regression: %s\n' "$*" >&2
  exit 1
}

test_repo="$tmp_dir/repo"
git init -q "$test_repo"
cd "$test_repo"
git config user.email test@example.invalid
git config user.name 'README status guard test'
cp "$repo_root/.gitignore" .gitignore

write_base_readme() {
  printf '%s\n' \
    '# Fixture' \
    'outside=base' \
    '## Current Status' \
    '<!-- scriptw-status:start -->' \
    'status=base' \
    '- `./scripts/publish-real-status-low-ram.sh wasm-aot codex-published-real`' \
    '<!-- scriptw-status:end -->' \
    'tail=base' > README.md
}

rewrite_readme() {
  local expression="$1"
  sed "$expression" README.md > README.md.next
  mv README.md.next README.md
}

change_generated_status() {
  rewrite_readme 's/status=base/status=changed/'
}

change_outside_status() {
  rewrite_readme 's/outside=base/outside=changed/'
}

change_refresh_to_spec_exec() {
  rewrite_readme 's/publish-real-status-low-ram.sh wasm-aot/publish-real-status-low-ram.sh spec-exec/'
}

change_refresh_to_wasm_alias() {
  rewrite_readme 's/publish-real-status-low-ram.sh wasm-aot/publish-real-status-low-ram.sh wasm/'
}

add_artifact() {
  local path="$1"
  mkdir -p "$(dirname "$path")"
  printf '{"fixture":true}\n' > "$path"
}

setup_outside_only() {
  change_outside_status
}

setup_readme_only() {
  change_generated_status
}

setup_node_snapshot() {
  change_generated_status
  add_artifact test262/snapshots/current-built-ins-123.json
}

setup_aggregate_snapshot() {
  change_generated_status
  add_artifact test262/snapshots/current-aggregate-123.json
}

setup_fake_snapshot() {
  change_generated_status
  add_artifact crates/scriptw-test262/tests/fixtures/fake_test262/snapshots/fake.json
}

setup_spec_exec_status() {
  change_generated_status
  add_artifact test262/snapshots/published-status-spec-exec.json
}

setup_canonical_json_only() {
  change_generated_status
  add_artifact test262/snapshots/published-status-wasm-aot.json
}

setup_canonical_txt_only() {
  change_generated_status
  add_artifact test262/snapshots/published-status-wasm-aot.txt
}

setup_canonical_pair() {
  change_generated_status
  add_artifact test262/snapshots/published-status-wasm-aot.json
  add_artifact test262/snapshots/published-status-wasm-aot.txt
}

setup_generation() {
  change_generated_status
  python3 - "$repo_root/scripts" "${1:-wasm-aot}" <<'PYGEN'
import hashlib, json, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from status_publication import generation_id
root = Path("test262/snapshots")
data = (json.dumps({"producer": "scriptw", "real_suite": {"backend": sys.argv[2]}}).encode(), b'fixture=true\n')
generation = generation_id(*data)
manifest = {"schema_version": 1, "backend": "wasm-aot", "generation": generation}
for field, suffix, payload in zip(("json", "text"), ("json", "txt"), data):
    name = f"published-status-wasm-aot-{generation}.{suffix}"
    (root / name).write_bytes(payload)
    manifest[field] = {"file": name, "sha256": hashlib.sha256(payload).hexdigest()}
(root / "published-status-wasm-aot.manifest.json").write_text(json.dumps(manifest))
PYGEN
}

setup_generation_oracle_payload() {
  setup_generation spec-exec
}

setup_generation_missing_member() {
  setup_generation
  rm test262/snapshots/published-status-wasm-aot-*.txt
}

setup_generation_damaged_member() {
  setup_generation
  printf 'changed\n' >> test262/snapshots/published-status-wasm-aot-*.txt
}

setup_generation_symlink_member() {
  setup_generation
  local member
  for member in test262/snapshots/published-status-wasm-aot-*.txt; do
    rm "$member"
    ln -s ../../README.md "$member"
  done
}

setup_canonical_pair_with_node() {
  setup_canonical_pair
  add_artifact test262/snapshots/current-built-ins-123.json
}

setup_canonical_pair_with_spec_exec() {
  setup_canonical_pair
  add_artifact test262/snapshots/published-status-spec-exec.json
}

setup_deleted_canonical_pair() {
  change_generated_status
  rm test262/snapshots/published-status-wasm-aot.json
  rm test262/snapshots/published-status-wasm-aot.txt
}

setup_canonical_symlink_pair() {
  change_generated_status
  rm test262/snapshots/published-status-wasm-aot.json
  rm test262/snapshots/published-status-wasm-aot.txt
  ln -s ../../README.md test262/snapshots/published-status-wasm-aot.json
  ln -s ../../README.md test262/snapshots/published-status-wasm-aot.txt
}

setup_reverse_t27_token_change() {
  change_refresh_to_spec_exec
}

setup_wasm_alias_t27_token_change() {
  change_refresh_to_wasm_alias
}

# A canonical pair would admit this change; the legacy marker must not.
setup_legacy_lila_markers() {
  setup_canonical_pair
  rewrite_readme 's/scriptw-status/lila-status/g'
}

setup_legacy_porffor_markers() {
  setup_canonical_pair
  rewrite_readme 's/scriptw-status/porffor-status/g'
}

setup_large_changed_file_list() {
  local index

  change_generated_status
  mkdir -p bulk
  for index in $(seq 1 3000); do
    printf 'fixture\n' > "bulk/changed-file-$index"
  done
}

write_base_readme
mkdir -p test262/snapshots
printf '{"fixture":"base"}\n' > test262/snapshots/published-status-wasm-aot.json
printf 'fixture=base\n' > test262/snapshots/published-status-wasm-aot.txt
git add -A
git commit -qm base
base_commit="$(git rev-parse HEAD)"

run_case() {
  local name="$1"
  local expected_status="$2"
  local setup="$3"
  local output
  local status

  git checkout -q -B "case-$name" "$base_commit"
  "$setup"
  git add -A
  git commit -qm "$name"

  set +e
  output="$("$guard" "$base_commit" 2>&1)"
  status=$?
  set -e

  if [ "$status" -ne "$expected_status" ]; then
    fail "$name returned $status, expected $expected_status: $output"
  fi
  printf 'check-readme-status-artifacts regression: %s ok\n' "$name"
}

run_case outside-generated-block 0 setup_outside_only
run_case readme-only 1 setup_readme_only
run_case node-snapshot 1 setup_node_snapshot
run_case aggregate-snapshot 1 setup_aggregate_snapshot
run_case fake-snapshot 1 setup_fake_snapshot
run_case spec-exec-status 1 setup_spec_exec_status
run_case canonical-json-only 1 setup_canonical_json_only
run_case canonical-txt-only 1 setup_canonical_txt_only
run_case canonical-pair 0 setup_canonical_pair
run_case canonical-pair-with-node 0 setup_canonical_pair_with_node
run_case canonical-pair-with-spec-exec 1 setup_canonical_pair_with_spec_exec
run_case deleted-canonical-pair 1 setup_deleted_canonical_pair
run_case coherent-generation 0 setup_generation
run_case oracle-generation-payload 1 setup_generation_oracle_payload
run_case missing-generation-member 1 setup_generation_missing_member
run_case damaged-generation-member 1 setup_generation_damaged_member
run_case symlink-generation-member 1 setup_generation_symlink_member
run_case canonical-symlink-pair 1 setup_canonical_symlink_pair
for canonical_path in \
  test262/snapshots/published-status-wasm-aot.json \
  test262/snapshots/published-status-wasm-aot.txt; do
  canonical_entry="$(git ls-tree HEAD -- "$canonical_path")"
  canonical_mode="${canonical_entry%% *}"
  if [ "$canonical_mode" != '120000' ]; then
    fail "$canonical_path symlink fixture has tree mode $canonical_mode, expected 120000"
  fi
done
printf 'check-readme-status-artifacts regression: canonical-symlink-tree-modes ok\n'
run_case reverse-t27-token-change 1 setup_reverse_t27_token_change
run_case wasm-alias-t27-token-change 1 setup_wasm_alias_t27_token_change
run_case large-changed-file-list 1 setup_large_changed_file_list
run_case legacy-lila-markers 1 setup_legacy_lila_markers
run_case legacy-porffor-markers 1 setup_legacy_porffor_markers

# Exercise the real repository comparison as a smoke check.
set +e
actual_output="$(cd "$repo_root" && "$guard" origin/main 2>&1)"
actual_status=$?
set -e
if [ "$actual_status" -ne 0 ]; then
  fail "actual origin/main comparison returned $actual_status: $actual_output"
fi
printf 'check-readme-status-artifacts regression: actual-origin-main-comparison ok\n'

printf 'check-readme-status-artifacts regression: all cases passed\n'
