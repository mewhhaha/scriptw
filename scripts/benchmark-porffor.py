#!/usr/bin/env python3
"""Compare Scriptw with Porffor (https://porffor.dev), another JavaScript-to-Wasm AOT compiler.

  prepare  identify the pinned tools, build the original17 kernels with both compilers
           (untimed) and run every program once with checked output. Each compile
           failure, rejected module, runtime failure and wrong output is a failed
           attempt in the receipt; only passing actor/case pairs are timed.
  run      time the admitted programs in fresh processes: Scriptw on the shared
           Wasmtime host, Porffor's Wasm on Wasmtime and on Node's WebAssembly
           engine, and the original source on Node/V8.
  compile  time fresh JavaScript-to-Wasm compiler processes of both compilers on one CPU.
  publish  check one complete run and compile measurement and write the summary JSON.
  tables   print the Markdown tables of a published summary.

Porffor's Wasm imports its own host functions (print, printChar, clocks), so
it runs through two benchmark-only harnesses: scripts/wasm_peer_node.cjs on
Node's WebAssembly engine, as Porffor's own runner does, and
scripts/wasm_peer_wasmtime.py on Wasmtime with the shared host's native tier.
"""
import argparse
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys

from benchmark_metrics import digest, measure_process
from compiler_comparison import ADAPTER
from limited_verification import limited_environment, require_cloud_budget

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths'
V8_HARNESS = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_baseline/node-runtime.cjs'
NODE_WASM_HARNESS = ROOT / 'scripts/wasm_peer_node.cjs'
WASMTIME_HARNESS = ROOT / 'scripts/wasm_peer_wasmtime.py'
PACKAGE, VERSION, WASMTIME_PY = 'porffor', '0.61.13', '47.0.1'
NODE_VERSION = '24.22.1'
ACTORS = ('scriptw', 'porffor.wasmtime', 'porffor.v8', 'v8')
LABELS = {'scriptw': 'Scriptw', 'porffor.wasmtime': 'Porffor on Wasmtime',
          'porffor.v8': 'Porffor on Node/V8 Wasm', 'v8': 'Node/V8 JavaScript'}
ENGINES = {'scriptw': 'Wasmtime 47.0.0 / Cranelift, shared precompiled host (Rust)',
           'porffor.wasmtime': 'Wasmtime 47.0.1 / Cranelift through wasmtime-py, same native tier as the host',
           'porffor.v8': 'Node 24.22.1 WebAssembly (V8 Liftoff, dynamic tier-up to TurboFan)',
           'v8': 'Node 24.22.1 JavaScript (V8 Ignition/Sparkplug/Maglev/TurboFan)'}
COMPILERS = ('scriptw', 'porffor')
SAMPLES, WARMUPS, PROCESSES_PER_BLOCK, CHECK_SAMPLES, CHECK_WARMUPS = 21, 10, 4, 3, 1
# A 4x4 Williams square: each actor leads, and follows every other, once per four rows.
WILLIAMS = ((0, 1, 3, 2), (1, 2, 0, 3), (2, 3, 1, 0), (3, 0, 2, 1))
SCHEMA = 'scriptw-wasm-peer-comparison-v1'
FAILED = ('compile-failed', 'module-rejected', 'unsupported-interface', 'runtime-failed', 'wrong-output')
POLICY = dict(
    execution=('Public-entry wall of a fresh instance per entry: Scriptw evaluate entry (Realm bootstrap inside), '
               'Porffor exported main (fresh Instance, linear memory and globals), V8 runInContext in a fresh context. '
               'Module compilation, native image loading and instantiation are outside the timer.'),
    engines=('Scriptw: precompiled native image on the shared Wasmtime 47.0.0 host, compiler disabled in the consumer. '
             'Porffor on Wasmtime: wasmtime-py 47.0.1 compiles the Wasm in-process before timing with the same Cranelift '
             'settings (SpeedAndSize, backtracking, explicit x86_64-unknown-linux-gnu target, no host CPU feature inference) '
             'and a one-Store pool keeping 16 MiB per memory slot resident; output is captured by a Wasm adapter, the entry is '
             'an unchecked ctypes call whose empty-call floor is retained per entry. Porffor on Node/V8: the module is '
             'compiled before timing; V8 compiles functions lazily with Liftoff and tiers up to TurboFan during warmups.'),
    v8='Normal V8 lazy compilation, JIT tiering and natural GC may occur inside entries; ten warmups do not assert steady state.',
    source=('Original17 bytes for Scriptw and V8. Porffor receives the same print-to-console.log adapter Scriptc receives, '
            'default -O1 optimization, Wasm target.'),
    correctness=('Every timed entry must print the manifest output exactly. Before timing, each actor/case runs once in its '
                 'own process (3 samples, 1 warmup); compile failures, rejected modules, runtime failures and wrong outputs '
                 'are retained as failed attempts and are not timed.'),
    statistics=('Fresh process is the unit: four processes per actor/case in each of two blocks (Williams order, second '
                'block reversed), 21 measured entries after 10 checked warmups each; per-actor medians of process medians.'),
    memory='GNU-time whole-child peak RSS: Node or Python runtimes plus the program; not a live-heap comparison.')


