#!/usr/bin/env bash
# Run locally the cheap guard steps that the CI `validate` job
# (.github/workflows/ci.yaml) runs before it builds anything, in the same
# order. Every step runs; the summary lists the failures. Keep this list in
# step with ci.yaml.
#
# Usage: scripts/ci-fast.sh [BASE_REF]
#   BASE_REF (default origin/main) is the comparison base for the README status
#   check that CI runs on pull requests; that step is skipped when the ref does
#   not resolve.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
base_ref="${1:-origin/main}"
failed=()

step() {
  local name="$1"
  shift
  printf '\n==> %s\n' "$name"
  if ! "$@"; then
    failed+=("$name")
  fi
}

intl_source_identities() {
  python3 -B scripts/intl_source_identities.py \
    && python3 -m unittest discover -s scripts/tests -p test_generate_intl_keyword_aliases.py -v \
    && python3 scripts/generate-intl-keyword-aliases.py --check \
    && python3 scripts/generate-intl-collator-data.py --check
}

step 'Check conformance ownership registry' ./scripts/conformance_owners.py
step 'Test README status artifact guard' ./scripts/tests/check-readme-status-artifacts.sh
step 'Test Test262 shortcut scanner' ./scripts/tests/audit-test262-shortcut-scanner.sh
step 'Test module-boundary guard harness' ./scripts/tests/check-module-boundaries-harness.sh
step 'Check module boundaries' ./scripts/check-module-boundaries.sh
step 'Audit Test262 shortcuts' ./scripts/audit-test262-shortcuts.sh --check
step 'Check Test262 host ABI' ./scripts/check-test262-host-abi.sh
step 'Check conformance source pins' python3 -B scripts/check_conformance_source_pins.py
step 'Test conformance source pin guard' python3 -B -m unittest scripts.tests.test_conformance_source_pins
step 'Check retired JavaScript boundaries' ./scripts/check-no-legacy-js.sh
step 'Check canonical Scriptw identities' ./scripts/check-scriptw-identity.sh
step 'Check generated README Test262 chart' python3 -B scripts/generate-test-graph.py --check
step 'Check pinned Intl source identities' intl_source_identities
if git rev-parse --verify --quiet "$base_ref^{commit}" >/dev/null; then
  step "Check generated README status edits against $base_ref" \
    ./scripts/check-readme-status-artifacts.sh "$base_ref"
else
  printf '\n==> Skipping README status edits: %s does not resolve\n' "$base_ref"
fi
step 'Check formatting' cargo fmt --all -- --check

if [ "${#failed[@]}" -ne 0 ]; then
  printf '\nci-fast: %d step(s) failed:\n' "${#failed[@]}" >&2
  printf '  %s\n' "${failed[@]}" >&2
  exit 1
fi
printf '\nci-fast: all steps passed\n'
