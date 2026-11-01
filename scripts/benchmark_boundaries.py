#!/usr/bin/env python3
"""Separate bounded fresh-precompiled / existing-Realm diagnostic cohort.

Run under scripts/limited_verification.py on a local machine. Builds and source
emission happen before measurements. All child receipts/logs and exact P/R,
source, V8 cachedData and expected outputs remain in a new output directory.
"""
import argparse
from datetime import datetime, timezone
import json
import os
import platform
from pathlib import Path
import shutil
import statistics

from benchmark_metrics import digest, measure_process, summary
from boundary_corpus import GUEST_WARMUPS, supported_react_fixture, workloads
from boundary_process import measure_ready_process, validate_ready
from limited_verification import kernel_group, limited_environment, require_kernel_budget

SCHEMA = 'scriptw-v8-portable-boundaries-v1'
ROOT = Path(__file__).resolve().parents[1]
HARNESS = (
    'scripts/benchmark_boundaries.py', 'scripts/boundary_corpus.py', 'scripts/boundary_node.cjs',
    'scripts/boundary_process.py',
    'scripts/benchmark_metrics.py', 'scripts/limited_verification.py', 'scripts/resource_numbers.py',
    'crates/scriptw-cli/examples/runtime_boundaries.rs',
    'crates/scriptw-cli/examples/runtime_boundaries/call_observer.rs',
    'crates/scriptw-cli/examples/runtime_boundaries/result_ready.rs',
)

