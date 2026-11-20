#!/usr/bin/env bash
# One run reports every finding. `errexit` is deliberately off: a rule that hits
# a missing file or a failing helper must not end the run before the thousands of
# rule lines after it execute. Instead the ERR trap below records each command
# failure that `set -e` would have aborted on as one finding, and the run goes on.
#
# - The trap is exempt where errexit is: the conditions of `if`, `while` and
#   `until`, commands negated by `!`, and every command of an `&&`/`||` list but
#   the last. Under `pipefail` a pipeline fails when any of its stages fails, so
#   a failing non-final stage is a finding, as it was under `set -eo pipefail`.
# - `set -E` (errtrace) carries the trap into functions, `( ... )` groups,
#   pipeline stages such as a `| while` body, process substitutions and command
#   substitutions. A failure in any of these subshells is recorded in the shared
#   log, and the subshell then exits 0, so its parent does not record the same
#   failure a second time. This is stricter than errexit in one place: errexit
#   ignores a failure inside `$( ... )` unless it decides the status of a plain
#   assignment, while the trap records it.
# - A function or sourced file whose last command fails returns that status to
#   its call site, where the trap fires again for the same failure; that repeat
#   is recognised by its call site and not recorded.
# - The guard fails closed. A finding log that cannot be created or written ends
#   the run with status 2. A count rule whose file is missing, or whose count is
#   not a number, is a finding. A run that ends before the last rule (an unbound
#   variable under `set -u`, a signal) reports how far it got and never prints ok.
#
# scripts/tests/check-module-boundaries-harness.sh runs each of these cases.
set -uo pipefail
set -E

# Findings are appended to a log, so a `fail` from any subshell still counts.
if ! failure_log="$(mktemp)" || [ ! -f "$failure_log" ] || [ ! -w "$failure_log" ]; then
  printf 'check-module-boundaries: cannot create the finding log; no rule ran\n' >&2
  exit 2
fi
run_complete=0
finish() {
  local status=$? recorded
  trap - ERR
  if [ "$run_complete" != 1 ]; then
    recorded="$(wc -l 2>/dev/null < "$failure_log" | tr -d '[:space:]')" || true
    printf 'check-module-boundaries: run ended early with status %s after %s recorded findings; later rules did not run\n' \
      "$status" "${recorded:-an unknown number of}" >&2 || true
    # 1 means "every rule ran and found something"; an unfinished run is 2.
    if [ "$status" -lt 2 ]; then
      status=2
    fi
  fi
  rm -f "$failure_log"
  exit "$status"
}
trap finish EXIT

fail() {
  printf 'check-module-boundaries: %s\n' "$*" >&2 || true
  if ! printf 'finding\n' >> "$failure_log"; then
    printf 'check-module-boundaries: cannot record a finding in %s; aborting\n' "$failure_log" >&2 || true
    # A subshell cannot end the run by exiting, so it stops the main shell.
    if [ "$BASHPID" != "$$" ]; then
      kill -s TERM "$$"
    fi
    exit 2
  fi
}

