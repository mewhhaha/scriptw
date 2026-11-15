#!/usr/bin/env python3
"""Count and attribute instructions of retained RSC requests with Callgrind.

For machines without hardware counters. The retained host runs under
Valgrind with instrumentation off until its first measured request. The
driver then zeroes Callgrind's counters at each of the host's "ready"
barriers and dumps them at each "done" barrier, while the host is blocked on
the barrier socket, so one dump holds one request plus the barrier's own
socket write and read. Counts are Callgrind instruction references on this
machine's Valgrind; they are deterministic and comparable between two hosts
measured the same way, but they are not hardware user-instruction counts.

  count     <host> <fixtures> <out>   medians and means per request
  symbols   <out>                     own instructions per symbol
  callers   <out> <callee-substring>  calls and inclusive cost by caller
  function  <out> <image> <symbol>    executed native instructions of one body

`count --profile` keeps the dumps the three readers need. The host writes
/tmp/perf-<pid>.map for its native code; `count` copies it beside the dumps.
`function` joins the counts to `objdump` of the native image the host writes
under SCRIPTW_RSC_ARTIFACT_DIR (program.cwasm) for the same source and policy.
Run it through scripts/limited_verification.py.
"""
import argparse
import bisect
import collections
import json
import os
import re
import secrets
import shutil
import socket
import statistics
import subprocess
import sys
import time
from pathlib import Path

NAME = re.compile(r"\((\d+)\)(?: (.*))?")


def dumps_in(directory):
    files = [path for path in Path(directory).glob("cg.out.*") if path.suffix[1:].isdigit()]
    return sorted(files, key=lambda path: int(path.suffix[1:]))


def count(args):
    out = args.output.resolve()
    fixtures = args.fixtures.resolve()
    out.mkdir(parents=True)
    parent, child = socket.socketpair()
    nonce = secrets.token_hex(16)
    environment = dict(
        os.environ,
        SCRIPTW_RSC_PROFILE_FD=str(child.fileno()),
        SCRIPTW_RSC_PROFILE_NONCE=nonce,
        SCRIPTW_RSC_PROFILE_MODE=args.mode,
    )
    command = [
        "valgrind", "--tool=callgrind", "--instr-atstart=no", "--vgdb=yes",
        f"--callgrind-out-file={out}/cg.out",
    ]
    if args.profile:
        command.append("--dump-instr=yes")
    command += [
        str(args.host.resolve()), str(fixtures / "rsc.js"), str(fixtures / "support.json"),
        str(out / "report.json"), str(args.samples), str(args.warmups), args.policy,
        "rsc-" + args.mode,
    ]
    started = time.time()
    with open(out / "stdout.json", "wb") as stdout, open(out / "stderr.log", "wb") as stderr:
        process = subprocess.Popen(
            command, env=environment, pass_fds=[child.fileno()], cwd=out,
            stdout=stdout, stderr=stderr,
        )
    child.close()

    def control(option):
        # The host is blocked on the barrier socket; vgdb interrupts it to run
        # the command before the acknowledgment releases it.
        subprocess.run(
            ["callgrind_control", option, str(process.pid)], cwd=out, check=True,
            timeout=120, stdout=subprocess.DEVNULL,
        )

    setup_seconds = None
    reader = parent.makefile("rb")
    while True:
        line = reader.readline()
        if not line:
            break
        text = line.decode()
        fields = text.split()
        if len(fields) != 5 or fields[0] != "SWRSC1" or fields[1] != nonce:
            sys.exit(f"unexpected barrier message: {text!r}")
        if setup_seconds is None:
            setup_seconds = time.time() - started
            control("--instr=on")
        if fields[2] == "ready":
            control("--zero")
        elif fields[2] == "done":
            control("--dump")
        parent.sendall(text.replace("SWRSC1", "SWACK1", 1).encode())
    if process.wait() != 0:
        sys.exit(f"host failed; see {out}/stderr.log")
    report = json.load(open(out / "report.json"))
    if report["cases"][0]["exact_node_output_checked"] is not True:
        sys.exit("the host did not check its Flight output")
    dumps = dumps_in(out)
    counts = []
    for path in dumps:
        with open(path, errors="replace") as handle:
            totals = [row for row in handle if row.startswith("totals:")]
        counts.append(int(totals[0].split()[1]))
    if len(counts) != args.samples:
        sys.exit(f"expected {args.samples} windows, found {len(counts)}")
    # The first windows still translate newly instrumented code paths.
    kept = counts[args.discard:]
    for path in dumps[:args.discard] if args.profile else dumps:
        path.unlink()
    perf_map = Path(f"/tmp/perf-{process.pid}.map")
    if args.profile:
        shutil.copy(perf_map, out / "perf.map")
    perf_map.unlink(missing_ok=True)
    result = dict(
        mode=args.mode, policy=args.policy, requests=len(kept), discarded=args.discard,
        median=statistics.median(kept), mean=round(statistics.fmean(kept)),
        minimum=min(kept), maximum=max(kept), setup_seconds=round(setup_seconds, 1),
        host_sha256=report["executable_sha256"],
        program_wasm_bytes=report["program_wasm_bytes"],
    )
    (out / "counts.json").write_text(json.dumps(dict(result, counts=counts)) + "\n")
    print(json.dumps(result))


