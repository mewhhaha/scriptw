#!/usr/bin/env python3
"""Allow only the two pinned external benchmark setup lines in the retirement audit."""
import sys

ALLOWED = {
    '- uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4; benchmark oracle and external compiler only',
    'npm install --prefix "$RUNNER_TEMP/scriptc" --no-audit --no-fund --save-exact scriptc@0.4.5 @scriptc/runtime-wasm32-wasi@0.4.5',
}
WORKFLOWS = {
    '.github/workflows/compiler-benchmarks.yaml',
    '.github/workflows/precompiled-benchmarks.yaml',
}
for line in sys.stdin:
    parts = line.rstrip('\n').split(':', 2)
    if len(parts) != 3 or parts[0] not in WORKFLOWS or parts[2].strip() not in ALLOWED:
        sys.stdout.write(line)
