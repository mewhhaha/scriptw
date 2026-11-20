#!/usr/bin/env python3
"""Pooled against on-demand Store allocation in one precompiled host.

`prepare` produces native images for one `compiler_precompiled_baseline`
host: Scriptw sources through `prepare-scriptw`, and Scriptc 0.4.5 WASI
programs through the Scriptc build and `prepare-wasi` commands that
benchmark-applications.py runs (print adapter, isolated Scriptc/Zig caches).
`run` alternates pooled and on-demand fresh processes of that host, one JSON
line per process, counting minor faults inside one timed entry through the
host's phase barriers. `summarize` reduces those lines per actor and fixture.

This is an unscored diagnostic of allocation, not a cohort, publisher input or
cross-runtime ratio. Run it under scripts/limited_verification.py.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import statistics
import subprocess
import sys

from benchmark_metrics import digest
from compilation_comparison import ADAPTER, process_environment, scriptc_wasi_preparation

ROOT = Path(__file__).resolve().parents[1]
PREPARED_SCHEMA = 'scriptw-store-allocation-prepared-v1'
MODES = ('pooled', 'on-demand')
ACTIONS = dict(scriptw='execute-scriptw', scriptc='execute-wasi')
MANIFESTS = ('manifest.json', 'portable-manifest.json')
PHASES = ('after_instantiate', 'after_execution', 'after_store_drop')
DEADLINE = 300


def require(condition, message):
    if not condition:
        raise SystemExit('store allocation: ' + message)


def expected_stdout(fixture):
    """The exact stdout a fixture's own manifest records for it."""
    fixture = Path(fixture)
    for name in MANIFESTS:
        manifest = fixture.parent / name
        if manifest.is_file():
            cases = [case for case in json.loads(manifest.read_text()).get('execution', [])
                     if case.get('file') == fixture.name]
            require(len(cases) <= 1, 'duplicate manifest entry: ' + str(fixture))
            if cases:
                require(isinstance(cases[0].get('stdout'), str), 'manifest entry has no stdout: ' + str(fixture))
                return cases[0]['stdout']
    raise SystemExit('store allocation: no manifest records the stdout of ' + str(fixture))


def alternating_modes(block, process):
    """Order of the two modes for one process pair; blocks start opposite."""
    return MODES if (process + block) % 2 == 0 else MODES[::-1]


def run_checked(argv, **kwargs):
    result = subprocess.run(list(map(str, argv)), capture_output=True, text=True, timeout=DEADLINE, **kwargs)
    require(result.returncode == 0, f'{argv[0]} {argv[1]} failed ({result.returncode}): {result.stderr[-2000:]}')
    return result.stdout


def tool(path, version_argv=None):
    path = Path(path).resolve(strict=True)
    record = dict(path=str(path), sha256=digest(path), bytes=path.stat().st_size)
    if version_argv:
        record['version_stdout'] = run_checked([path, *version_argv]).strip()
    return record