CACHE_DOMAINS = {'sha256': 'scriptw-program-cache-compiler-v4',
                 'blake3': 'scriptw-program-cache-compiler-v5-blake3'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_cache_identity(identity, requested, executable_sha256):
    require(isinstance(identity, dict) and requested in CACHE_DOMAINS,
            'missing or unknown compiler cache identity cohort')
    require(identity.get('image_hash_algorithm') == requested and
            identity.get('fingerprint_domain') == CACHE_DOMAINS[requested],
            'executing compiler cache algorithm/domain differs from requested cohort')
    digest_value = identity.get('image_digest')
    require(isinstance(digest_value, str) and len(digest_value) == 64 and
            all(c in '0123456789abcdef' for c in digest_value), 'invalid actual cache image digest')
    if requested == 'sha256':
        require(digest_value == executable_sha256, 'cache SHA differs from loaded executable evidence')


def validate_calls(calls, samples):
    """Validate the reported boundary against every raw callback timestamp."""
    require(isinstance(calls, dict), 'missing isolated-call observation')
    durations = calls.get('samples_ns')
    callbacks = calls.get('callbacks')
    require(isinstance(durations, list) and len(durations) == samples,
            'wrong isolated-call sample count')
    require(isinstance(callbacks, list) and len(callbacks) == 1 + 2 * samples,
            'incomplete call callback protocol')
    markers = ['scriptw:call:ready']
    for index in range(samples):
        markers.extend((f'scriptw:call:start:{index}', f'scriptw:call:end:{index}'))
    last_exit = 0
    for callback, marker in zip(callbacks, markers):
        require(callback.get('marker') == marker, 'call markers out of order')
        entry, exit = callback.get('entry_ns'), callback.get('exit_ns')
        require(type(entry) is int and type(exit) is int and last_exit <= entry <= exit,
                'invalid callback clock ordering')
        last_exit = exit
    for index, duration in enumerate(durations):
        require(type(duration) is int and duration >= 0 and duration ==
                callbacks[2 + 2 * index]['entry_ns'] - callbacks[1 + 2 * index]['exit_ns'],
                'call duration differs from raw callback boundaries')


def validate_child(row, *, runtime, mode, expected, source_sha, prepared, samples):
    require(row.get('mode') == mode, 'child executed another boundary')
    if runtime == 'scriptw':
        require(row.get('schema') == 'scriptw-portable-boundaries-child-v1', 'wrong Scriptw cohort')
        fields = ['source_fingerprint', 'source_commit', 'executable_sha256', 'artifact',
                  'compiler_cache_identity', 'requested_compiler_cache_image_hash']
        if mode != 'source':
            fields.extend(('bundle_sha256', 'bundle_bytes'))
        for field in fields:
            require(row.get(field) == prepared[field], f'Scriptw {field} changed after preparation')
        if mode == 'source':
            spans = row.get('compilation_ns')
            require(row.get('source_sha256') == source_sha and row.get('source_bytes') == prepared['source_bytes']
                    and isinstance(spans, dict) and set(spans) == {'preparation', 'lowering', 'emission', 'validation'}
                    and all(type(value) is int and value >= 0 for value in spans.values())
                    and 'bundle_sha256' not in row,
                    'source boundary did not report real source compilation')
        execution = row.get('execution', {})
        require(execution.get('backend') == 'wasm-aot' and execution.get('completion') == 'normal',
                'not normal real Wasm-AOT execution')
        require(execution.get('stdout') == expected, 'Scriptw output mismatch')
        initial, total = ('source_read_and_compilation_ns', 'source_to_result_ns') if mode == 'source' else (
            'bundle_read_and_admission_ns', 'bundle_load_and_execute_ns')
        require(row.get(initial, -1) >= 0 and row.get(total, -1) >= row[initial],
                'invalid bundle timing')
    else:
        require(row.get('schema') == 'scriptw-v8-boundaries-child-v1', 'wrong V8 cohort')
        if mode == 'source':
            require(row.get('cached_data_supplied') is False and row.get('cached_data_rejected') is None,
                    'V8 source boundary must construct a Script without cachedData')
        else:
            require(row.get('cached_data_rejected') is False, 'V8 cachedData rejected; source fallback forbidden')
        require(row.get('source_sha256') == source_sha and row.get('stdout') == expected,
                'V8 source/output mismatch')
        fields = ['node', 'v8', 'exec_argv']
        if mode != 'source': fields.extend(('cached_data_sha256', 'cached_data_bytes'))
        for field in fields:
            require(row.get(field) == prepared[field], f'V8 {field} changed after preparation')
        require(row.get('exec_argv') == [], 'Node must use ordinary flags')
    if mode == 'calls':
        require(row.get('guest_warmups') == GUEST_WARMUPS, 'wrong guest warmup contract')
        validate_calls(row.get('calls'), samples)


def stamp():
    return datetime.now(timezone.utc).isoformat()


def run(args):
    require_kernel_budget(4096 * 1024 ** 2)
    require(len(os.sched_getaffinity(0)) == 1, 'one inherited CPU is required')
    environment = limited_environment()
    cache_algorithm = environment.get('SCRIPTW_COMPILER_CACHE_IMAGE_HASH', 'sha256')
    require(cache_algorithm in CACHE_DOMAINS, 'unknown explicit compiler cache image hash cohort')
    environment['SCRIPTW_COMPILER_CACHE_IMAGE_HASH'] = cache_algorithm
    require(not environment.get('NODE_OPTIONS'), 'unset NODE_OPTIONS for ordinary Node flags')
    environment.pop('NODE_COMPILE_CACHE', None)
    environment['NODE_DISABLE_COMPILE_CACHE'] = '1'
    for name in ('SCRIPTW_WASM_TRACE', 'SCRIPTW_WASM_TRACE_DUMP', 'SCRIPTW_ROW_PERF_CONTROL',
                 'SCRIPTW_ROW_PERF_ACK', 'SCRIPTW_ROW_PERF_REGION',
                 'SCRIPTW_BOUNDARY_READY_FD', 'SCRIPTW_BOUNDARY_READY_NONCE'):
        require(not environment.get(name), f'unset unrelated instrumentation {name}')
    runner, node = args.runner.resolve(), args.node.resolve()
    require(runner.is_file() and node.is_file(), 'built runner and Node executable are required')
    time_binary = args.time or shutil.which('gtime') or shutil.which('time')
    require(time_binary is not None, 'GNU time is required for actual child RSS')
    time_binary = Path(time_binary).resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in HARNESS:
        destination = output / 'harness' / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, destination)
    node_helper = output / 'harness/scripts/boundary_node.cjs'
    environment['SCRIPTW_CACHE_DIR'] = str(output / 'native-cache')
    report = dict(schema=SCHEMA, cohort='portable-boundaries-' + cache_algorithm + '-v1',
        complete=False, started_at=stamp(), commands=[], cases=[],
        configuration=dict(samples=args.samples, fresh_child_warmups=1, guest_warmups=GUEST_WARMUPS,
            boundaries=args.boundaries,
            compiler_cache_image_hash=cache_algorithm, compiler_cache_fingerprint_domain=CACHE_DOMAINS[cache_algorithm],
            call_sessions=args.call_sessions, timeout_seconds=args.timeout,
            native_cache='private retained disk cache; first fresh child is recorded unmeasured warmup; cache effects retained in raw phases',
            node_flags=[], node_application_compile_cache='disabled; explicit vm.Script cachedData only in precompiled cohorts',
            fresh_scope='parent-before-Popen to nonce-bound checked-result pipe arrival; post-result full-SHA/artifact provenance remains in separate instrumented whole-child accounting; no subtraction',
            calls_scope='recorded callback-exit to next callback-entry wall spans in one Realm; ten guest warmups; no empty subtraction',
            comparability='new diagnostic cohort; do not merge ratios with ordinary baseline, policy or amortized-batching reports'),
        binaries=dict(scriptw=dict(path=str(runner), sha256=digest(runner)),
                      node=dict(path=str(node), sha256=digest(node)),
                      accounting=dict(path=str(time_binary), sha256=digest(time_binary))),
        harness_sha256={name: digest(output / 'harness' / name) for name in HARNESS},
        harness_snapshots={name: str(Path('harness') / name) for name in HARNESS},
        machine=dict(system=platform.system(), kernel=platform.release(), architecture=platform.machine(),
            logical_cpus=os.cpu_count(), cpu_model=next((line.split(':', 1)[1].strip()
                for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')), None)),
        resources=dict(cgroup=str(kernel_group()), affinity=sorted(os.sched_getaffinity(0)),
            memory_max=(kernel_group() / 'memory.max').read_text().strip(),
            memory_swap_max=(kernel_group() / 'memory.swap.max').read_text().strip(),
            memory_oom_group=(kernel_group() / 'memory.oom.group').read_text().strip()))

    def save():
        pending = output / 'receipt.pending'
        pending.write_text(json.dumps(report, indent=2) + '\n')
        pending.replace(output / 'receipt.json')

    def invoke(label, runtime, command, expected_ready_sha=None):
        binary = report['binaries'][runtime]
        require(digest(binary['path']) == binary['sha256'], 'runtime binary changed before invocation')
        if runtime == 'node':
            require(digest(node_helper) == report['harness_sha256']['scripts/boundary_node.cjs'],
                    'retained Node control changed before invocation')
        item = dict(label=label, runtime=runtime, command=list(map(str, command)),
                    cwd=str(ROOT), started_at=stamp(), state='running',
                    stdout=label + '.json', stderr=label + '.stderr')
        report['commands'].append(item)
        save()
        with (output / item['stdout']).open('xb') as stdout, (output / item['stderr']).open('xb') as stderr:
            measure = measure_process if expected_ready_sha is None else measure_ready_process
            accounting = measure(command, cwd=ROOT, env=environment, stdout=stdout,
                stderr=stderr, timeout=args.timeout, time_binary=time_binary)
        item.update(accounting=accounting, state='finished', finished_at=stamp())
        item['stdout_sha256'] = digest(output / item['stdout'])
        item['stderr_sha256'] = digest(output / item['stderr'])
        save()
        require(accounting['exit_code'] == 0 and not accounting.get('timed_out'),
                'child failed; retained logs: ' + label)
        require(digest(binary['path']) == binary['sha256'], 'runtime binary changed during invocation')
        row = json.loads((output / item['stdout']).read_text())
        if expected_ready_sha is not None:
            validate_ready(accounting, row, expected_ready_sha)
        return row, item

    try:
        inventory = workloads(args.samples)
        if 'react-ssr-synchronous' in args.cases:
            inventory.append(supported_react_fixture(ROOT, args.samples))
        by_name = {case['name']: case for case in inventory}
        require(len(set(args.cases)) == len(args.cases), 'duplicate case selection')
        require(set(args.cases) <= set(by_name), 'unknown boundary workload')
        for name in args.cases:
            fixture = by_name[name]
            case = dict(name=name, partition=fixture['partition'], boundaries={}, unavailable={})
            if 'source_evidence' in fixture:
                case['react_scope'] = {k: fixture[k] for k in
                    ('source_evidence', 'source_evidence_sha256', 'supported_scope')}
                case['react_scope']['upstream_sha256'] = fixture['upstream_sha256']
                case['react_scope']['restored_assets'] = {}
                for asset_name, content in fixture['restored_assets'].items():
                    asset_path = output / 'react-assets' / asset_name
                    asset_path.parent.mkdir(parents=True, exist_ok=True)
                    asset_path.write_text(content)
                    case['react_scope']['restored_assets'][asset_name] = dict(
                        path=str(asset_path.relative_to(output)), sha256=digest(asset_path), bytes=asset_path.stat().st_size)
            report['cases'].append(case)
            for boundary in args.boundaries:
                fixture_boundary = 'fresh' if boundary == 'source' else boundary
                source_text = fixture[fixture_boundary + '_source']
                if source_text is None:
                    continue
                prefix = name + '-' + boundary
                input_prefix = name + ('-startup' if boundary in ('source', 'fresh') else '-calls')
                source, expected_file = output / (input_prefix + '.js'), output / (input_prefix + '.expected')
                bundle, cache = output / (prefix + '.swbundle'), output / (prefix + '.v8cache')
                expected = fixture[fixture_boundary + '_stdout']
                for path, content in ((source, source_text), (expected_file, expected)):
                    if path.exists(): require(path.read_text() == content, 'shared startup input differs between boundaries')
                    else:
                        with path.open('x') as stream: stream.write(content)
                source_sha = digest(source)
                retained = dict(source=source.name, source_sha256=source_sha,
                    source_bytes=source.stat().st_size, expected_stdout=expected,
                    expected_file=expected_file.name, expected_sha256=digest(expected_file),
                    bundle=bundle.name, cached_data=None if boundary == 'source' else cache.name, scriptw=[], node=[])
                case['boundaries'][boundary] = retained
                save()
                sw_prepared, _ = invoke(prefix + '-prepare-scriptw', 'scriptw',
                    [runner, 'prepare', source, bundle])
                require(sw_prepared.get('schema') == 'scriptw-portable-boundaries-child-v1' and
                        sw_prepared.get('source_sha256') == source_sha and
                        sw_prepared.get('bundle_sha256') == digest(bundle) and
                        sw_prepared.get('executable_sha256') == report['binaries']['scriptw']['sha256'],
                        'Scriptw preparation identity or artifact mismatch')
                validate_cache_identity(sw_prepared.get('compiler_cache_identity'), cache_algorithm,
                                        sw_prepared['executable_sha256'])
                require(sw_prepared.get('requested_compiler_cache_image_hash') == cache_algorithm,
                        'child requested cache hash differs from declared cohort')
                node_command = [node, node_helper, 'source', source, expected_file] if boundary == 'source' else [
                    node, node_helper, 'prepare', source, cache, expected_file]
                node_prepared, _ = invoke(prefix + '-prepare-node', 'node', node_command)
                require(node_prepared.get('schema') == 'scriptw-v8-boundaries-child-v1' and
                        node_prepared.get('source_sha256') == source_sha and
                        node_prepared.get('stdout') == expected and
                        node_prepared.get('exec_argv') == [], 'V8 preparation mismatch')
                if boundary == 'source':
                    require(node_prepared.get('cached_data_supplied') is False and
                            node_prepared.get('cached_data_rejected') is None, 'V8 source control supplied cachedData')
                else:
                    require(node_prepared.get('cached_data_sha256') == digest(cache), 'V8 cachedData artifact mismatch')
                retained['preparation'] = dict(scriptw=sw_prepared, node=node_prepared)
                retained['bundle_sha256'] = digest(bundle)
                retained['cached_data_sha256'] = None if boundary == 'source' else digest(cache)
                # A single retained native-cache warmup belongs to fresh child
                # startup. Calls warm inside each new Realm, in their JS fixture.
                count = args.samples + 1 if boundary in ('source', 'fresh') else args.call_sessions
                for index in range(count):
                    order = ('scriptw', 'node') if index % 2 == 0 else ('node', 'scriptw')
                    for runtime in order:
                        require(digest(source) == source_sha and digest(bundle) == retained['bundle_sha256']
                                and (boundary == 'source' or digest(cache) == retained['cached_data_sha256']), 'retained input changed')
                        if boundary == 'source':
                            command = [runner, 'source', source, expected_file] if runtime == 'scriptw' else [
                                node, node_helper, 'source', source, expected_file]
                        else:
                            command = [runner, boundary, bundle, expected_file] if runtime == 'scriptw' else [
                                node, node_helper, boundary, source, cache, expected_file]
                        if boundary == 'calls':
                            command.append(str(args.samples))
                        row, command_receipt = invoke(f'{prefix}-{index:02}-{runtime}', runtime, command,
                            expected_ready_sha=retained['expected_sha256'])
                        validate_child(row, runtime=runtime, mode=boundary, expected=expected,
                            source_sha=source_sha, prepared=sw_prepared if runtime == 'scriptw' else node_prepared,
                            samples=args.samples)
                        if runtime == 'scriptw':
                            policy = {field: row['execution'][field] for field in
                                      ('runtime_policy', 'actual_native_policy')}
                            require(retained.get('actual_runtime_policy', policy) == policy,
                                    'executing native/GC policy changed inside boundary cohort')
                            retained['actual_runtime_policy'] = policy
                        retained[runtime].append(dict(warmup=boundary in ('source', 'fresh') and index == 0,
                            command=command_receipt['label'], accounting=command_receipt['accounting'], raw=row))
                        save()
                for runtime in ('scriptw', 'node'):
                    rows = [r for r in retained[runtime] if not r['warmup']]
                    if boundary in ('source', 'fresh'):
                        metric = 'source_to_result_ns' if boundary == 'source' else 'load_and_execute_ns'
                        raw_field = ('source_to_result_ns' if boundary == 'source' else 'bundle_load_and_execute_ns') if runtime == 'scriptw' else 'load_and_execute_ns'
                        retained[runtime + '_summary'] = dict(
                            spawn_to_checked_result_ready_ns=summary([r['accounting']['result_ready_elapsed_ns'] for r in rows]),
                            child_wall_ns=summary([r['accounting']['elapsed_ns'] for r in rows]))
                        retained[runtime + '_summary'][metric] = summary([r['raw'][raw_field] for r in rows])
                    else:
                        # Preserve each Realm as a cluster. Pooling these
                        # samples would conceal between-process variation.
                        retained[runtime + '_sessions'] = [summary(r['raw']['calls']['samples_ns']) for r in rows]
                if boundary in ('source', 'fresh'):
                    retained['ratio_scriptw_over_v8'] = {metric:
                        retained['scriptw_summary'][metric]['median'] / retained['node_summary'][metric]['median']
                        for metric in ('spawn_to_checked_result_ready_ns', 'child_wall_ns',
                                       'source_to_result_ns' if boundary == 'source' else 'load_and_execute_ns')}
                else:
                    ratios = [sw['median'] / v8['median'] for sw, v8 in zip(
                        retained['scriptw_sessions'], retained['node_sessions'])]
                    retained['ratio_scriptw_over_v8'] = dict(paired_session_medians=ratios,
                        median_of_session_ratios=statistics.median(ratios),
                        scope='Each ratio compares medians of one separately initialized Realm per runtime; all raw guest-call samples retained. Two sessions do not establish statistical confidence.')
                save()
        report['complete'] = True
        report['finished_at'] = stamp()
    except BaseException as error:
        report['failure'] = str(error)
        raise
    finally:
        save()
    return output / 'receipt.json'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--node', type=Path, default=Path(shutil.which('node') or '/missing-node'))
    parser.add_argument('--time', type=Path)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--case', action='append', dest='cases')
    parser.add_argument('--boundary', choices=('source', 'fresh', 'calls'), action='append', dest='boundaries')
    parser.add_argument('--samples', type=int, choices=range(7, 101), default=7)
    parser.add_argument('--call-sessions', type=int, choices=range(1, 6), default=2)
    parser.add_argument('--timeout', type=int, choices=range(1, 3601), default=180)
    args = parser.parse_args()
    args.cases = args.cases or ['calls', 'arrays', 'json']
    args.boundaries = args.boundaries or ['source', 'fresh', 'calls']
    require(len(set(args.boundaries)) == len(args.boundaries), 'duplicate boundary selection')
    print(run(args))


if __name__ == '__main__':
    main()
