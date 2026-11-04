#!/usr/bin/env python3
"""Measure current native policies and normal V8 on retained-module programs.

Run through limited_verification.py. Native preparation precedes all consumers;
each consumer initializes one module and checks every prepared-input invocation.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import sys

from benchmark_metrics import digest, measure_process
from limited_verification import kernel_group, limited_environment, require_kernel_budget

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = 'scriptw-situation-benchmarks-v1'
SAMPLES, WARMUPS, BLOCKS, ROUNDS = 7, 3, 2, 4
ADAPTER = 'crates/scriptw-cli/tests/fixtures/v8_baseline/node-situations.cjs'
HOST_SOURCE = 'crates/scriptw-cli/examples/compiler_precompiled_baseline.rs'
IDENTITY_FIELDS = ('source_commit', 'source_fingerprint', 'compiler_identity_scheme', 'executable_sha256')
POLICIES = {'fast': ('None', 'SinglePass', 'single-pass'),
            'speed': ('Speed', 'Backtracking', 'backtracking')}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def integer(value, *, positive=False):
    require(type(value) is int and value >= int(positive), 'expected a nonnegative integer')
    return value


def sha(data):
    return hashlib.sha256(data).hexdigest()


def stamp():
    return datetime.now(timezone.utc).isoformat()


def member(root, name):
    require(isinstance(name, str) and Path(name).name == name and '\\' not in name
            and name not in ('', '.', '..'), 'retained file must be a basename')
    path = root / name
    require(path.resolve().is_relative_to(root.resolve()), 'retained file escapes its directory')
    return path


def validate_manifest(plan):
    require(plan.get('schema') == 'scriptw-retained-situations-v1'
            and plan.get('source_format') == 'module', 'standalone module manifest required')
    cases = plan.get('execution')
    require(isinstance(cases, list) and 1 <= len(cases) <= 128, 'invalid situation inventory')
    names = set()
    for case in cases:
        name = case.get('file')
        member(Path('.'), name)
        require(name.endswith('.js') and name not in names, 'duplicate or non-JavaScript situation')
        names.add(name)
        for field in ('label', 'category', 'scope'):
            require(isinstance(case.get(field), str) and case[field].strip(), 'missing ' + field)
        integer(case.get('operations'), positive=True)
        invocations = case.get('invocations')
        require(isinstance(invocations, list) and len(invocations) == 4, 'four checked inputs required')
        for seed, row in zip((3, 5, 7, 11), invocations, strict=True):
            require(set(row) == {'input', 'stdout'} and type(row['input']) is int
                    and row['input'] == seed and isinstance(row['stdout'], str)
                    and re.fullmatch(r'-?\d+\n', row['stdout']), 'invalid checked input/checksum')
        require(case.get('stdout') == invocations[0]['stdout'], 'first checksum differs')
        for field in ('source_sites', 'frame_entries'):
            if field in case:
                integer(case[field], positive=True)
    return cases


def validate_cargo(rows, host, frozen_receipt=None):
    candidates = [row for row in rows if row.get('reason') == 'compiler-artifact'
                  and row.get('target', {}).get('name') == 'compiler_precompiled_baseline'
                  and row.get('executable')]
    require(len(candidates) == 1, 'one actual Cargo example producer required')
    row = candidates[0]
    target, profile = row['target'], row['profile']
    require(target['kind'] == ['example'] and target['crate_types'] == ['bin']
            and Path(target['src_path']).resolve() == (ROOT / HOST_SOURCE).resolve(),
            'wrong Cargo example source/target')
    package = row['package_id']
    require(package.startswith('path+file:') and
            ('/scriptw-cli#' in package or '#scriptw-cli@' in package), 'wrong Cargo package')
    require(row['features'] == [] and profile['opt_level'] == '3'
            and profile['debug_assertions'] is False and profile['test'] is False
            and profile['overflow_checks'] is False, 'release opt3/no-feature producer required')
    if frozen_receipt is None:
        producer = Path(row['executable']).resolve()
        require(producer.is_file() and digest(producer) == digest(host),
                'Cargo producer executable must match the frozen host bytes')
    else:
        require(isinstance(frozen_receipt, dict) and frozen_receipt.get('artifact') == row
                and frozen_receipt.get('sha256') == digest(host)
                and isinstance(frozen_receipt.get('source_manifest_sha256'), str)
                and re.fullmatch(r'[0-9a-f]{64}', frozen_receipt['source_manifest_sha256']),
                'frozen receipt must bind the exact Cargo artifact, source manifest and host bytes')
    return row


def validate_identity(row, host_sha):
    require(row.get('schema') == 'scriptw-precompiled-shared-host-v1'
            and row.get('executable_sha256') == host_sha, 'frozen host identity differs')
    require(all(isinstance(row.get(field), str) and row[field] for field in IDENTITY_FIELDS),
            'missing compiled source identity')
    shared = row.get('shared_configuration', {})
    require(shared.get('consumer_compiler_enabled') is False
            and shared.get('runtime_policy') == 'scriptw-required-wasmtime-experimental-v1'
            and shared.get('collector') == 'copying' and shared.get('gc_initial_bytes') == 16777216
            and shared.get('wasmtime_version') == '47.0.0' and shared.get('target') == 'generic',
            'unexpected required experimental runtime baseline')


def validate_native(row, identity, policy, *, producer):
    schema = 'scriptw-retained-call-execution-v2'
    if producer:
        schema = 'scriptw-retained-call-producer-v2'
    require(row.get('schema') == schema and row.get('source_format') == 'module'
            and row.get('native_policy') == policy and row.get('compiler_enabled') is producer
            and row.get('excluded_from_ordinary_cohorts') is True
            and row.get('excluded_from_scored_cohorts') is True, 'native cohort declaration differs')
    require(all(row.get(field) == identity[field] for field in IDENTITY_FIELDS),
            'native producer/consumer source or executable identity differs')
    configuration = dict(identity['shared_configuration'])
    configuration.update(native_compilation_mode=policy, register_allocation=POLICIES[policy][2])
    require(row.get('shared_configuration') == configuration, 'native shared configuration differs')
    native = row.get('native_configuration', {})
    require(native.get('parallel_compilation') is False, 'native jobs differ')
    if producer:
        require(native.get('target') == configuration['target_triple']
                and native.get('cranelift_opt_level') == POLICIES[policy][0]
                and native.get('register_allocation') == POLICIES[policy][1],
                'actual native compiler policy differs')
    else:
        require(native.get('cranelift_opt_level') is None
                and native.get('register_allocation') is None
                and native.get('target') is None, 'consumer compiler must be disabled')


def validate_preparation(raw, cases, directory, sources, identity, policy):
    validate_native(raw, identity, policy, producer=True)
    require(raw.get('manifest_sha256') == digest(sources / 'manifest.json'), 'producer manifest differs')
    receipts = raw.get('cases', [])
    require([row.get('file') for row in receipts] == [case['file'] for case in cases],
            'producer case inventory differs')
    for row, case in zip(receipts, cases, strict=True):
        validate_native(row, identity, policy, producer=True)
        require(all(row.get(field) == case[field] for field in
                    ('file', 'stdout', 'label', 'category', 'scope', 'operations', 'invocations')),
                'producer workload metadata differs')
        original = sources / case['file']
        for field in ('original_source_file', 'derived_source_file'):
            require(member(directory, row[field]).read_bytes() == original.read_bytes(),
                    'native source was rewritten')
        require(row['original_source_sha256'] == digest(original)
                and row['derived_source_sha256'] == digest(original), 'native source digest differs')
        image = member(directory, row['image'])
        wasm = member(directory, case['file'] + '.retained.wasm')
        require(digest(image) == row['image_sha256'] and image.stat().st_size == row['image_bytes']
                and digest(wasm) == row['program_wasm_sha256']
                and wasm.stat().st_size == row['program_wasm_bytes'], 'native image/raw P differs')
        for field in ('image_bytes', 'program_wasm_bytes', 'native_serialized_bytes'):
            integer(row[field], positive=True)
        require(row['native_serialized_bytes'] < row['image_bytes'], 'native serialized size exceeds image')
        require(set(row['phases_ns']) == {'source_compilation', 'native_preparation'}, 'wrong producer phases')
        for value in row['phases_ns'].values():
            integer(value)
        require(json.loads(image.with_suffix('.json').read_text()) == row, 'image sidecar receipt differs')
    require(json.loads((directory / 'producer.json').read_text()) == raw, 'producer receipt differs')
    return receipts


def validate_execution(raw, case, actor, source_sha, identity, prepared=None):
    require(raw.get('samples') == SAMPLES and raw.get('warmups') == WARMUPS,
            'consumer sample plan differs')
    if actor == 'v8':
        require(raw.get('schema') == 'scriptw-v8-retained-situations-v1'
                and raw.get('file') == case['file'] and raw.get('source_sha256') == source_sha
                and raw.get('node') == 'v24.12.0' and isinstance(raw.get('v8'), str)
                and raw.get('native_syntax_absent') is True
                and raw.get('execution_policy') == {'jit': 'normal', 'gc': 'natural'},
                'V8 source/version/runtime policy differs')
        phases = raw.get('phase_ns', {})
        require(set(phases) == {'parse', 'link', 'init'}, 'wrong V8 phases')
    else:
        validate_native(raw, identity, actor.removeprefix('scriptw-'), producer=False)
        require(raw.get('producer') == prepared and raw.get('initialization_count') == 1,
                'consumer image/producer or initialization count differs')
        phases = raw.get('phases_ns', {})
        require(set(phases) == {'native_admission', 'setup', 'module_initialization'}, 'wrong native phases')
    for value in phases.values():
        integer(value)
    observations = raw.get('observations', [])
    require(len(observations) == SAMPLES + WARMUPS, 'consumer invocation count differs')
    for index, row in enumerate(observations):
        planned = case['invocations'][index % 4]
        require(type(row.get('index')) is int and row['index'] == index
                and type(row.get('invocation_index')) is int and row['invocation_index'] == index % 4
                and type(row.get('input')) is int and row['input'] == planned['input']
                and row.get('warmup') is (index < WARMUPS)
                and row.get('stdout') == planned['stdout'] and row.get('matched') is True,
                'invocation ordering/checksum/warmup differs')
        integer(row.get('call_ns'), positive=True)
        integer(row.get('prepare_ns'))
        if actor != 'v8':
            require(row.get('completion') == 'normal'
                    and row.get('stdout_sha256') == sha(planned['stdout'].encode()),
                    'native completion/checksum digest differs')
    return [row['call_ns'] for row in observations if not row['warmup']]


def measurement_plan(cases, actors):
    for block in range(BLOCKS):
        for case_index, case in enumerate(cases):
            for round_index in range(ROUNDS):
                start = (case_index + round_index) % len(actors)
                order = actors[start:] + actors[:start]
                if block == 1:
                    order = list(reversed(order))
                for position, actor in enumerate(order):
                    yield dict(file=case['file'], actor=actor, block=block,
                               round=round_index, position=position)


def reduce_runtime(rows, operations):
    """Each fresh process is a cluster; reduce process medians, never pooled calls."""
    integer(operations, positive=True)
    require(len(rows) == BLOCKS * ROUNDS and all(row.get('status') == 'ok' for row in rows),
            'cannot summarize incomplete/failed process inventory')
    require({(row['block'], row['round']) for row in rows} ==
            {(block, round_index) for block in range(BLOCKS) for round_index in range(ROUNDS)},
            'duplicate or missing process block/round')
    medians = []
    for row in rows:
        samples = row['samples_ns']
        require(len(samples) == SAMPLES, 'incomplete process samples')
        for value in samples:
            integer(value, positive=True)
        require(row['median_ns'] == statistics.median(samples), 'recorded process median differs')
        integer(row['accounting']['peak_rss_bytes'], positive=True)
        medians.append(row['median_ns'])
    peak = [row['accounting']['peak_rss_bytes'] for row in rows]
    middle = statistics.median(medians)
    return dict(status='complete', processes=[{key: row[key] for key in
        ('block', 'round', 'command', 'median_ns', 'samples_ns', 'accounting')} for row in rows],
        median_ns=middle, ms=middle / 1e6, process_medians_ns=medians,
        block_medians_ns=[statistics.median([row['median_ns'] for row in rows if row['block'] == block])
                          for block in range(BLOCKS)],
        range_ns=[min(medians), max(medians)], ns_per_operation=middle / operations,
        peak_rss_bytes=dict(median=statistics.median(peak), minimum=min(peak), maximum=max(peak)))


def run(args):
    require_kernel_budget(4 * 1024**3)
    require(len(os.sched_getaffinity(0)) == 1, 'one inherited CPU required')
    environment = limited_environment()
    require(not environment.get('NODE_OPTIONS'), 'unset NODE_OPTIONS')
    for key, value in environment.items():
        if key.startswith(('SCRIPTW_PRECOMPILED_PHASE_', 'SCRIPTW_ROW_PERF_', 'SCRIPTW_BOUNDARY_READY_')):
            require(not value, 'unset unrelated instrumentation ' + key)
        if key in ('SCRIPTW_PRECOMPILED_EXECUTION_WINDOWS', 'SCRIPTW_WASM_TRACE',
                   'SCRIPTW_WASM_TRACE_DUMP', 'SCRIPTW_PERF_MAP_DIR', 'SCRIPTW_EMIT_SIZE_REPORT'):
            require(not value, 'unset unrelated instrumentation ' + key)
    environment.pop('NODE_COMPILE_CACHE', None)
    environment['NODE_DISABLE_COMPILE_CACHE'] = '1'
    environment['SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES'] = '1'
    require(args.policies and len(set(args.policies)) == len(args.policies)
            and set(args.policies) <= set(POLICIES), 'select distinct fast/speed policies')
    fixture_root = args.fixtures.resolve()
    manifest_bytes = (fixture_root / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_bytes)
    inventory = validate_manifest(manifest)
    selected = args.cases or [case['file'] for case in inventory]
    require(len(selected) == len(set(selected)) and set(selected) <= {case['file'] for case in inventory},
            'unknown or duplicate case selection')
    cases = [case for case in inventory if case['file'] in selected]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('bin', 'sources', 'tooling', 'logs', 'build', 'images', 'manifests', 'scratch'):
        (output / name).mkdir()
    host, node, time_binary = [Path(shutil.which(str(path)) or path).resolve()
                              for path in (args.host, args.node, args.time)]
    shutil.copy2(host, output / 'bin/compiler_precompiled_baseline')
    host = output / 'bin/compiler_precompiled_baseline'
    shutil.copyfile(args.producer_log, output / 'build/cargo.jsonl')
    cargo = [json.loads(line) for line in (output / 'build/cargo.jsonl').read_text().splitlines() if line.strip()]
    receipt_path = None
    frozen_receipt = None
    if args.producer_receipt is not None:
        receipt_path = output / 'build/host-producer-receipt.json'
        shutil.copyfile(args.producer_receipt, receipt_path)
        frozen_receipt = json.loads(receipt_path.read_text())
    producer = validate_cargo(cargo, host, frozen_receipt)
    tooling = ['scripts/benchmark-situations.py', 'scripts/benchmark_metrics.py',
               'scripts/limited_verification.py', 'scripts/performance_situations.py', ADAPTER, HOST_SOURCE]
    for directory in ('runtime_precompiled', 'compiler_precompiled_baseline'):
        tooling.extend(str(path.relative_to(ROOT)) for path in
                       sorted((ROOT / 'crates/scriptw-cli/examples' / directory).rglob('*.rs')))
    for name in tooling:
        destination = output / 'tooling' / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, destination)
    (output / 'sources/manifest.json').write_bytes(manifest_bytes)
    inputs = [dict(original=str(fixture_root / 'manifest.json'), path='sources/manifest.json',
                   sha256=sha(manifest_bytes), bytes=len(manifest_bytes))]
    # Retain the complete generated fixture inventory even for a selected run.
    for case in inventory:
        source = member(fixture_root, case['file'])
        data = source.read_bytes()
        require(not re.search(rb'%[A-Za-z_$][\w$]*\s*\(', data), 'native intrinsic syntax forbidden')
        (output / 'sources' / case['file']).write_bytes(data)
        inputs.append(dict(original=str(source), path='sources/' + case['file'],
                           sha256=sha(data), bytes=len(data)))
    workloads = [dict(case, source_sha256=digest(output / 'sources' / case['file'])) for case in cases]
    actors = ['scriptw-' + policy for policy in args.policies] + ['v8']
    group = kernel_group()
    report = dict(schema=SCHEMA, status='incomplete', complete=False, started_at=stamp(),
        workloads=workloads, cases=[], rows=[], preparations=[], commands=[], inputs=inputs,
        tools=dict(host=dict(path='bin/compiler_precompiled_baseline', sha256=digest(host), cargo_producer=producer),
                   node=dict(path=str(node), sha256=digest(node)),
                   time=dict(path=str(time_binary), sha256=digest(time_binary))),
        source_identity=None,
        policy=dict(actors=actors, native_policies=args.policies, samples=SAMPLES, warmups=WARMUPS,
            blocks=BLOCKS, processes_per_actor_per_block=ROUNDS,
            comparison='current configuration cohorts; no before/after compiler claim',
            execution='one initialized module/Realm per process; untimed prepare(input); timed public run(); checked output on every invocation; natural GC',
            reduction='median of eight fresh-process medians; two block medians and process range; no pooled call samples',
            memory='whole runtime-child peak RSS, not live JavaScript heap; preparation RSS separate',
            scriptc=dict(status='unavailable', reason='Scriptc WASI _start has no compatible retained-module prepare/run ABI'),
            timeout_seconds=60, preparation_timeout_seconds=300,
            caveats='descriptive same-host observations; not conformance, statistical confidence or universal application performance'),
        resources=dict(cgroup=str(group), affinity=sorted(os.sched_getaffinity(0)),
            memory_max=(group / 'memory.max').read_text().strip(),
            memory_swap_max=(group / 'memory.swap.max').read_text().strip(),
            memory_oom_group=(group / 'memory.oom.group').read_text().strip(),
            environment={key: environment.get(key) for key in ('CARGO_BUILD_JOBS', 'SCRIPTW_JOBS',
                'RUST_TEST_THREADS', 'RAYON_NUM_THREADS', 'SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES',
                'SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES', 'NODE_OPTIONS', 'NODE_DISABLE_COMPILE_CACHE')}),
        machine=dict(system=platform.system(), kernel=platform.release(), architecture=platform.machine()),
        tooling=[dict(original=str(ROOT / name), path='tooling/' + name,
                      sha256=digest(output / 'tooling' / name)) for name in tooling])
    if receipt_path is not None:
        report['tools']['host']['frozen_producer_receipt'] = dict(
            original=str(args.producer_receipt), path='build/host-producer-receipt.json',
            sha256=digest(receipt_path))
    prepared = {}

    def checkpoint():
        report['cases'] = []
        for case in cases:
            runtimes = {}
            for actor in actors:
                rows = [row for row in report['rows'] if row['file'] == case['file'] and row['actor'] == actor]
                runtimes[actor] = dict(status='incomplete', processes=len(rows))
                if any(row['status'] not in ('ok', 'running') for row in rows):
                    runtimes[actor] = dict(status='failed', processes=len(rows))
                elif len(rows) == BLOCKS * ROUNDS and all(row['status'] == 'ok' for row in rows):
                    runtimes[actor] = reduce_runtime(rows, case['operations'])
            report['cases'].append(dict(case, runtimes=runtimes))
        pending = output / 'report.pending'
        pending.write_text(json.dumps(report, indent=2) + '\n')
        pending.replace(output / 'report.json')

    def unchanged():
        for item in inputs:
            require(digest(item['original']) == item['sha256'] and digest(output / item['path']) == item['sha256'],
                    'original or retained fixture changed: ' + item['path'])
        for name, path in (('host', host), ('node', node), ('time', time_binary)):
            require(digest(path) == report['tools'][name]['sha256'], 'tool changed: ' + name)
        for item in report['tooling']:
            require(digest(item['original']) == item['sha256']
                    and digest(output / item['path']) == item['sha256'], 'original or retained tooling changed')

    def invoke(label, actor, stage, argv, timeout):
        event = dict(id=label, actor=actor, stage=stage, argv=list(map(str, argv)), cwd=str(ROOT),
                     stdout='logs/' + label + '.stdout', stderr='logs/' + label + '.stderr',
                     status='running', started_at=stamp())
        report['commands'].append(event)
        checkpoint()
        try:
            unchanged()
            scratch = output / 'scratch' / label
            scratch.mkdir()
            child_env = dict(environment, TMPDIR=str(scratch), SCRIPTW_CACHE_DIR=str(scratch / 'cache'))
            with (output / event['stdout']).open('xb') as stdout, (output / event['stderr']).open('xb') as stderr:
                event['accounting'] = measure_process(argv, cwd=ROOT, env=child_env, stdout=stdout,
                    stderr=stderr, timeout=timeout, time_binary=time_binary)
            for stream in ('stdout', 'stderr'):
                event[stream + '_sha256'] = digest(output / event[stream])
            require(event['accounting']['exit_code'] == 0 and not event['accounting']['timed_out'],
                    'child failed or timed out; see retained logs')
            require(not (output / event['stderr']).read_bytes(), 'child wrote stderr; see retained log')
            unchanged()
            event['status'] = 'ok'
        except (ValueError, OSError) as error:
            event.update(status='failed', error=str(error))
        event['finished_at'] = stamp()
        checkpoint()
        return event

    def value(event):
        require(event['status'] == 'ok', event.get('error', 'command failed'))
        return json.loads((output / event['stdout']).read_text())

    def prepare(policy, selected_cases, suffix):
        actor, label = 'scriptw-' + policy, 'prepare-' + policy + suffix
        sources = output / 'manifests' / label
        sources.mkdir()
        (sources / 'manifest.json').write_text(json.dumps(dict(manifest, execution=selected_cases)) + '\n')
        for case in selected_cases:
            shutil.copyfile(output / 'sources' / case['file'], sources / case['file'])
        directory = output / 'images' / label
        item = dict(actor=actor, stage='prepare', status='running', command=label, cases=[],
                    selected=[case['file'] for case in selected_cases])
        report['preparations'].append(item)
        event = invoke(label, actor, 'prepare', [host, 'prepare-retained', sources, directory, policy], 300)
        try:
            raw = value(event)
            receipts = validate_preparation(raw, selected_cases, directory, sources, identity, policy)
            for receipt in receipts:
                name = receipt['file']
                image = member(directory, receipt['image'])
                prepared[actor, name] = dict(receipt=receipt, path=image,
                    files={str(path): digest(path) for path in directory.iterdir() if path.is_file()})
                item['cases'].append(dict(receipt, image_path=str(image.relative_to(output)),
                    wasm_path=str((directory / (name + '.retained.wasm')).relative_to(output))))
            item['status'] = 'ok'
        except (ValueError, KeyError, TypeError, OSError) as error:
            item.update(status='failed', error=str(error))
        checkpoint()
        return item['status'] == 'ok'

    checkpoint()
    try:
        event = invoke('host-identity', 'host', 'identity', [host, 'identity'], 60)
        identity = value(event)
        validate_identity(identity, digest(host))
        report['source_identity'] = {field: identity[field] for field in IDENTITY_FIELDS}
        report['tools']['host']['identity'] = identity
        event = invoke('node-version', 'v8', 'version', [node, '--version'], 60)
        require(event['status'] == 'ok' and (output / event['stdout']).read_text().strip() == 'v24.12.0',
                'pinned Node v24.12.0 required')
        report['tools']['node']['version'] = 'v24.12.0'
        event = invoke('time-version', 'time', 'version', [time_binary, '--version'], 60)
        require(event['status'] == 'ok' and 'GNU' in (output / event['stdout']).read_text(), 'GNU time required')
        for policy in args.policies:
            if not prepare(policy, cases, '') and len(cases) > 1:
                # A producer stops at its first compiler failure. Recover the
                # independent cases without turning that original failure green.
                for index, case in enumerate(cases):
                    prepare(policy, [case], '-case-' + str(index))
        for task in measurement_plan(cases, actors):
            case = next(case for case in cases if case['file'] == task['file'])
            label = f"block-{task['block']}-round-{task['round']}-{task['actor']}-{task['file']}"
            row = dict(task, stage='execute', command=label, status='running',
                preparation_commands=[item['command'] for item in report['preparations']
                    if item['actor'] == task['actor'] and task['file'] in item['selected']])
            report['rows'].append(row)
            try:
                if task['actor'] == 'v8':
                    argv = [node, '--experimental-vm-modules', '--no-warnings', output / 'tooling' / ADAPTER,
                            output / 'sources', task['file'], str(SAMPLES), str(WARMUPS)]
                    native = None
                else:
                    native = prepared.get((task['actor'], task['file']))
                    require(native is not None, 'native preparation failed; no image admitted')
                    require(all(digest(path) == hashed for path, hashed in native['files'].items()),
                            'native image/receipt/source changed')
                    argv = [host, 'execute-retained', native['path'], case['stdout'], str(SAMPLES), str(WARMUPS)]
                event = invoke(label, task['actor'], 'execute', argv, 60)
                row['accounting'] = event.get('accounting')
                raw = value(event)
                row['raw'] = raw
                receipt = None
                if native is not None:
                    receipt = native['receipt']
                samples = validate_execution(raw, case, task['actor'], digest(output / 'sources' / task['file']),
                                             identity, receipt)
                if task['actor'] == 'v8':
                    require(report['tools']['node'].get('v8', raw['v8']) == raw['v8'], 'V8 version changed')
                    report['tools']['node']['v8'] = raw['v8']
                row.update(status='ok', samples_ns=samples, median_ns=statistics.median(samples))
            except (ValueError, KeyError, TypeError, OSError) as error:
                row.update(status='failed', error=str(error))
            checkpoint()
        unchanged()
        require(all(digest(path) == hashed for native in prepared.values()
                    for path, hashed in native['files'].items()), 'prepared files changed during campaign')
        report['complete'] = len(report['rows']) == len(cases) * len(actors) * BLOCKS * ROUNDS and all(
            row['status'] == 'ok' for row in report['rows']) and all(
            preparation['status'] == 'ok' for preparation in report['preparations'])
        report['status'] = 'complete'
        if not report['complete']:
            report['status'] = 'failed'
    except (ValueError, KeyError, TypeError, OSError) as error:
        report.update(status='failed', failure=str(error))
    finally:
        report['finished_at'] = stamp()
        report['retained_files'] = [dict(path=str(path.relative_to(output)), bytes=path.stat().st_size, sha256=digest(path))
            for directory in ('sources', 'tooling', 'build', 'logs', 'images', 'manifests')
            for path in sorted((output / directory).rglob('*')) if path.is_file()]
        checkpoint()
    print(output / 'report.json', flush=True)
    return report['complete']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', type=Path, required=True)
    parser.add_argument('--producer-log', type=Path, required=True)
    parser.add_argument('--producer-receipt', type=Path,
                        help='frozen artifact/source/hash receipt when Cargo output has been rebuilt')
    parser.add_argument('--fixtures', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--node', type=Path, required=True)
    parser.add_argument('--time', type=Path, required=True)
    parser.add_argument('--case', dest='cases', action='append')
    parser.add_argument('--policies', type=lambda text: text.split(','), default=['fast', 'speed'])
    args = parser.parse_args()
    if not run(args):
        raise SystemExit(1)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError) as error:
        sys.exit('Situation benchmark refused: ' + str(error))
