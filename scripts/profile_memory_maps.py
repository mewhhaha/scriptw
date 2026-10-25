#!/usr/bin/env python3
"""Snapshot a real Wasm child's mappings at acknowledged guest boundaries.

Uses the existing runtime_baseline row-observer FIFOs; no native profiler or
compiler modification is required. These instrumented observations attribute
process residency, not live JavaScript bytes or ordinary benchmark timings.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import select
import signal
import subprocess
import sys
import time

from limited_verification import require_kernel_budget
from process_memory_maps import parse_smaps, read_smaps

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths'
MARKERS = ['scriptw:rows:construction:start', 'scriptw:rows:traversal:start',
           'scriptw:rows:traversal:end']


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def prepare_case(source, original_stdout):
    match = re.fullmatch(r'\s*(?://[^\n]*\n)*\(function workload\(\)\s*\{(.*)\}\)\(\);\s*',
                         source, re.S)
    body = match[1] if match else source
    require(len(re.findall(r'\bprint\s*\(', body)) == 1, 'fixture needs one final print')
    final = re.search(r'\bprint\((.*)\);\s*\Z', body, re.S)
    require(final is not None, 'fixture print must be its last statement')
    transformed = ('(function memoryProbe() {\n' + f'print("{MARKERS[0]}");\n'
                   + body[:final.start()] + f'\nprint("{MARKERS[1]}");\n'
                   + body[final.start():] + f'\nprint("{MARKERS[2]}");\n' + '})();\n')
    expected = MARKERS[0] + '\n' + MARKERS[1] + '\n' + original_stdout + MARKERS[2] + '\n'
    return transformed, expected


def native_ranges(text):
    ranges = []
    for match in re.finditer(r'^(R|P) wasm_sha256=([0-9a-f]{64}) '
                             r'native_text_sha256=([0-9a-f]{64}) base=([0-9a-f]+) bytes=(\d+)$',
                             text, re.M):
        kind, wasm, native, base, size = match.groups()
        ranges.append(dict(module=kind, wasm_sha256=wasm, native_text_sha256=native,
                           start=int(base, 16), end=int(base, 16) + int(size)))
    require({row['module'] for row in ranges} == {'R', 'P'}, 'missing R/P native-text identity')
    return ranges


def capture(child, directory, index, label):
    raw = read_smaps(child.pid)
    path = directory / f'{index:02}-{label}.smaps'
    path.write_text(raw)
    observation = parse_smaps(raw)
    code = directory / 'code'
    identity = code / f'perf-{child.pid}.map.identity'
    mapping = code / f'perf-{child.pid}.map'
    identity_text = identity.read_text()
    require(identity_text.startswith(f'scriptw-perf-map-v1 pid={child.pid} '),
            'native identity belongs to another process')
    require(f'map_sha256={digest(mapping)}\n' in identity_text, 'native map hash differs')
    (directory / f'{index:02}-{label}.native-identity').write_text(identity_text)
    ranges = native_ranges(identity_text)
    observation['native_text_mappings'] = [
        dict(module=region['module'], text_start=region['start'], text_end=region['end'],
             mappings=[m for m in observation['mappings']
                       if m['start'] < region['end'] and m['end'] > region['start']])
        for region in ranges
    ]
    # Native map contents are immutable per digest; deduplicate repeated phases.
    archive = directory / ('native-map-' + digest(mapping) + '.map')
    if not archive.exists():
        archive.write_bytes(mapping.read_bytes())
    return dict(phase=label, sample=index, pid=child.pid, raw=path.name,
                raw_sha256=digest(path), native_map=archive.name,
                native_map_sha256=digest(archive), native_ranges=ranges, **observation)


def run_case(runner, fixtures, case, output, samples, warmups):
    directory = output / case['file']
    directory.mkdir()
    for name in ('artifacts', 'code'):
        (directory / name).mkdir()
    control, ack = directory / 'control.fifo', directory / 'ack.fifo'
    os.mkfifo(control)
    os.mkfifo(ack)
    read_fd = os.open(control, os.O_RDWR | os.O_NONBLOCK)
    ack_fd = os.open(ack, os.O_RDWR | os.O_NONBLOCK)
    command = [str(runner), str(fixtures), str(samples), str(warmups), case['file'],
               str(directory / 'artifacts'), '--row-phases']
    env = dict(os.environ, SCRIPTW_ROW_PERF_CONTROL=str(control), SCRIPTW_ROW_PERF_ACK=str(ack),
               SCRIPTW_ROW_PERF_REGION='construction', SCRIPTW_PERF_MAP_DIR=str(directory / 'code'),
               SCRIPTW_CACHE_DIR=str(directory / 'cache'), SCRIPTW_WASM_TRACE='1')
    observations, pending = [], b''
    started = time.monotonic()
    child = None
    try:
        with (directory / 'stdout.json').open('wb') as stdout, (directory / 'stderr.log').open('wb') as stderr:
            child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr,
                                     start_new_session=True)
            while child.poll() is None:
                require(time.monotonic() - started < 180, 'memory probe exceeded 180-second deadline')
                ready, _, _ = select.select([read_fd], [], [], .1)
                if not ready:
                    continue
                pending += os.read(read_fd, 256)
                require(len(pending) < 512, 'unexpected control message size')
                while b'\n' in pending:
                    line, pending = pending.split(b'\n', 1)
                    index, phase = divmod(len(observations), 2)
                    require(index < samples and line == (b'enable' if phase == 0 else b'disable'),
                            'missing, extra or reordered guest memory boundary')
                    observations.append(capture(child, directory, index,
                                                'guest-start' if phase == 0 else 'guest-body-finished'))
                    os.write(ack_fd, b'ack\n\0')
            require(child.returncode == 0 and len(observations) == samples * 2 and not pending,
                    'memory probe child failed or omitted a boundary')
        result = json.loads((directory / 'stdout.json').read_text())
        require(result['samples'] == samples and result['warmups'] == warmups
                and len(result['cases']) == 1, 'child sampling plan differs')
        actual = result['cases'][0]
        require(actual['file'] == case['file'] and actual['source_sha256'] == digest(fixtures / case['file']),
                'executed fixture identity differs')
        require(len(actual['observations']) == samples + warmups
                and all(row['stdout'] == case['stdout'] for row in actual['observations']),
                'compiled workload output or sample count differs')
        return dict(file=case['file'], command=command, exit_code=child.returncode,
                    compiler_source=result['source_commit'], source_fingerprint=result['source_fingerprint'],
                    runner_sha256=result['executable_sha256'], observations=observations,
                    stdout_sha256=digest(directory / 'stdout.json'), stderr_sha256=digest(directory / 'stderr.log'))
    finally:
        if child is not None and child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
        os.close(read_fd)
        os.close(ack_fd)
        control.unlink()
        ack.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=FIXTURES)
    parser.add_argument('--case', action='append', dest='cases')
    parser.add_argument('--samples', type=int, default=3, choices=range(3, 8))
    parser.add_argument('--warmups', type=int, default=1, choices=range(1, 11))
    args = parser.parse_args()
    require_kernel_budget(4 * 1024**3)
    require(len(os.sched_getaffinity(0)) == 1, 'memory probes require one CPU')
    plan = json.loads((args.fixtures / 'manifest.json').read_text())
    inventory = {case['file']: case for case in [plan['startup'], *plan['execution']]}
    selected = args.cases or ['startup.js', 'integer.js', 'retained-objects.js']
    require(len(set(selected)) == len(selected) and set(selected) <= set(inventory), 'invalid case selection')
    output, runner = args.output.resolve(), args.runner.resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixtures = output / 'fixtures'
    fixtures.mkdir()
    cases, sources = [], []
    for name in selected:
        original = args.fixtures / name
        source, expected = prepare_case(original.read_text(), inventory[name]['stdout'])
        (fixtures / name).write_text(source)
        cases.append(dict(file=name, stdout=expected))
        sources.append(dict(file=name, original_path=str(original), original_sha256=digest(original),
                            source_sha256=digest(fixtures / name)))
    (fixtures / 'manifest.json').write_text(json.dumps(dict(execution=cases)))
    report = dict(schema='scriptw-process-memory-maps-v1', complete=False,
                  observed_at=datetime.now(timezone.utc).isoformat(), runner=str(runner),
                  runner_sha256=digest(runner), samples=args.samples, warmups=args.warmups,
                  cpu_affinity=sorted(os.sched_getaffinity(0)), sources=sources, cases=[],
                  boundary='Synchronous guest-start and guest-body-finished print hooks in real Wasm, '
                           'after bootstrap; raw /proc child smaps while hook awaits acknowledgement. '
                           'Instrumented process residency, not live JS bytes or accepted execution timing.')
    def save():
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    save()
    for case in cases:
        result = run_case(runner, fixtures, case, output, args.samples, args.warmups)
        require(result['runner_sha256'] == report['runner_sha256'], 'executed runner identity differs')
        report['cases'].append(result)
        save()
        print(case['file'] + ': checked phase mappings captured', flush=True)
    report['complete'] = True
    save()


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError) as error:
        print(f'memory map diagnostic: {error}', file=sys.stderr)
        raise SystemExit(1)