# Call site of the frame that returned the last recorded failure, as
# "<frame depth> <status> <source>:<line>". The trap that fires there for the
# same status is that failure returning, not a new one.
returning_failure=''
command_failed() {
  local status="$1" command="$2" line="$3"
  # Frames below this handler; the handler runs in the frame that failed.
  local depth=$((${#FUNCNAME[@]} - 1))
  local site="${BASH_SOURCE[1]:-$0}:$line"
  if [ "$BASHPID" != "$$" ]; then
    fail "command failed with status $status at $site: $command"
    exit 0
  fi
  local repeat=0
  if [ "$returning_failure" = "$depth $status $site" ]; then
    repeat=1
  fi
  returning_failure="$((depth - 1)) $status ${BASH_SOURCE[2]:-$0}:${BASH_LINENO[1]:-0}"
  if [ "$repeat" = 0 ]; then
    fail "command failed with status $status at $site: $command"
  fi
}
trap 'command_failed "$?" "$BASH_COMMAND" "$LINENO"' ERR

# Succeeds only for a decimal count, so an empty or garbled count (a missing
# file, a failed reader) can never satisfy a rule.
is_count() {
  case "$1" in
    ''|*[!0-9]*) return 1 ;;
  esac
}

# Records a finding and fails when a rule's input file is missing.
rule_input_file() {
  if [ -f "$1" ]; then
    return 0
  fi
  fail "missing file: $1 (needed by: $2)"
  return 1
}

# Records the finding and returns 0: later rules still run. A caller that must
# skip work for a missing file tests `[ -f ]` itself.
require_file() {
  if [ ! -f "$1" ]; then
    fail "missing file: $1"
  fi
}

require_module_decl() {
  file="$1"
  module="$2"
  if ! grep -Eq "^(pub\\(crate\\) |pub )?mod ${module};$" "$file"; then
    fail "$file must declare module: $module"
  fi
}

require_pub_use() {
  file="$1"
  pattern="$2"
  description="$3"
  if ! grep -Eq "$pattern" "$file"; then
    fail "$file must re-export $description"
  fi
}

require_fixed_string_count() {
  file="$1"
  needle="$2"
  expected="$3"
  description="$4"
  rule_input_file "$file" "$expected $description sites" || return 0
  count="$(grep -Fc -e "$needle" "$file" || true)"
  if ! is_count "$count" || [ "$count" -ne "$expected" ]; then
    fail "$file must contain $expected $description sites (found ${count:-no count})"
  fi
}

require_fixed_string_present() {
  file="$1"
  needle="$2"
  description="$3"
  if ! grep -Fq "$needle" "$file"; then
    fail "$file must consume $description: $needle"
  fi
}

require_exact_line_count() {
  file="$1"
  line="$2"
  expected="$3"
  description="$4"
  rule_input_file "$file" "$expected exact $description lines" || return 0
  count="$(grep -Fxc -e "$line" "$file" || true)"
  if ! is_count "$count" || [ "$count" -ne "$expected" ]; then
    fail "$file must contain $expected exact $description lines (found ${count:-no count})"
  fi
}

require_regex_count() {
  file="$1"
  pattern="$2"
  expected="$3"
  description="$4"
  rule_input_file "$file" "$expected $description lines" || return 0
  count="$(grep -Ec -e "$pattern" "$file" || true)"
  if ! is_count "$count" || [ "$count" -ne "$expected" ]; then
    fail "$file must contain $expected $description lines (found ${count:-no count})"
  fi
}

require_text_regex_count() {
  text="$1"
  pattern="$2"
  expected="$3"
  description="$4"
  # `|| true` covers grep's no-match status; pipefail would fail the pipeline.
  count="$(printf '%s\n' "$text" | grep -Ec -e "$pattern" || true)"
  if ! is_count "$count" || [ "$count" -ne "$expected" ]; then
    fail "text must contain $expected $description lines (found ${count:-no count})"
  fi
}

require_tree_regex_count() {
  root="$1"
  pattern="$2"
  expected="$3"
  description="$4"
  if [ ! -e "$root" ]; then
    fail "missing tree: $root (needed by: $expected $description declarations)"
    return 0
  fi
  # grep exits 1 for no match and 2 for an error; only no match is a count.
  count="$({ grep -RhE --include='*.rs' -e "$pattern" "$root" || [ "$?" -eq 1 ]; } | wc -l | tr -d '[:space:]')"
  if ! is_count "$count" || [ "$count" -ne "$expected" ]; then
    fail "$root must contain $expected $description declarations (found ${count:-no count})"
  fi
}

# Prints a Rust source with `//` and (nested) `/* */` comments removed and string
# and character literals kept, one output line per input line. Rules that look
# for code use it so a commented-out call cannot stand in for a deleted one.
# Raw strings are read as ordinary strings.
rust_code_without_comments() {
  awk '
    {
      line = $0
      output = ""
      length_ = length(line)
      for (position = 1; position <= length_; position++) {
        character = substr(line, position, 1)
        following = substr(line, position + 1, 1)
        if (block_depth > 0) {
          if (character == "*" && following == "/") { block_depth--; position++ }
          else if (character == "/" && following == "*") { block_depth++; position++ }
          continue
        }
        if (quoted) {
          output = output character
          if (character == "\\") { output = output following; position++ }
          else if (character == "\"") quoted = 0
          continue
        }
        if (character == "\"") { quoted = 1; output = output character; continue }
        if (character == "'"'"'") {
          # A character literal, not a lifetime: copy it whole.
          if (following == "\\") {
            closing = index(substr(line, position + 2), "'"'"'")
            if (closing > 0) {
              output = output substr(line, position, closing + 2)
              position += closing + 1
              continue
            }
          } else if (substr(line, position + 2, 1) == "'"'"'") {
            output = output substr(line, position, 3)
            position += 2
            continue
          }
        }
        if (character == "/" && following == "/") break
        if (character == "/" && following == "*") { block_depth = 1; position++; continue }
        output = output character
      }
      print output
    }
  ' "$1"
}

# Shared item reader must precede every guard that consumes its projection.
braced_rust_item_source() {
  source_file="$1"
  item_start_pattern="$2"
  awk -v item_start_pattern="$item_start_pattern" '
    !capturing {
      trimmed_line = $0
      sub(/^[[:space:]]*/, "", trimmed_line)
      if (match(trimmed_line, item_start_pattern) != 1) {
        next
      }
      capturing = 1
    }
    {
      print
      opening_line = $0
      closing_line = $0
      openings = gsub(/\{/, "", opening_line)
      closings = gsub(/\}/, "", closing_line)
      depth += openings - closings
      if (openings > 0) {
        body_started = 1
      }
      if (body_started && depth == 0) {
        exit
      }
    }
  ' "$source_file"
}

# Non-test CODE lines: everything before the crate's `#[cfg(test)]` block, minus
# blank lines and minus whole-line comments (`//`, `///`, `//!` and lines inside
# a whole-line `/* ... */` block).
#
# Blanks and comments are excluded because of what this budget is FOR: it exists
# so implementation cannot creep back into a crate root that is supposed to hold
# nothing but `mod`, `use` and `pub use`. Counting documentation against that
# budget makes the guard punish the one thing a re-export surface most needs.
# Measured at batch 6: `scriptw-ir/src/lib.rs` was 169 raw lines and RED against
# a budget of 140, while its code was 140 lines exactly — every line over the
# limit was a doc comment pointing a re-exported contract type at its
# `docs/rust-rewrite/contracts/` file, added by the theory rounds. Raising the
# number instead would have ratcheted the budget for a file that had not grown.
#
# THIS IS A LOOSENING OF EVERY `check_orchestration_surface` BUDGET, not only of
# the one that motivated it. Each budget below is now read against a code-only
# count; a number chosen against the old raw count is therefore no longer the
# limit it was written to be, and each is annotated at its call site with what it
# measures today.
#
# The block-comment rule is a state machine rather than the `^[[:space:]]*\*`
# heuristic it replaces. That heuristic dropped any line whose first non-space
# character is `*` — a `*slot = value;` deref statement, a continued expression —
# so the count could silently UNDER-report real code for any file this script
# guards, in the one direction that turns a red budget green without anyone
# editing the budget. Only whole-line block comments are skipped; a `/* ... */`
# that opens after code on the same line still counts that line, which is the
# conservative direction.
non_test_lines() {
  awk '
    /^#\[cfg\(test\)\]/ { exit }
    in_block { if ($0 ~ /\*\//) { in_block = 0 } ; next }
    /^[[:space:]]*$/ { next }
    /^[[:space:]]*\/\// { next }
    /^[[:space:]]*\/\*/ { if ($0 !~ /\*\//) { in_block = 1 } ; next }
    { count += 1 }
    END { print count + 0 }
  ' "$1"
}

check_orchestration_surface() {
  file="$1"
  max_lines="$2"
  rule_input_file "$file" "a budget of $max_lines non-test code lines" || return 0
  lines="$(non_test_lines "$file")"
  if ! is_count "$lines" || [ "$lines" -gt "$max_lines" ]; then
    fail "$file has $lines non-test code lines; expected at most $max_lines"
  fi
}

check_no_inline_legacy_includes() {
  file="$1"
  if grep -Eq 'include!|#\[path' "$file"; then
    fail "$file must not reassemble legacy implementation through include!/#[path]"
  fi
}

# Two registered Cargo integration children have exact crate-root-relative paths.
# The general production/legacy include ban remains unchanged for every other owner.
check_registered_module_source_test_child() {
  registered_root="$1"
  registered_child='crates/scriptw-engine/tests/aot_module_sources/prototype_chain.rs'
  capability_child='crates/scriptw-engine/tests/aot_module_sources/evaluation_capability.rs'
  if [ "$registered_root" != 'crates/scriptw-engine/tests/aot_module_sources.rs' ]; then
    fail "$registered_root is not the registered ModuleSource integration root"
    return
  fi
  require_file "$registered_child"
  require_module_decl "$registered_root" prototype_chain
  require_exact_line_count "$registered_child" '#[test]' 1 'integration child test gate'
  check_no_inline_legacy_includes "$registered_child"
  require_file "$capability_child"
  require_module_decl "$registered_root" evaluation_capability
  require_exact_line_count "$capability_child" '#[test]' 2 'evaluation capability child test gates'
  check_no_inline_legacy_includes "$capability_child"
  if ! awk '
    $0 == "#[path = \"aot_module_sources/prototype_chain.rs\"]" {
      count++
      if (count > 1 || getline <= 0 || $0 != "mod prototype_chain;") exit 1
      next
    }
    $0 == "#[path = \"aot_module_sources/evaluation_capability.rs\"]" {
      capability_count++
      if (capability_count > 1 || getline <= 0 || $0 != "mod evaluation_capability;") exit 1
      next
    }
    /include!|#\[path/ { exit 1 }
    END { if (count != 1 || capability_count != 1) exit 1 }
  ' "$registered_root"; then
    fail "$registered_root may only attach its two exact registered integration children"
  fi
}

check_raw_line_budget() {
  file="$1"
  max_lines="$2"
  rule_input_file "$file" "a budget of $max_lines raw lines" || return 0
  lines="$(wc -l < "$file" | tr -d '[:space:]')"
  if ! is_count "$lines" || [ "$lines" -gt "$max_lines" ]; then
    fail "$file has $lines raw lines; expected at most $max_lines"
  fi
}

if command -v sha256sum >/dev/null 2>&1; then
  sha256_stream() {
    sha256sum | cut -d ' ' -f 1
  }
elif command -v shasum >/dev/null 2>&1; then
  sha256_stream() {
    shasum -a 256 | cut -d ' ' -f 1
  }
else
  printf 'check-module-boundaries: sha256sum or shasum is required\n' >&2
  exit 1
fi

ir_lib="crates/scriptw-ir/src/lib.rs"
ir_builtins="crates/scriptw-ir/src/builtins.rs"
ir_lowering="crates/scriptw-ir/src/lowering.rs"
wasm_lib="crates/scriptw-aot-wasm/src/lib.rs"
wasm_builtins_mod="crates/scriptw-aot-wasm/src/builtins/mod.rs"
wasm_standard_builtins="crates/scriptw-aot-wasm/src/builtins/standard.rs"
wasm_intrinsics_mod="crates/scriptw-aot-wasm/src/intrinsics/mod.rs"

require_file "$ir_lib"
require_file "$wasm_lib"
require_file "$wasm_builtins_mod"

# Sourced in dependency order: later guards consume earlier helper projections.
module_guard_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/module-boundaries"
source "$module_guard_dir/ir.sh"
source "$module_guard_dir/wasm.sh"
source "$module_guard_dir/regexp-objects.sh"
source "$module_guard_dir/intl.sh"
source "$module_guard_dir/native-source.sh"

# Every rule ran. From here on only a readable, numeric zero count is a pass.
trap - ERR
run_complete=1
if ! failures="$(wc -l < "$failure_log")"; then
  printf 'check-module-boundaries: cannot read the finding log; failing closed\n' >&2
  exit 2
fi
failures="${failures//[[:space:]]/}"
if ! is_count "$failures"; then
  printf 'check-module-boundaries: finding count is not a number (%s); failing closed\n' "$failures" >&2
  exit 2
fi
if [ "$failures" != 0 ]; then
  printf 'check-module-boundaries: %s findings\n' "$failures" >&2
  exit 1
fi

printf 'check-module-boundaries: ok\n'
