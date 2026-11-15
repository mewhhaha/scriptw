#!/usr/bin/env python3
"""Wall-clock request times: retained Scriptw host (Speed) against Node --jitless.

usage: rsc_wall_clock_compare.py <host> <fixtures> <node> <processes> [cpu]

Interleaved fresh processes pinned to one CPU, 100 warmups and 101 requests
each, the Node side through react-driver.cjs retained-module. A diagnostic for
machines where the qualified campaign (benchmark-rsc.py) cannot run; it needs
an otherwise idle machine and is not a published measurement. Run it from the
repository root through scripts/limited_verification.py.
"""
import json, statistics, subprocess, sys, tempfile, os
from pathlib import Path
host, fixtures, node, processes = sys.argv[1], Path(sys.argv[2]).resolve(), sys.argv[3], int(sys.argv[4])
cpu = sys.argv[5] if len(sys.argv) > 5 else "2"
driver = "crates/scriptw-cli/tests/fixtures/edge_rendering/react-driver.cjs"
W, S = 100, 101
def scriptw(case):
    with tempfile.TemporaryDirectory() as d:
        out = os.path.join(d, "r.json")
        subprocess.run(["taskset", "-c", cpu, host, str(fixtures / "rsc.js"), str(fixtures / "support.json"), out,
                        str(S), str(W), "speed", case], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        rows = json.load(open(out))["cases"][0]["observations"]
    return [r["elapsed_ns"] for r in rows if not r["warmup"]]
def v8(case):
    raw = subprocess.run(["taskset", "-c", cpu, node, "--jitless", "--experimental-vm-modules", "--no-warnings", driver,
                          "retained-module", str(fixtures), str(S), str(W), case], check=True, capture_output=True, text=True).stdout
    rows = json.loads(raw)["observations"]
    return [r["elapsed_ns"] for r in rows if not r["warmup"]]
result = {}
for case in ("rsc-sync", "rsc-async"):
    data = {"scriptw": [], "v8": []}
    for index in range(processes):
        order = (("scriptw", scriptw), ("v8", v8)) if index % 2 == 0 else (("v8", v8), ("scriptw", scriptw))
        for name, run in order:
            data[name].append(run(case))
    row = {}
    for name, runs in data.items():
        row[name] = dict(median_us=round(statistics.median(statistics.median(r) for r in runs) / 1000, 1),
                         mean_us=round(statistics.fmean(statistics.fmean(r) for r in runs) / 1000, 1),
                         best_process_median_us=round(min(statistics.median(r) for r in runs) / 1000, 1))
    row["ratio_median"] = round(row["scriptw"]["median_us"] / row["v8"]["median_us"], 3)
    row["ratio_mean"] = round(row["scriptw"]["mean_us"] / row["v8"]["mean_us"], 3)
    result[case] = row
    print(case, json.dumps(row), flush=True)