class Symbols:
    def __init__(self, directory):
        rows = []
        for line in open(Path(directory) / "perf.map", errors="replace"):
            parts = line.rstrip("\n").split(" ", 2)
            if len(parts) == 3:
                rows.append((int(parts[0], 16), int(parts[1], 16), parts[2]))
        rows.sort()
        self.rows = rows
        self.starts = [row[0] for row in rows]

    def name(self, known, address):
        if known is not None and not known.startswith("0x"):
            return known
        index = bisect.bisect_right(self.starts, address) - 1
        if index >= 0 and address < self.rows[index][0] + self.rows[index][1]:
            return self.rows[index][2]
        return "?unmapped"


def records(dump):
    """Yield (function, address, own cost) and (function, address, callee,
    callee address, calls, inclusive cost) rows of one dump."""
    names = {}
    function = callee = None
    address = callee_address = calls = 0
    pending_call = False
    body = False
    for line in open(dump, errors="replace"):
        if not body:
            body = line.startswith("events:")
            continue
        line = line.rstrip("\n")
        if not line:
            continue
        if line.startswith(("fn=", "cfn=")):
            prefix, text = line.split("=", 1)
            match = NAME.match(text)
            if match.group(2) is not None:
                names[match.group(1)] = match.group(2)
            name = names.get(match.group(1), match.group(1))
            if prefix == "fn":
                function = name
            else:
                callee = name
            continue
        if line.startswith("calls="):
            fields = line[6:].split()
            calls = int(fields[0])
            callee_address = int(fields[1], 16) if fields[1].startswith("0x") else 0
            pending_call = True
            continue
        fields = line.split()
        position = fields[0]
        if position.startswith("0x"):
            address = int(position, 16)
        elif position[0] == "+":
            address += int(position[1:])
        elif position[0] == "-":
            address -= int(position[1:])
        elif position != "*":
            continue
        if pending_call:
            pending_call = False
            yield (function, address, callee, callee_address, calls, int(fields[-1]))
        else:
            yield (function, address, int(fields[-1]))


def symbols(args):
    names = Symbols(args.output)
    dumps = dumps_in(args.output)
    cost = collections.Counter()
    for dump in dumps:
        for row in records(dump):
            if len(row) == 3:
                cost[names.name(row[0], row[1])] += row[2]
    total = sum(cost.values())
    print(f"{total / len(dumps):.0f} instructions per request over {len(dumps)} requests")
    for name, value in cost.most_common(args.top):
        print(f"{value / len(dumps):10.0f} {100 * value / total:5.1f}%  {name[:110]}")