def prepare(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    host = args.host.resolve(strict=True)
    tools = dict(host=dict(tool(host), identity=json.loads(run_checked([host, 'identity']))))
    scriptc_env = None
    if args.scriptc:
        require(args.runtime_pack and args.zig, '--scriptc needs --runtime-pack and --zig')
        tools['scriptc'] = tool(args.scriptc, ['--version'])
        tools['zig'] = tool(args.zig, ['version'])
        require(tools['scriptc']['version_stdout'].removeprefix('scriptc ').strip() == '0.4.5'
                and tools['zig']['version_stdout'] == '0.16.0', 'Scriptc 0.4.5 and Zig 0.16.0 required')
        runtime_pack = args.runtime_pack.resolve(strict=True)
        pack = json.loads((runtime_pack / 'runtime-pack.json').read_text())
        require(pack.get('version') == '0.4.5' and pack.get('target', {}).get('name') == 'wasm32-wasi',
                'Scriptc runtime pack pin differs')
        tools['runtime_pack'] = dict(path=str(runtime_pack), manifest_sha256=digest(runtime_pack / 'runtime-pack.json'))
        scriptc_env = {key: value for key, value in os.environ.items()
                       if not key.startswith(('SCRIPTW_', 'SCRIPTC_', 'ZIG_'))}
        scriptc_env['PATH'] = str(Path(tools['zig']['path']).parent) + os.pathsep + scriptc_env['PATH']
        zig_cache = Path(os.environ.get('ZIG_GLOBAL_CACHE_DIR', str(Path.home() / '.cache/zig'))).resolve()
    actors = [actor for actor in ('scriptw', 'scriptc') if actor in args.actor]
    require(actors, 'select --actor scriptw and/or scriptc')
    require('scriptc' not in actors or scriptc_env is not None, 'the Scriptc actor needs --scriptc')
    entries = []
    for fixture in args.fixtures:
        fixture = fixture.resolve(strict=True)
        stdout = expected_stdout(fixture)
        expected = output / 'expected' / (fixture.name + '.txt')
        expected.parent.mkdir(exist_ok=True)
        expected.write_text(stdout)
        for actor in actors:
            work = output / actor / fixture.stem
            work.mkdir(parents=True)
            if actor == 'scriptw':
                single = work / 'source'
                single.mkdir()
                shutil.copyfile(fixture, single / fixture.name)
                (single / 'manifest.json').write_text(json.dumps(dict(execution=[dict(file=fixture.name, stdout=stdout)])))
                receipt = json.loads(run_checked([host, 'prepare-scriptw', single, work / 'image']))
                require(len(receipt['cases']) == 1, 'producer case count differs')
                image_receipt = receipt['cases'][0]
                image = work / 'image' / image_receipt['image']
            else:
                for folder in ('tmp', 'cache', 'zig-local'):
                    (work / folder).mkdir()
                source = work / fixture.name
                source.write_bytes(ADAPTER.encode() + fixture.read_bytes())
                wasm, image = work / 'program.wasm', work / (fixture.name + '.swwa')
                emit_argv, native_argv = scriptc_wasi_preparation(tools['scriptc']['path'], host, source, wasm, image)
                run_checked(emit_argv, cwd=work, env=dict(scriptc_env, **process_environment(work, runtime_pack, zig_cache)))
                require(wasm.read_bytes()[:8] == b'\0asm\1\0\0\0', 'Scriptc did not emit portable Wasm')
                image_receipt = json.loads(run_checked(native_argv))
                require(image_receipt['wasm_sha256'] == digest(wasm), 'WASI producer input differs')
            require(digest(image) == image_receipt['image_sha256'], 'producer native image differs')
            (work / 'producer.json').write_text(json.dumps(image_receipt, indent=1, sort_keys=True) + '\n')
            entries.append(dict(actor=actor, fixture=fixture.name, source=str(fixture), source_sha256=digest(fixture),
                                expected=str(expected), image=str(image), image_sha256=image_receipt['image_sha256']))
            print(f'prepared {actor} {fixture.name}', flush=True)
    (output / 'prepared.json').write_text(json.dumps(dict(schema=PREPARED_SCHEMA, tools=tools, entries=entries),
                                                     indent=1, sort_keys=True) + '\n')
    print(output / 'prepared.json')


def proc_faults(pid):
    raw = Path(f'/proc/{pid}/stat').read_text()
    fields = raw[raw.rindex(')') + 2:].split()
    return dict(minflt=int(fields[7]), majflt=int(fields[9]))


def observe(host, entry, mode, args):
    """One fresh host process; faults are read while it waits at each barrier."""
    parent, child = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
    env = {key: value for key, value in os.environ.items() if not key.startswith('SCRIPTW_PRECOMPILED_')}
    env.update(SCRIPTW_PRECOMPILED_STORE_ALLOCATION=mode, SCRIPTW_PRECOMPILED_PHASE_FD=str(child.fileno()),
               SCRIPTW_PRECOMPILED_PHASE_NONCE=secrets.token_hex(16), SCRIPTW_PRECOMPILED_PHASE_INDEX=str(args.index))
    expected = Path(entry['expected']).read_text()
    process = subprocess.Popen([str(host), ACTIONS[entry['actor']], entry['image'], expected,
                                str(args.samples), str(args.warmups)], env=env, pass_fds=[child.fileno()],
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               preexec_fn=lambda: os.sched_setaffinity(0, {args.cpu}))
    child.close()
    faults = {}
    try:
        for line in parent.makefile('rb'):
            text = line.decode()
            require(text.startswith('SWPHASE1 '), 'unexpected barrier line: ' + text[:80])
            fields = text[len('SWPHASE1 '):]
            faults[fields.split()[2]] = proc_faults(process.pid)
            parent.sendall(('SWACK1 ' + fields).encode())
        stdout, stderr = process.communicate(timeout=DEADLINE)
    finally:
        parent.close()
        if process.poll() is None:
            process.kill()
            process.wait()
    require(process.returncode == 0, 'host failed: ' + stderr.decode()[:2000])
    require(all(phase in faults for phase in PHASES), 'missing phase barrier')
    report = json.loads(stdout)
    require(report['store_allocation']['strategy'] == mode, 'host ran another Store allocation')
    observation = report['observations'][args.index]
    require(observation['matched'], 'timed entry output differs')
    return dict(execution_ns=observation['phases_ns']['execution'],
                execution_cpu_ns=observation.get('execution_thread_cpu', {}).get('elapsed_ns'),
                minflt_execution=faults['after_execution']['minflt'] - faults['after_instantiate']['minflt'],
                majflt_execution=faults['after_execution']['majflt'] - faults['after_instantiate']['majflt'],
                store_allocation=report['store_allocation'])


def run(args):
    prepared = json.loads((args.prepared / 'prepared.json').read_text())
    require(prepared.get('schema') == PREPARED_SCHEMA, 'prepared schema differs')
    host = Path(prepared['tools']['host']['path'])
    require(args.cpu in os.sched_getaffinity(0), f'CPU {args.cpu} is outside this process affinity')
    require(0 <= args.index < args.samples + args.warmups, 'observation index outside the process')

    def check_inputs():
        require(digest(host) == prepared['tools']['host']['sha256'], 'host changed since preparation')
        for entry in prepared['entries']:
            require(digest(entry['image']) == entry['image_sha256'], 'image changed: ' + entry['image'])

    check_inputs()
    entries = [entry for entry in prepared['entries'] if not args.only or entry['fixture'] in args.only]
    with args.output.open('x') as lines:
        for entry in entries:
            for block in range(1, args.blocks + 1):
                for process in range(1, args.processes + 1):
                    for mode in alternating_modes(block, process):
                        row = dict(actor=entry['actor'], fixture=entry['fixture'], block=block, process=process,
                                   mode=mode, **observe(host, entry, mode, args))
                        lines.write(json.dumps(row, sort_keys=True) + '\n')
                        lines.flush()
            print(f"measured {entry['actor']} {entry['fixture']}", flush=True)
    check_inputs()
    print(args.output)


def quartiles(values):
    values = sorted(values)
    if len(values) < 2:
        return values[0], values[0], values[0]
    q1, median, q3 = statistics.quantiles(values, n=4, method='inclusive')
    return q1, median, q3


def summarize_rows(rows):
    """Per actor/fixture: median faults per mode and entry ms per block."""
    groups = {}
    for row in rows:
        groups.setdefault((row['actor'], row['fixture']), []).append(row)
    summary = []
    for (actor, fixture), group in sorted(groups.items()):
        item = dict(actor=actor, fixture=fixture, faults={}, blocks={})
        for mode in MODES:
            selected = [row for row in group if row['mode'] == mode]
            item['faults'][mode] = statistics.median(row['minflt_execution'] for row in selected)
        for block in sorted({row['block'] for row in group}):
            entry = {}
            for mode in MODES:
                q1, median, q3 = quartiles([row['execution_ns'] / 1e6 for row in group
                                            if row['block'] == block and row['mode'] == mode])
                entry[mode] = dict(median_ms=round(median, 3), q1_ms=round(q1, 3), q3_ms=round(q3, 3))
            entry['saved'] = round(1 - entry['pooled']['median_ms'] / entry['on-demand']['median_ms'], 3)
            item['blocks'][block] = entry
        summary.append(item)
    return summary


def summarize(args):
    rows = [json.loads(line) for line in args.results.read_text().splitlines() if line.strip()]
    require(rows, 'no rows')
    json.dump(summarize_rows(rows), sys.stdout, indent=1)
    print()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    p = commands.add_parser('prepare', help='produce native images for one host')
    p.add_argument('--host', type=Path, required=True, help='compiler_precompiled_baseline executable')
    p.add_argument('--output', type=Path, required=True, help='fresh directory')
    p.add_argument('--actor', action='append', choices=('scriptw', 'scriptc'), required=True)
    p.add_argument('--scriptc', type=Path, help='Scriptc 0.4.5 executable')
    p.add_argument('--runtime-pack', type=Path, help='@scriptc/runtime-wasm32-wasi 0.4.5 directory')
    p.add_argument('--zig', type=Path, help='Zig 0.16.0 executable')
    p.add_argument('fixtures', type=Path, nargs='+', help='fixture sources whose sibling manifest records stdout')
    p.set_defaults(handler=prepare)
    r = commands.add_parser('run', help='alternate pooled and on-demand fresh processes')
    r.add_argument('--prepared', type=Path, required=True)
    r.add_argument('--output', type=Path, required=True, help='new JSON-lines file')
    r.add_argument('--processes', type=int, default=8, help='process pairs per block')
    r.add_argument('--blocks', type=int, default=2)
    r.add_argument('--cpu', type=int, default=max(os.sched_getaffinity(0)))
    r.add_argument('--samples', type=int, default=3)
    r.add_argument('--warmups', type=int, default=3)
    r.add_argument('--index', type=int, default=3, help='observation whose entry is reported and fault-counted')
    r.add_argument('--only', action='append', help='fixture file name to measure (repeatable)')
    r.set_defaults(handler=run)
    s = commands.add_parser('summarize', help='reduce run output')
    s.add_argument('results', type=Path)
    s.set_defaults(handler=summarize)
    args = parser.parse_args()
    args.handler(args)


if __name__ == '__main__':
    main()