def save(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def load(path):
    return json.loads(Path(path).read_text())


def stamp():
    return datetime.now(timezone.utc).isoformat()


def require(condition, message):
    if not condition:
        raise ValueError('wasm peer comparison: ' + message)


def identity(path):
    path = Path(path)
    return dict(path=str(path), bytes=path.stat().st_size, sha256=digest(path))


def cases():
    manifest = load(FIXTURES / 'manifest.json')
    rows = manifest['execution']
    require(len(rows) == 17 and all(Path(row['file']).name == row['file'] for row in rows), 'the original17 corpus is required')
    return rows


def process_plan(names, admitted):
    """Fresh-process order: per block and case, four Williams rows of the admitted actors."""
    require(len(names) == len(set(names)), 'duplicate case')
    result = []
    for block in range(2):
        ordered = list(enumerate(names))
        if block == 1:
            ordered.reverse()
        for index, name in ordered:
            for row in range(PROCESSES_PER_BLOCK):
                sequence = WILLIAMS[(index + row) % len(WILLIAMS)]
                if block == 1:
                    sequence = sequence[::-1]
                for slot in sequence:
                    actor = ACTORS[slot]
                    if actor in admitted.get(name, ()):
                        result.append(dict(block=block, case=name, actor=actor))
    return result


def compile_plan(names):
    """ABBA per case with alternating leader; the second block reverses cases and leaders."""
    result = []
    for block in range(2):
        ordered = list(enumerate(names))
        if block == 1:
            ordered.reverse()
        for index, name in ordered:
            first, second = COMPILERS if (index + block) % 2 == 0 else COMPILERS[::-1]
            for actor in (first, second, second, first):
                result.append(dict(block=block, case=name, actor=actor))
    return result


def entry_times(report, actor, expected, samples, warmups):
    """Checked per-entry wall times of one harness report; raises on any failed or unmatched entry."""
    if actor == 'v8':
        require(report['schema'] == 'scriptw-v8-baseline-v2' and len(report['cases']) == 1, 'V8 report differs')
        observations = report['cases'][0]['observations']
        key = 'execution_ns'
    elif actor == 'scriptw':
        require(report['schema'] == 'scriptw-precompiled-execution-v1' and report['compiler_enabled'] is False,
                'Scriptw report differs')
        observations = report['observations']
        key = None
    else:
        schema = 'scriptw-wasm-peer-wasmtime-v1' if actor == 'porffor.wasmtime' else 'scriptw-wasm-peer-node-v1'
        require(report['schema'] == schema and report['status'] == 'complete', actor + ' report differs')
        observations = report['observations']
        key = 'execution_ns'
    require(report['samples'] == samples and report['warmups'] == warmups, 'sample counts differ')
    require(len(observations) == samples + warmups, 'incomplete observations')
    require([row['warmup'] for row in observations] == [True] * warmups + [False] * samples, 'warmup order differs')
    for row in observations:
        require(row['stdout'] == expected, 'output differs')
        if actor != 'v8':
            require(row['matched'] is True and row['completion'] == 'normal', 'failed entry')
    measured = observations[warmups:]
    if key is None:
        return [row['phases_ns']['execution'] for row in measured]
    return [row[key] for row in measured]


def classify(actor, compiled, exit_code, report, expected):
    """One checked attempt: 'passed' or a failed-attempt status with its detail."""
    if compiled is not None and compiled['status'] != 'compiled':
        return dict(status='compile-failed', detail=compiled['detail'])
    if report is None:
        return dict(status='runtime-failed', detail=f'harness exited with status {exit_code}')
    if actor in ('porffor.wasmtime', 'porffor.v8') and report.get('status') in ('module-rejected', 'unsupported-interface'):
        return dict(status=report['status'], detail=report.get('error') or ', '.join(report.get('imports', [])))
    observations = report['cases'][0]['observations'] if actor == 'v8' else report['observations']
    for row in observations:
        if actor != 'v8' and row.get('completion') != 'normal':
            return dict(status='runtime-failed', detail=f"{row['completion']}: {row.get('error', '')}"[:600])
        if row['stdout'] != expected:
            return dict(status='wrong-output', detail='printed ' + json.dumps(row['stdout'][:200]) + ', expected ' + json.dumps(expected))
    if exit_code != 0:
        return dict(status='runtime-failed', detail=f'harness exited with status {exit_code}')
    return dict(status='passed', detail='')


def scope(cloud, processes):
    affinity = sorted(os.sched_getaffinity(0))
    require(len(affinity) <= processes, f'at most {processes} inherited CPUs, the same for every actor')
    machine = dict(platform=platform.platform(), affinity=affinity,
                   cpu_model=next(line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                                  if line.startswith('model name')))
    if cloud:
        machine['cloud'] = dict(inherited_memory_cap_bytes=require_cloud_budget(serial=False))
    return machine


def environment(output):
    env = limited_environment()
    for key in ('NODE_OPTIONS', 'NODE_COMPILE_CACHE', 'LD_PRELOAD', 'LD_AUDIT', 'PYTHONPATH', 'PYTHONHOME'):
        require(not env.get(key), 'forbidden inherited setting: ' + key)
    for key in list(env):
        if key.startswith('SCRIPTW_PRECOMPILED_') or key in ('SCRIPTW_CACHE_DIR', 'NODE_PATH', 'NODE_EXTRA_CA_CERTS'):
            del env[key]
    env.update(NODE_DISABLE_COMPILE_CACHE='1', TMPDIR=str(output / 'tmp'), SCRIPTW_CACHE_DIR=str(output / 'cache'))
    return env


def peer_entry(tools):
    """The compiler's Node entry point, as its own package manifest names it."""
    package = load(tools / 'node_modules' / PACKAGE / 'package.json')
    require(package['name'] == PACKAGE and package['version'] == VERSION, 'the pinned compiler package is required')
    entries = package['bin'] if isinstance(package['bin'], dict) else {PACKAGE: package['bin']}
    require(len(entries) == 1, 'one compiler executable expected')
    return (tools / 'node_modules' / PACKAGE / next(iter(entries.values()))).resolve(strict=True)


def tool_identities(args, env):
    node = Path(shutil.which(str(args.node)) or args.node).resolve(strict=True)
    versions = json.loads(subprocess.check_output([node, '-p', 'JSON.stringify(process.versions)'], env=env, text=True))
    require(versions['node'] == NODE_VERSION, 'Node ' + NODE_VERSION + ' is required')
    tools = args.tools.resolve(strict=True)
    entry = peer_entry(tools)
    reported = subprocess.check_output([node, entry, '--version'], env=env, text=True, cwd=tools).strip()
    require(reported == VERSION, 'compiler reports another version: ' + reported)
    lock = tools / 'package-lock.json'
    installed = {name.removeprefix('node_modules/'): row['version']
                 for name, row in load(lock)['packages'].items() if name and (tools / name / 'package.json').exists()}
    require(installed.get(PACKAGE) == VERSION, 'the lock file does not pin the compiler package')
    python = Path(args.wasmtime_python).absolute()
    probe = ('import json, wasmtime; from importlib import metadata; from wasmtime import _ffi; '
             'print(json.dumps(dict(version=metadata.version("wasmtime"), library=_ffi.dll._name)))')
    wasmtime = json.loads(subprocess.check_output([python, '-I', '-c', probe], env=env, text=True))
    require(wasmtime['version'] == WASMTIME_PY, 'wasmtime-py ' + WASMTIME_PY + ' is required')
    host = args.host.resolve(strict=True)
    host_identity = json.loads(subprocess.check_output([host, 'identity'], env=env, text=True, timeout=60))
    require(host_identity['executable_sha256'] == digest(host), 'host identity does not describe its bytes')
    scriptw = args.scriptw.resolve(strict=True)
    compiler = json.loads(subprocess.check_output([scriptw, 'compiler-identity'], env=env, text=True, timeout=60))
    require(compiler['executable_sha256'] == digest(scriptw)
            and compiler['source_fingerprint'] == host_identity['source_fingerprint'], 'CLI and host differ in source')
    return dict(
        node=dict(identity(node), versions=dict(node=versions['node'], v8=versions['v8'])),
        peer=dict(package=PACKAGE, version=VERSION, entry=identity(entry), lock=identity(lock),
                  installed_packages=installed, optimization='default (-O1)', target='wasm'),
        wasmtime_py=dict(python=str(python), version=wasmtime['version'], library=identity(wasmtime['library'])),
        host=dict(identity(host), identity=host_identity),
        scriptw=dict(identity(scriptw), identity=compiler),
        time=identity(args.time.resolve(strict=True)),
        harnesses={path.name: identity(path) for path in (V8_HARNESS, NODE_WASM_HARNESS, WASMTIME_HARNESS)})


def argv_for(actor, prepared, output_root, case, samples, warmups):
    tools = prepared['tools']
    name = case['file']
    if actor == 'scriptw':
        image = output_root / 'scriptw' / prepared['cases'][name]['scriptw_image']
        return [tools['host']['path'], 'execute-scriptw', str(image), case['stdout'], str(samples), str(warmups)]
    if actor == 'v8':
        return [tools['node']['path'], str(V8_HARNESS), str(FIXTURES), str(samples), str(warmups), name]
    wasm = str(output_root / 'wasm' / (name + '.wasm'))
    if actor == 'porffor.v8':
        return [tools['node']['path'], str(NODE_WASM_HARNESS), wasm, case['stdout'], str(samples), str(warmups)]
    return [tools['wasmtime_py']['python'], '-I', str(WASMTIME_HARNESS), wasm, case['stdout'], str(samples), str(warmups)]


def invoke(argv, folder, stem, env, time_binary, cwd=ROOT):
    with (folder / (stem + '.stdout')).open('xb') as stdout, (folder / (stem + '.stderr')).open('xb') as stderr:
        resources = measure_process(argv, cwd=cwd, env=env, stdout=stdout, stderr=stderr, timeout=900, time_binary=time_binary)
    save(folder / (stem + '.command.json'), dict(argv=list(map(str, argv)), **resources))
    try:
        report = load(folder / (stem + '.stdout'))
    except (ValueError, UnicodeError):
        report = None
    return resources, report


def prepare(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('tmp', 'cache', 'adapted', 'wasm', 'logs', 'checks'):
        (output / name).mkdir()
    machine = scope(args.cloud, 2)
    env = environment(output)
    tools = tool_identities(args, env)
    entry = Path(tools['peer']['entry']['path'])
    rows = cases()
    prepared = dict(schema=SCHEMA + '-prepared', status='incomplete', started_at=stamp(), machine=machine, tools=tools,
                    adapter=ADAPTER, cases={})
    # Porffor: the adapted source to Wasm, one fresh untimed process per kernel.
    for number, case in enumerate(rows):
        name = case['file']
        adapted = output / 'adapted' / name
        adapted.write_bytes(ADAPTER.encode() + (FIXTURES / name).read_bytes())
        wasm = output / 'wasm' / (name + '.wasm')
        resources, _ = invoke([tools['node']['path'], entry, 'wasm', adapted, wasm], output / 'logs', f'{number:02d}-compile-{name}',
                              env, args.time)
        compiled = dict(status='compiled', detail='', exit_code=resources['exit_code'], elapsed_ns=resources['elapsed_ns'])
        if resources['exit_code'] != 0 or not wasm.exists() or wasm.read_bytes()[:8] != b'\0asm\1\0\0\0':
            stderr = (output / 'logs' / f'{number:02d}-compile-{name}.stderr').read_text(errors='replace').strip()
            compiled.update(status='compile-failed', detail=stderr[-600:] or f"exit status {resources['exit_code']}")
        else:
            compiled['wasm'] = dict(bytes=wasm.stat().st_size, sha256=digest(wasm))
        prepared['cases'][name] = dict(source_sha256=digest(FIXTURES / name), adapted_sha256=digest(adapted),
                                       stdout=case['stdout'], compiled=compiled, attempts={})
    # Scriptw: one producer process prepares every native image for the shared host.
    resources, producer = invoke([tools['host']['path'], 'prepare-scriptw', FIXTURES, output / 'scriptw'], output / 'logs',
                                 'scriptw-producer', env, args.time)
    require(resources['exit_code'] == 0 and producer['source_fingerprint'] == tools['host']['identity']['source_fingerprint'],
            'Scriptw producer failed')
    for row in producer['cases']:
        case = prepared['cases'][row['file']]
        require(row['source_sha256'] == case['source_sha256'] and row['stdout'] == case['stdout'], 'Scriptw producer corpus differs')
        require(digest(output / 'scriptw' / row['image']) == row['image_sha256'], 'Scriptw image differs')
        case.update(scriptw_image=row['image'], scriptw_image_sha256=row['image_sha256'],
                    scriptw_program_wasm=dict(bytes=row['program_wasm_bytes'], sha256=row['program_wasm_sha256']))
    # Every actor runs every kernel once, outputs checked; failures are retained, not raised.
    for case in rows:
        name = case['file']
        record = prepared['cases'][name]
        for actor in ACTORS:
            compiled = record['compiled'] if actor.startswith(PACKAGE + '.') else None
            if compiled is not None and compiled['status'] != 'compiled':
                record['attempts'][actor] = classify(actor, compiled, None, None, case['stdout'])
                continue
            argv = argv_for(actor, prepared, output, case, CHECK_SAMPLES, CHECK_WARMUPS)
            resources, report = invoke(argv, output / 'checks', f'{actor}-{name}', env, args.time)
            attempt = classify(actor, None, resources['exit_code'], report, case['stdout'])
            if attempt['status'] == 'runtime-failed' and report is None:
                stderr = (output / 'checks' / f'{actor}-{name}.stderr').read_text(errors='replace').strip()
                attempt['detail'] += ': ' + stderr[-600:]
            record['attempts'][actor] = attempt
            print(name, actor, attempt['status'], flush=True)
        if any(record['attempts'][actor]['status'] in ('runtime-failed', 'wrong-output') for actor in ACTORS[1:3]):
            # The compiler's own runner on the same adapted source, so a failure is not the harnesses'.
            stem = f'own-runner-{name}'
            resources, _ = invoke([tools['node']['path'], entry, output / 'adapted' / name], output / 'checks', stem, env, args.time)
            text = '\n'.join((output / 'checks' / (stem + suffix)).read_text(errors='replace').strip() for suffix in ('.stdout', '.stderr'))
            record['own_runner'] = dict(exit_code=resources['exit_code'], output=text.strip()[-600:])
    prepared.update(status='complete', finished_at=stamp())
    shutil.rmtree(output / 'tmp')
    shutil.rmtree(output / 'cache')
    save(output / 'prepared.json', prepared)
    print(output / 'prepared.json')


def admitted_actors(prepared):
    return {name: [actor for actor in ACTORS if record['attempts'][actor]['status'] == 'passed']
            for name, record in prepared['cases'].items()}


def inputs(prepared, root):
    tools = prepared['tools']
    paths = [tools['node']['path'], tools['host']['path'], tools['scriptw']['path'], tools['time']['path'],
             tools['peer']['entry']['path'], tools['peer']['lock']['path'], tools['wasmtime_py']['library']['path'],
             *[row['path'] for row in tools['harnesses'].values()], FIXTURES / 'manifest.json']
    for name, record in prepared['cases'].items():
        paths += [FIXTURES / name, root / 'scriptw' / record['scriptw_image']]
        if record['compiled']['status'] == 'compiled':
            paths.append(root / 'wasm' / (name + '.wasm'))
    return {str(path): digest(path) for path in sorted(set(map(str, paths)))}


def checked_inputs(prepared, root):
    current = inputs(prepared, root)
    tools = prepared['tools']
    for key in ('node', 'host', 'scriptw', 'time'):
        require(current[tools[key]['path']] == tools[key]['sha256'], key + ' changed since prepare')
    for name, record in prepared['cases'].items():
        require(current[str(root / 'scriptw' / record['scriptw_image'])] == record['scriptw_image_sha256'], 'image changed')
        if record['compiled']['status'] == 'compiled':
            require(current[str(root / 'wasm' / (name + '.wasm'))] == record['compiled']['wasm']['sha256'], 'Wasm changed')
    return current


def reduce_rows(rows, names):
    result = []
    for name in names:
        actors = {}
        for actor in ACTORS:
            group = [row for row in rows if row['case'] == name and row['actor'] == actor]
            if not group:
                continue
            medians = [statistics.median(row['wall_ns']) for row in group]
            actors[actor] = dict(
                processes=len(group), median_ns=statistics.median(medians),
                mean_ns=statistics.mean(statistics.mean(row['wall_ns']) for row in group),
                block_median_ns=[statistics.median(statistics.median(row['wall_ns']) for row in group if row['block'] == block)
                                 for block in range(2)],
                peak_rss_bytes=statistics.median(row['resources']['peak_rss_bytes'] for row in group))
            if actor == 'porffor.wasmtime':
                actors[actor]['empty_call_median_ns'] = statistics.median(value for row in group for value in row['empty_call_ns'])
        result.append(dict(case=name, actors=actors))
    return result


def comparisons(summary):
    """Each other actor against Scriptw over the kernels both completed."""
    result = {}
    for other in ACTORS[1:]:
        shared = [row for row in summary if 'scriptw' in row['actors'] and other in row['actors']]
        ratios = [row['actors']['scriptw']['median_ns'] / row['actors'][other]['median_ns'] for row in shared]
        faster = [row['case'] for row in shared if all(
            row['actors']['scriptw']['block_median_ns'][block] < row['actors'][other]['block_median_ns'][block] for block in range(2))]
        result[other] = dict(kernels=len(shared), scriptw_faster_in_both_blocks=faster,
                             scriptw_time_ratio_geomean=math.exp(statistics.mean(map(math.log, ratios))) if ratios else None,
                             scriptw_time_ratio_range=[min(ratios), max(ratios)] if ratios else None)
    return result


def run(args):
    root = args.prepared.resolve()
    prepared = load(root / 'prepared.json')
    require(prepared['schema'] == SCHEMA + '-prepared' and prepared['status'] == 'complete', 'complete preparation required')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('tmp', 'cache', 'raw'):
        (output / name).mkdir()
    machine = scope(args.cloud, 2)
    env = environment(output)
    samples, warmups = SAMPLES, (1 if args.smoke else WARMUPS)
    before = checked_inputs(prepared, root)
    save(output / 'inputs-before.json', before)
    rows_by_name = {row['file']: row for row in cases()}
    names = list(rows_by_name)
    admitted = admitted_actors(prepared)
    if args.case:
        names = [name for name in names if name in args.case]
    plan = process_plan(names, admitted)
    report = dict(schema=SCHEMA + '-run', status='incomplete', started_at=stamp(), smoke=args.smoke, machine=machine,
                  samples=samples, warmups=warmups, policy=POLICY, engines=ENGINES, plan=plan, prepared_sha256=digest(root / 'prepared.json'),
                  measurement_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(), rows=[])
    for number, entry in enumerate(plan):
        actor, name = entry['actor'], entry['case']
        case = rows_by_name[name]
        stem = f'{number:03d}-{actor}-{name}'
        argv = argv_for(actor, prepared, root, case, samples, warmups)
        resources, raw = invoke(argv, output / 'raw', stem, env, args.time)
        require(resources['exit_code'] == 0 and not resources['timed_out'] and raw is not None, 'process failed: ' + stem)
        row = dict(entry, raw='raw/' + stem + '.stdout', raw_sha256=digest(output / 'raw' / (stem + '.stdout')),
                   resources=resources, wall_ns=entry_times(raw, actor, case['stdout'], samples, warmups))
        if actor == 'porffor.wasmtime':
            row['empty_call_ns'] = [value['empty_call_ns'] for value in raw['observations'][warmups:]]
        if actor == 'scriptw':
            row['store_allocation'] = raw.get('store_allocation')
        report['rows'].append(row)
        print(f'{number + 1}/{len(plan)}', actor, name, round(statistics.median(row['wall_ns']) / 1e6, 4), 'ms', flush=True)
    after = checked_inputs(prepared, root)
    save(output / 'inputs-after.json', after)
    require(before == after, 'an input changed during measurement')
    require(sorted(os.sched_getaffinity(0)) == machine['affinity'], 'CPU affinity changed')
    report['summary'] = reduce_rows(report['rows'], names)
    report.update(status='complete', finished_at=stamp(), processes=len(plan), comparisons=comparisons(report['summary']))
    shutil.rmtree(output / 'tmp')
    shutil.rmtree(output / 'cache')
    save(output / 'analysis.json', report)
    print(output / 'analysis.json')


def compile_timing(args):
    root = args.prepared.resolve()
    prepared = load(root / 'prepared.json')
    require(prepared['status'] == 'complete', 'complete preparation required')
    require(len(os.sched_getaffinity(0)) == 1, 'one CPU required, the same for both compilers')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('logs', 'work', 'tmp'):
        (output / name).mkdir()
    machine = scope(args.cloud, 1)
    env = environment(output)
    before = checked_inputs(prepared, root)
    tools = prepared['tools']
    names = [name for name, record in prepared['cases'].items() if record['compiled']['status'] == 'compiled']
    plan = compile_plan(names)
    report = dict(schema=SCHEMA + '-compile', status='incomplete', started_at=stamp(), machine=machine, plan=plan, rows=[],
                  excluded={name: record['compiled']['detail'] for name, record in prepared['cases'].items() if name not in names},
                  policy=('Parent wall around a GNU-time-supervised fresh compiler process on one CPU: startup, source read, '
                          'compilation and Wasm output. Scriptw build wasm writes P and its shared R sidecar; Porffor runs its '
                          'JavaScript compiler on Node with the print adapter and writes standalone Wasm. Fresh work, tmp and '
                          'Scriptw cache directories per process; Node compile cache disabled; OS page caches retained.'))
    for number, entry in enumerate(plan):
        actor, name = entry['actor'], entry['case']
        stem = f'{number:03d}-{actor}-{name}'
        work = output / 'work' / stem
        (work / 'tmp').mkdir(parents=True)
        (work / 'cache').mkdir()
        child = dict(env, TMPDIR=str(work / 'tmp'), SCRIPTW_CACHE_DIR=str(work / 'cache'))
        wasm = work / 'program.wasm'
        if actor == 'scriptw':
            source = work / name
            shutil.copyfile(FIXTURES / name, source)
            child['SCRIPTW_WASM_DUMP'] = str(wasm)
            argv = [tools['scriptw']['path'], 'build', 'wasm', str(source)]
        else:
            source = work / name
            shutil.copyfile(root / 'adapted' / name, source)
            argv = [tools['node']['path'], tools['peer']['entry']['path'], 'wasm', str(source), str(wasm)]
        resources, _ = invoke(argv, output / 'logs', stem, child, args.time, cwd=work)
        require(resources['exit_code'] == 0 and not resources['timed_out'], 'failed build: ' + stem)
        require(wasm.read_bytes()[:8] == b'\0asm\1\0\0\0', 'no portable Wasm: ' + stem)
        report['rows'].append(dict(entry, resources=resources, wasm=dict(bytes=wasm.stat().st_size, sha256=digest(wasm))))
        shutil.rmtree(work)
        print(f'{number + 1}/{len(plan)}', stem, round(resources['elapsed_ns'] / 1e6, 1), 'ms', flush=True)
    require(checked_inputs(prepared, root) == before, 'an input changed during measurement')
    summary = []
    for name in names:
        actors = {}
        for actor in COMPILERS:
            group = [row for row in report['rows'] if row['case'] == name and row['actor'] == actor]
            actors[actor] = dict(processes=len(group), median_ns=statistics.median(row['resources']['elapsed_ns'] for row in group),
                                 block_median_ns=[statistics.median(row['resources']['elapsed_ns'] for row in group if row['block'] == block)
                                                  for block in range(2)],
                                 peak_rss_bytes=statistics.median(row['resources']['peak_rss_bytes'] for row in group),
                                 wasm_bytes=statistics.median(row['wasm']['bytes'] for row in group))
        summary.append(dict(case=name, actors=actors))
    ratios = [row['actors']['scriptw']['median_ns'] / row['actors']['porffor']['median_ns'] for row in summary]
    report.update(status='complete', finished_at=stamp(), summary=summary,
                  scriptw_time_ratio_geomean=math.exp(statistics.mean(map(math.log, ratios))),
                  scriptw_faster=[row['case'] for row in summary if row['actors']['scriptw']['median_ns'] < row['actors']['porffor']['median_ns']])
    shutil.rmtree(output / 'tmp')
    save(output / 'compile.json', report)
    print(output / 'compile.json')


def public_tool(tool):
    """Identities without host paths."""
    return {key: value for key, value in tool.items() if key not in ('path', 'python')} | (
        {'file': Path(tool['path']).name} if 'path' in tool else {})


def publish(args):
    run_report = load(args.run / 'analysis.json')
    compile_report = load(args.compile / 'compile.json')
    require(run_report['schema'] == SCHEMA + '-run' and run_report['status'] == 'complete' and not run_report['smoke'],
            'a complete, non-smoke run is required')
    require(compile_report['schema'] == SCHEMA + '-compile' and compile_report['status'] == 'complete', 'a complete compile run is required')
    prepared = load(args.prepared / 'prepared.json')
    require(run_report['prepared_sha256'] == digest(args.prepared / 'prepared.json'), 'run used another preparation')
    require(len(run_report['summary']) == 17, 'publication needs the whole original17 corpus')
    tools = prepared['tools']
    failures = [dict(case=name, actor=actor, **attempt, own_runner=record.get('own_runner'))
                for name, record in prepared['cases'].items()
                for actor, attempt in record['attempts'].items() if attempt['status'] != 'passed']
    finished = datetime.fromisoformat(run_report['finished_at'])
    value = dict(
        schema=SCHEMA, date=finished.date().isoformat(), run_started_at=run_report['started_at'], run_finished_at=run_report['finished_at'],
        compile_finished_at=compile_report['finished_at'], measurement_commit=run_report['measurement_commit'],
        scriptw=dict(source_commit=tools['host']['identity']['source_commit'], source_fingerprint=tools['host']['identity']['source_fingerprint'],
                     host_sha256=tools['host']['sha256'], cli_sha256=tools['scriptw']['sha256'],
                     shared_configuration=tools['host']['identity']['shared_configuration']),
        tools=dict(node=public_tool(tools['node']), peer=dict(public_tool(tools['peer']), entry=public_tool(tools['peer']['entry']),
                                                               lock=public_tool(tools['peer']['lock'])),
                   wasmtime_py=dict(version=tools['wasmtime_py']['version'], library=public_tool(tools['wasmtime_py']['library'])),
                   harnesses={name: public_tool(row) for name, row in tools['harnesses'].items()}),
        machine=dict(run=run_report['machine'], compile=compile_report['machine']),
        actors={actor: dict(label=LABELS[actor], engine=ENGINES[actor]) for actor in ACTORS},
        policy=run_report['policy'], compile_policy=compile_report['policy'],
        samples=run_report['samples'], warmups=run_report['warmups'], processes=run_report['processes'],
        runtime=run_report['summary'], comparisons=run_report['comparisons'], failures=failures,
        compile=dict(processes=len(compile_report['rows']), summary=compile_report['summary'],
                     scriptw_time_ratio_geomean=compile_report['scriptw_time_ratio_geomean'],
                     scriptw_faster=compile_report['scriptw_faster'], excluded=compile_report['excluded']),
        evidence=dict(analysis_sha256=digest(args.run / 'analysis.json'), compile_sha256=digest(args.compile / 'compile.json'),
                      prepared_sha256=digest(args.prepared / 'prepared.json'),
                      retained='raw process reports stay in the measurement directories; this summary is their checked reduction'))
    args.output.write_text(json.dumps(value, indent=2) + '\n')
    print(args.output)


def milliseconds(value):
    return f'{value / 1e6:.3f}'


def tables(args):
    value = load(args.summary)
    failures = {(row['case'], row['actor']): row for row in value['failures']}
    print('| Kernel | ' + ' | '.join(LABELS[actor] + ' (ms)' for actor in ACTORS) + ' | Scriptw ÷ Porffor on Wasmtime |')
    print('| --- | ' + ' | '.join('---:' for _ in ACTORS) + ' | ---: |')
    for row in value['runtime']:
        cells = []
        for actor in ACTORS:
            if actor in row['actors']:
                cells.append(milliseconds(row['actors'][actor]['median_ns']))
            else:
                cells.append(failures[(row['case'], actor)]['status'].replace('-', ' '))
        ratio = '—'
        if 'scriptw' in row['actors'] and 'porffor.wasmtime' in row['actors']:
            ratio = f"{row['actors']['scriptw']['median_ns'] / row['actors']['porffor.wasmtime']['median_ns']:.2f}×"
        print(f"| `{row['case']}` | " + ' | '.join(cells) + f' | {ratio} |')
    print()
    print('| Comparison | Kernels | Scriptw faster in both blocks | Scriptw time ÷ other, geomean | Range |')
    print('| --- | ---: | ---: | ---: | --- |')
    for other, comparison in value['comparisons'].items():
        low, high = comparison['scriptw_time_ratio_range']
        print(f"| Scriptw vs {LABELS[other]} | {comparison['kernels']} | {len(comparison['scriptw_faster_in_both_blocks'])} | "
              f"{comparison['scriptw_time_ratio_geomean']:.3f}× | {low:.2f}–{high:.2f}× |")
    print()
    print('| Kernel | Scriptw build (ms) | Porffor build (ms) | Scriptw P Wasm (bytes) | Porffor Wasm (bytes) |')
    print('| --- | ---: | ---: | ---: | ---: |')
    for row in value['compile']['summary']:
        actors = row['actors']
        print(f"| `{row['case']}` | {actors['scriptw']['median_ns'] / 1e6:.1f} | {actors['porffor']['median_ns'] / 1e6:.1f} | "
              f"{int(actors['scriptw']['wasm_bytes']):,} | {int(actors['porffor']['wasm_bytes']):,} |")
    print()
    print('| Kernel | Actor | Failed attempt | Detail |')
    print('| --- | --- | --- | --- |')
    for row in value['failures']:
        detail = row['detail'].replace('|', '\\|').replace('\n', ' ')
        print(f"| `{row['case']}` | {LABELS[row['actor']]} | {row['status']} | {detail[:160]} |")


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    cloud = "admit a managed cloud machine's inherited finite memory cap and record it"
    step = commands.add_parser('prepare')
    step.add_argument('--output', type=Path, required=True, help='new preparation directory')
    step.add_argument('--tools', type=Path, required=True, help='npm prefix holding the pinned compiler package and its package-lock.json')
    step.add_argument('--wasmtime-python', type=Path, required=True, help='Python interpreter with wasmtime==' + WASMTIME_PY)
    step.add_argument('--host', type=Path, required=True, help='shared precompiled host (compiler_precompiled_baseline)')
    step.add_argument('--scriptw', type=Path, required=True, help='Scriptw CLI built from the same source as the host')
    step.add_argument('--node', default='node')
    step.add_argument('--time', type=Path, required=True, help='GNU time')
    step.add_argument('--cloud', action='store_true', help=cloud)
    step = commands.add_parser('run')
    step.add_argument('--prepared', type=Path, required=True)
    step.add_argument('--output', type=Path, required=True, help='new measurement directory')
    step.add_argument('--time', type=Path, required=True)
    step.add_argument('--case', action='append', help='measure only this kernel (repeatable); publish needs all')
    step.add_argument('--cloud', action='store_true', help=cloud)
    step.add_argument('--smoke', action='store_true', help='tooling smoke: one warmup per process; never publishable')
    step = commands.add_parser('compile')
    step.add_argument('--prepared', type=Path, required=True)
    step.add_argument('--output', type=Path, required=True, help='new measurement directory')
    step.add_argument('--time', type=Path, required=True)
    step.add_argument('--cloud', action='store_true', help=cloud)
    step = commands.add_parser('publish')
    step.add_argument('--prepared', type=Path, required=True)
    step.add_argument('--run', type=Path, required=True)
    step.add_argument('--compile', type=Path, required=True)
    step.add_argument('--output', type=Path, required=True)
    step = commands.add_parser('tables')
    step.add_argument('summary', type=Path)
    args = parser.parse_args(argv)
    return dict(prepare=prepare, run=run, compile=compile_timing, publish=publish, tables=tables)[args.command](args)


if __name__ == '__main__':
    main()