def callers(args):
    names = Symbols(args.output)
    dumps = dumps_in(args.output)
    calls = collections.Counter()
    cost = collections.Counter()
    for dump in dumps:
        for row in records(dump):
            if len(row) == 6:
                callee = names.name(row[2], row[3])
                if args.callee in callee:
                    key = (names.name(row[0], row[1]), callee)
                    calls[key] += row[4]
                    cost[key] += row[5]
    print("calls and inclusive instructions per request")
    for key, value in cost.most_common(args.top):
        print(f"{calls[key] / len(dumps):9.1f} {value / len(dumps):10.0f}  {key[0][:60]} -> {key[1][:50]}")
    print(f"{sum(calls.values()) / len(dumps):9.1f} {sum(cost.values()) / len(dumps):10.0f}  all")


def function(args):
    names = Symbols(args.output)
    found = [row for row in names.rows if args.symbol in row[2] and "adapter" not in row[2]]
    if len(found) != 1:
        sys.exit(f"{len(found)} native bodies match {args.symbol!r}")
    start, size, name = found[0]
    dumps = dumps_in(args.output)
    cost = collections.Counter()
    for dump in dumps:
        for row in records(dump):
            if len(row) == 3 and start <= row[1] < start + size:
                cost[row[1] - start] += row[2]
    table = subprocess.run(
        ["objdump", "-t", str(args.image)], capture_output=True, text=True, check=True
    ).stdout
    entries = [line.split() for line in table.splitlines() if line.endswith("::" + name)]
    if len(entries) != 1:
        sys.exit(f"{len(entries)} image symbols match {name!r}")
    base, length = int(entries[0][0], 16), int(entries[0][4], 16)
    listing = subprocess.run(
        ["objdump", "-d", "--no-show-raw-insn", "-M", "intel", f"--start-address={base}",
         f"--stop-address={base + length}", str(args.image)],
        capture_output=True, text=True, check=True,
    ).stdout
    by_count = collections.Counter()
    for value in cost.values():
        by_count[round(value / len(dumps))] += value
    total = sum(cost.values())
    print(f"{name}: {total / len(dumps):.0f} instructions per request, "
          f"{len(cost)} distinct instructions executed")
    print("executions per request x instructions = instructions per request")
    for executions, value in sorted(by_count.items(), key=lambda item: -item[1])[:args.top]:
        print(f"{executions:8d} x {value / len(dumps) / max(executions, 1):7.0f} = "
              f"{value / len(dumps):9.0f}")
    if args.listing:
        with open(args.listing, "w") as handle:
            for line in listing.splitlines():
                match = re.match(r"\s*([0-9a-f]+):\s+(.*)", line)
                if match and int(match.group(1), 16) - base in cost:
                    offset = int(match.group(1), 16) - base
                    handle.write(f"{cost[offset] / len(dumps):8.1f} {offset:6x}: {match.group(2)}\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    one = commands.add_parser("count")
    one.add_argument("host", type=Path)
    one.add_argument("fixtures", type=Path)
    one.add_argument("output", type=Path)
    one.add_argument("--mode", choices=("sync", "async"), default="sync")
    one.add_argument("--policy", default="speed")
    one.add_argument("--samples", type=int, default=100)
    one.add_argument("--warmups", type=int, default=100)
    one.add_argument("--discard", type=int, default=3)
    one.add_argument("--profile", action="store_true")
    one.set_defaults(run=count)
    one = commands.add_parser("symbols")
    one.add_argument("output", type=Path)
    one.add_argument("--top", type=int, default=60)
    one.set_defaults(run=symbols)
    one = commands.add_parser("callers")
    one.add_argument("output", type=Path)
    one.add_argument("callee")
    one.add_argument("--top", type=int, default=20)
    one.set_defaults(run=callers)
    one = commands.add_parser("function")
    one.add_argument("output", type=Path)
    one.add_argument("image", type=Path)
    one.add_argument("symbol")
    one.add_argument("--listing", type=Path)
    one.add_argument("--top", type=int, default=14)
    one.set_defaults(run=function)
    args = parser.parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
