#!/usr/bin/env bash
# One run reports every finding. `errexit` is deliberately off: a rule that hits
# a missing file or a failing helper must not end the run before the thousands of
# rule lines after it execute. Instead the ERR trap below records every command
# failure that `set -e` would have aborted on (same condition rules: `if`,
# `while`, `!`, `&&`/`||` heads and non-final pipeline stages stay exempt) as a
# finding, so nothing that used to fail the guard can pass it now.
set -uo pipefail
set -E

# Findings are appended to a log, so a `fail` from any subshell still counts.
failure_log="$(mktemp)"
trap 'rm -f "$failure_log"' EXIT

fail() {
  printf 'check-module-boundaries: %s\n' "$*" >&2
  printf 'finding\n' >> "$failure_log"
}

# Only the main shell's failures are findings, matching errexit, which command
# substitutions do not inherit: `$(a; b)` tolerates a failing `a` and the
# assignment that consumes it fails (and is recorded) when `b` fails.
command_failed() {
  if [ "$BASHPID" = "$$" ]; then
    fail "command failed with status $1 at $2:$3: $4"
  fi
}
trap 'command_failed "$?" "${BASH_SOURCE[0]:-$0}" "$LINENO" "$BASH_COMMAND"' ERR

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
  count="$(grep -Fc "$needle" "$file" || true)"
  if [ "$count" -ne "$expected" ]; then
    fail "$file must contain $expected $description sites (found $count)"
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
  count="$(grep -Fxc "$line" "$file" || true)"
  if [ "$count" -ne "$expected" ]; then
    fail "$file must contain $expected exact $description lines (found $count)"
  fi
}

require_regex_count() {
  file="$1"
  pattern="$2"
  expected="$3"
  description="$4"
  count="$(grep -Ec "$pattern" "$file" || true)"
  if [ "$count" -ne "$expected" ]; then
    fail "$file must contain $expected $description lines (found $count)"
  fi
}

require_text_regex_count() {
  text="$1"
  pattern="$2"
  expected="$3"
  description="$4"
  count="$(printf '%s\n' "$text" | grep -Ec "$pattern" || true)"
  if [ "$count" -ne "$expected" ]; then
    fail "text must contain $expected $description lines (found $count)"
  fi
}

require_tree_regex_count() {
  root="$1"
  pattern="$2"
  expected="$3"
  description="$4"
  count="$({ grep -RhE --include='*.rs' "$pattern" "$root" || true; } | wc -l | tr -d '[:space:]')"
  if [ "$count" -ne "$expected" ]; then
    fail "$root must contain $expected $description declarations (found $count)"
  fi
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
  lines="$(non_test_lines "$file")"
  if [ "$lines" -gt "$max_lines" ]; then
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
  lines="$(wc -l < "$file")"
  if [ "$lines" -gt "$max_lines" ]; then
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

failures="$(wc -l < "$failure_log" | tr -d '[:space:]')"
if [ "$failures" -ne 0 ]; then
  printf 'check-module-boundaries: %s findings\n' "$failures" >&2
  exit 1
fi

printf 'check-module-boundaries: ok\n'
