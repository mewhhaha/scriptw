#!/usr/bin/env bash
# The Rust PGO training set of the local dist `scriptw` binary. scripts/build-dist.sh
# pgo runs it with LLVM_PROFILE_FILE set; it can also run alone.
#
# usage: scripts/dist-pgo-train.sh <instrumented-scriptw>
#
# The set represents compilation, as the benchmarks and Test262 drive it:
#   - nine of the 17 compilation-panel kernels (v8_execution_hotpaths), each
#     through `build wasm`, `precompile --policy speed` and `run`;
#   - when SCRIPTW_DIST_PGO_APPLICATION_FIXTURES names a generated application fixture
#     directory (scripts/benchmark-react.py prepare), the RSC program
#     (rsc.js plus rsc_baseline's module wrapper) and the six edge programs
#     through `precompile --policy speed`;
#   - the Test262 subtree built-ins/JSON/stringify (wasm-aot, one thread).
# Held out, to show that the profile generalizes: the other eight kernels,
# the React client bundle (client.js) and every other Test262 directory,
# including built-ins/Array/prototype/find, which the measurements use. The
# held-out inputs exercise mostly the same code paths: lowering and codegen of
# features the set never compiles (generators, classes, RegExp, Intl, Temporal,
# BigInt, ...) are profiled cold and may be slower than in release.
#
# SCRIPTW_DIST_PGO_SUITE_ROOT overrides the pinned suite (default
# test262/vendor/test262).
#
# Every run gets a fresh SCRIPTW_CACHE_DIR and SCRIPTW_JOBS=1. A failing run,
# including a failing Test262 case, stops the script on purpose: a run killed
# by a signal writes no profile, a binary trained on part of the set looks fine
# but is slower, and a correctness regression must be fixed before training.
# The private work directory target/dist-pgo-train.* is removed on exit; only
# a SIGKILL (for example the wrapper's OOM kill) leaves it behind.
set -euo pipefail

[[ $# -eq 1 ]] || { sed -n '2,/^set -euo/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//'; exit 2; }
bin="$(cd -- "$(dirname -- "$1")" && pwd)/$(basename -- "$1")"
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
kernels="$repo/crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths"
mkdir -p "$repo/target"
work="$(mktemp -d "$repo/target/dist-pgo-train.XXXXXX")"
trap 'rm -rf "$work"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
export SCRIPTW_JOBS=1

fresh() { rm -rf "$work/cache"; mkdir "$work/cache"; export SCRIPTW_CACHE_DIR="$work/cache"; }

for k in sort-reverse-128 sort-reverse-512 sort-sorted-512 to-sorted-512 join-1024 join-4096 \
  numeric-loop-10000 integer objects; do
  fresh
  SCRIPTW_WASM_DUMP="$work/p.wasm" "$bin" build wasm "$kernels/$k.js" > /dev/null
  fresh
  "$bin" precompile --policy speed "$kernels/$k.js" -o "$work/k.img" > /dev/null
  rm -f "$work/k.img"
  fresh
  "$bin" run "$kernels/$k.js" > /dev/null
  echo "trained kernel $k"
done
rm -f "$work"/p.wasm*

if [[ -n ${SCRIPTW_DIST_PGO_APPLICATION_FIXTURES:-} ]]; then
  fixtures="$(cd -- "$SCRIPTW_DIST_PGO_APPLICATION_FIXTURES" && pwd)"
  # rsc_baseline's WRAPPER: the same module the RSC benchmarks compile.
  { cat "$fixtures/rsc.js"
    printf '\nexport const run = globalThis.applicationRun;\nexport function encode(value) { return JSON.stringify(value); }\n'
  } > "$work/rsc.mjs"
  fresh
  "$bin" precompile --policy speed "$work/rsc.mjs" -o "$work/a.img" > /dev/null
  echo "trained rsc"
  for e in edge-html edge-json-api edge-middleware edge-nested-result edge-query edge-router; do
    fresh
    "$bin" precompile --policy speed "$fixtures/$e.js" -o "$work/a.img" > /dev/null
    rm -f "$work/a.img"
    echo "trained $e"
  done
fi

fresh
suite="$(cd -- "${SCRIPTW_DIST_PGO_SUITE_ROOT:-$repo/test262/vendor/test262}" && pwd)"
# A private snapshot directory: other Test262 runs share test262/snapshots.
(cd "$repo" && "$bin" --jobs 1 test262 run built-ins/JSON/stringify --execution-backend wasm-aot \
  --suite-root "$suite" --snapshot-dir "$work/snapshots" --threads 1 --timeout-ms 60000 | tail -3)
echo "trained test262 built-ins/JSON/stringify"
