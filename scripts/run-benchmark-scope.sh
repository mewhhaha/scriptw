#!/usr/bin/env bash
# GitHub-hosted Ubuntu has a system manager but need not have a user manager.
set -euo pipefail
exec sudo systemd-run --quiet --wait --pipe --collect \
  --unit="scriptw-benchmark-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}-${RANDOM}" \
  --property=MemoryMax=4G --property=MemorySwapMax=0 --property=MemoryOOMGroup=yes \
  --property="User=$(id -un)" --property="WorkingDirectory=$PWD" \
  /usr/bin/env "HOME=$HOME" "PATH=$PATH" \
  "LD_LIBRARY_PATH=${LD_LIBRARY_PATH:-}" \
  "CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}" "RUSTUP_HOME=${RUSTUP_HOME:-$HOME/.rustup}" \
  "SCRIPTC_RUNTIME_PACK=${SCRIPTC_RUNTIME_PACK:-}" \
  "BENCHMARK_RUN_URL=${BENCHMARK_RUN_URL:-}" \
  python3 -B scripts/limited_verification.py --in-scope -- "$@"
