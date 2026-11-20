#!/usr/bin/env bash
# Runs scripts/check-module-boundaries.sh against stub rule files and checks
# that every failure is recorded exactly once and that nothing unexpected can
# end in "ok": failures inside subshells, pipeline loops, process and command
# substitutions, failures returned by functions and sourced files, count rules
# whose input is missing, an unbound variable, and a finding log that cannot be
# created, read or written.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
guard_source="$repo_root/scripts/check-module-boundaries.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

regressions=0
regression() {
  printf 'check-module-boundaries harness: %s\n' "$*" >&2
  regressions=$((regressions + 1))
}

# Each case gets a fresh tree: the guard's own script, the three files its
# prelude requires, empty rule files, and the case body as ir.sh.
run_case() {
  local name="$1" body="$2" tmpdir="${3:-}"
  local tree="$work/$name"
  mkdir -p "$tree/scripts/module-boundaries" "$tree/tmp" \
    "$tree/crates/scriptw-ir/src" "$tree/crates/scriptw-aot-wasm/src/builtins"
  cp "$guard_source" "$tree/scripts/check-module-boundaries.sh"
  : > "$tree/crates/scriptw-ir/src/lib.rs"
  : > "$tree/crates/scriptw-aot-wasm/src/lib.rs"
  : > "$tree/crates/scriptw-aot-wasm/src/builtins/mod.rs"
  for rules in wasm regexp-objects intl native-source; do
    : > "$tree/scripts/module-boundaries/$rules.sh"
  done
  printf '%s\n' "$body" > "$tree/scripts/module-boundaries/ir.sh"
  set +e
  (cd "$tree" && TMPDIR="${tmpdir:-$tree/tmp}" bash scripts/check-module-boundaries.sh \
    > "$tree/stdout" 2> "$tree/stderr")
  case_status=$?
  set -e
  case_stdout="$(cat "$tree/stdout")"
  case_stderr="$(cat "$tree/stderr")"
}

expect_findings() {
  local name="$1" expected="$2" body="$3"
  run_case "$name" "$body"
  if [ "$case_status" -ne 1 ] \
    || [[ "$case_stderr" != *"check-module-boundaries: $expected findings"* ]] \
    || [[ "$case_stdout" == *ok* ]]; then
    regression "$name: expected exit 1 with $expected findings, got exit $case_status" \
      "stdout=[$case_stdout] stderr=[$case_stderr]"
  fi
}

expect_ok() {
  local name="$1" body="$2"
  run_case "$name" "$body"
  if [ "$case_status" -ne 0 ] || [ "$case_stdout" != 'check-module-boundaries: ok' ]; then
    regression "$name: expected ok, got exit $case_status stdout=[$case_stdout] stderr=[$case_stderr]"
  fi
}

# No "ok", a non-zero status other than 1 (an aborted run, not a finding count),
# and the given diagnostic.
expect_abort() {
  local name="$1" diagnostic="$2" body="$3" tmpdir="${4:-}"
  run_case "$name" "$body" "$tmpdir"
  if [ "$case_status" -eq 0 ] || [ "$case_status" -eq 1 ] \
    || [[ "$case_stdout" == *ok* ]] \
    || [[ "$case_stderr" != *"$diagnostic"* ]]; then
    regression "$name: expected an aborted run reporting [$diagnostic], got exit $case_status" \
      "stdout=[$case_stdout] stderr=[$case_stderr]"
  fi
}

expect_ok clean ':'
expect_ok exempt_contexts '
if false; then :; fi
while false; do :; done
false || true
false && true
! false
captured="$(false)" || true
if [ -n "$(false)" ]; then :; fi
( false ) || true'

# A failing command inside a subshell form used to abort that subshell under
# `set -e` and so fail the guard; each must still be one finding.
expect_findings subshell_group 1 '( false; true )'
expect_findings pipeline_loop_body 1 "printf 'a\\n' | while read -r line; do false; true; done"
expect_findings non_final_pipeline_stage 1 'false | true'
expect_findings process_substitution 1 'while read -r line; do :; done < <(false)'
expect_findings command_substitution_argument 1 '[ -z "$(false; true)" ]'
expect_findings assignment_from_failure 1 'captured="$(false)"'

# A function's or a sourced file's failing last command returns its status to
# the call site; that is the same failure, not a second one.
expect_findings function_last_command 1 'last_fails() { false; }
last_fails'
expect_findings nested_function_last_command 1 'inner() { false; }
outer() { inner; }
outer'
expect_findings repeated_call_site 2 'last_fails() { false; }
for attempt in 1 2; do last_fails; done'
expect_findings sourced_file_last_command 1 'false'
expect_findings distinct_failures 3 'false
false
( false )'

# Count and budget rules whose input is missing are findings, never passes.
expect_findings missing_fixed_string_count 1 "require_fixed_string_count missing.rs needle 0 'absent needle'"
expect_findings missing_exact_line_count 1 "require_exact_line_count missing.rs line 0 'absent line'"
expect_findings missing_regex_count 1 "require_regex_count missing.rs 'x' 0 'absent pattern'"
expect_findings missing_tree_count 1 "require_tree_regex_count missing-tree 'x' 0 'absent declaration'"
expect_findings missing_raw_budget 1 'check_raw_line_budget missing.rs 10'
expect_findings missing_code_budget 1 'check_orchestration_surface missing.rs 10'
expect_findings invalid_regex_count 1 "require_regex_count crates/scriptw-ir/src/lib.rs '(' 0 'invalid pattern'"
expect_findings present_count_mismatch 1 "require_fixed_string_count crates/scriptw-ir/src/lib.rs needle 1 'needle'"
expect_ok present_count_match "require_fixed_string_count crates/scriptw-ir/src/lib.rs needle 0 'needle'"

# A run that cannot finish or cannot record what it found never passes.
expect_abort unbound_variable 'run ended early' ': "$undefined_rule_variable"'
expect_abort uncreatable_log 'cannot create the finding log' ':' "$work/nonexistent-dir"
expect_abort removed_log 'cannot read the finding log' 'rm -f "$failure_log"'
expect_abort unwritable_log 'cannot record a finding' 'rm -f "$failure_log"
mkdir "$failure_log"
false'
expect_abort unwritable_log_from_subshell 'cannot record a finding' 'rm -f "$failure_log"
mkdir "$failure_log"
( false; true )
printf "main shell kept running\n" >&2'
if [[ "$case_stderr" == *'main shell kept running'* ]]; then
  regression "unwritable_log_from_subshell: the main shell ran on after a subshell could not record its finding"
fi

if [ "$regressions" -ne 0 ]; then
  printf 'check-module-boundaries harness: %s regressions\n' "$regressions" >&2
  exit 1
fi
printf 'check-module-boundaries harness: ok\n'
