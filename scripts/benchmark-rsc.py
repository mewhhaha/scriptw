#!/usr/bin/env python3
"""Same-source retained Flight campaign; run through limited_verification.py.

Each case gets its own process. Reversed blocks balance actor/case ordering;
request summaries reduce fresh-process medians, never a pool of dependent calls.
No compilation, installation, publication or baseline discovery is implicit.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics

from benchmark_metrics import measure_process
from limited_verification import kernel_group, limited_environment, require_kernel_budget

ROOT = Path(__file__).resolve().parents[1]
DRIVER = 'crates/scriptw-cli/tests/fixtures/edge_rendering/react-driver.cjs'
HOST_SOURCE = 'crates/scriptw-cli/examples/rsc_baseline.rs'
CASES = ('rsc-sync', 'rsc-async')
NATIVE_MODES = ('fast', 'speed', 'fast-backtracking', 'speed-single-pass')
WRAPPER = '\nexport const run = globalThis.applicationRun;\nexport function encode(value) { return JSON.stringify(value); }\n'
FIELDS = ('memory.max', 'memory.swap.max', 'memory.oom.group', 'cpu.max', 'memory.events')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(value):
    return hashlib.sha256(value).hexdigest()


def digest(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def stats(values):
    require(bool(values), 'empty timing series')
    require(all(type(value) in (int, float) and math.isfinite(value) and value >= 0 for value in values),
            'finite nonnegative timing series required')
    ordered = sorted(values)
    return dict(samples=values, mean=statistics.mean(values), median=statistics.median(values),
                minimum=ordered[0], maximum=ordered[-1], p95=ordered[math.ceil(len(values) * .95) - 1])


def schedule(actors, cases, processes_per_block):
    """Pair each complete ordering with its reverse in a second block."""
    require(actors and len(actors) == len(set(actors)), 'distinct actors required')
    require(cases and len(cases) == len(set(cases)) and set(cases) <= set(CASES), 'distinct known cases required')
    require(type(processes_per_block) is int and processes_per_block > 0, 'positive process count required')
    forward = []
    for repetition in range(processes_per_block):
        case_order = list(cases)
        if repetition % 2:
            case_order.reverse()
        offset = repetition % len(actors)
        actor_order = list(actors[offset:]) + list(actors[:offset])
        for case in case_order:
            for actor in actor_order:
                forward.append(dict(repetition=repetition, case=case, actor=actor))
    return [dict(row, block=block, ordinal=index) for block, rows in
            enumerate((forward, list(reversed(forward)))) for index, row in enumerate(rows)]


def checked_calls(rows, *, samples, warmups, golden):
    require(isinstance(rows, list) and len(rows) == samples + warmups, 'incomplete call inventory')
    expected = golden.removesuffix('\n').encode()
    measured, curve = [], []
    for index, row in enumerate(rows):
        require(row.get('index') == index and type(row.get('warmup')) is bool
                and row['warmup'] == (index < warmups), 'call index/warmup mismatch')
        require(type(row.get('elapsed_ns')) is int and row['elapsed_ns'] > 0, 'invalid request time')
        require(row.get('exact_node_output_checked') is True
                and row.get('output_json_sha256') == sha(expected)
                and row.get('output_json_bytes') == len(expected), 'unchecked or mismatched Flight output')
        if row['warmup']:
            curve.append(row['elapsed_ns'])
        else:
            measured.append(row['elapsed_ns'])
    return dict(request_ns=stats(measured), warmup_curve_ns=curve)


def check_report(raw, plan, inputs, support, samples, warmups, host_sha, native_mode):
    for name, expected in inputs.items():
        require(raw['inputs'].get(name) == expected, 'source/wrapper/support mismatch: ' + name)
    case = plan['case']
    if plan['actor'] == 'v8':
        require(raw.get('schema') == 'scriptw-react-retained-module-reference-v1'
                and raw.get('case_order') == [case] and raw.get('case_selection') == case,
                'V8 must run exactly the selected case')
        require(raw.get('samples') == samples and raw.get('warmups') == warmups, 'V8 call counts differ')
        require(all(row.get('case') == case for row in raw['observations']), 'unexpected V8 case')
        rows = raw['observations']
        phases = {key: raw[key] for key in ('realm_creation_ns', 'source_module_parse_ns',
                                           'module_link_and_initialization_ns')}
    else:
        require(raw.get('schema') == 'scriptw-react-retained-baseline-v1'
                and raw.get('status') == 'complete' and 'profiling' not in raw and 'artifact_dump' not in raw
                and raw.get('case_selection') == case, 'incomplete, profiled or wrong Scriptw case')
        require(raw.get('executable_sha256') == host_sha and raw.get('native_mode') == native_mode,
                'host identity/policy differs')
        require(len(raw['cases']) == 1 and raw['cases'][0]['case'] == case, 'one case per Scriptw process required')
        selected = raw['cases'][0]
        require(selected['samples'] == samples and selected['warmups'] == warmups, 'Scriptw call counts differ')
        configuration = raw['configuration']
        require(configuration['compilation_jobs'] == 1 and configuration['native_parallel_compilation'] is False,
                'Scriptw compilation must be serial')
        policies = {'fast': ('None', 'SinglePass'), 'speed': ('Speed', 'Backtracking'),
                    'fast-backtracking': ('None', 'Backtracking'), 'speed-single-pass': ('Speed', 'SinglePass')}
        require((configuration['cranelift_opt_level'], configuration['register_allocation']) == policies[native_mode]
                and configuration['wasmtime'] == '47.0.0', 'unexpected native compilation policy/runtime')
        require(configuration.get('native_inlining') == 'No'
                and configuration.get('native_inlining_requested_flags') == {},
                'unexpected native inlining policy')
        require(raw.get('source_fingerprint') and raw.get('compiler_identity_scheme') and raw.get('toolchain')
                and raw.get('runtime') and raw.get('program_wasm_sha256'), 'missing compiled source/runtime identities')
        rows = selected['observations']
        phases = raw['phases_ns']
    require(phases and all(type(value) is int and value > 0 for value in phases.values()), 'invalid cold/setup phases')
    require(raw.get('request_clock') and raw.get('gc_policy'), 'missing timing/GC policy')
    return dict(checked_calls(rows, samples=samples, warmups=warmups, golden=support['expected'][case]),
                phases_ns=phases, request_clock=raw['request_clock'], gc_policy=raw['gc_policy'])


def aggregate(rows, actors, cases):
    result = {}
    for case in cases:
        result[case] = {}
        for actor in actors:
            selected = [row for row in rows if row['case'] == case and row['actor'] == actor]
            medians = [row['request_ns']['median'] for row in selected]
            if medians:
                result[case][actor] = dict(processes=len(selected), process_medians_ns=stats(medians),
                    process_means_ns=stats([row['request_ns']['mean'] for row in selected]),
                    process_p95_ns=stats([row['request_ns']['p95'] for row in selected]),
                    block_medians_ns={str(block): statistics.median([row['request_ns']['median']
                        for row in selected if row['block'] == block]) for block in (0, 1)
                        if any(row['block'] == block for row in selected)},
                    whole_process_peak_rss_bytes=stats([row['accounting']['peak_rss_bytes'] for row in selected]))
    return result


def named_paths(values):
    result = {}
    for value in values:
        name, separator, path = value.partition('=')
        require(separator and name and name != 'v8' and name.replace('-', '').isalnum()
                and name not in result, 'expected unique NAME=PATH, reserving v8')
        result[name] = Path(path).resolve(strict=True)
    return result


def actor_policies(hosts, default, overrides):
    result = dict.fromkeys(hosts, default)
    selected = set()
    for override in overrides:
        name, separator, mode = override.partition('=')
        require(separator and name in hosts and name not in selected and mode in NATIVE_MODES,
                'policy must be a unique host NAME=fast|speed|fast-backtracking|speed-single-pass')
        selected.add(name)
        result[name] = mode
    return result


def witness(path):
    stat = Path(path).stat()
    return dict(device=stat.st_dev, inode=stat.st_ino, mode=stat.st_mode,
                bytes=stat.st_size, mtime_ns=stat.st_mtime_ns, ctime_ns=stat.st_ctime_ns)


def stamp():
    return datetime.now(timezone.utc).isoformat()


def run(args):
    require_kernel_budget(4 * 1024**3)
    require(len(os.sched_getaffinity(0)) == 1, 'one inherited CPU required')
    require(args.samples > 0 and args.warmups >= 0 and args.samples + args.warmups <= 1000,
            'require positive samples, nonnegative warmups and at most 1000 calls')
    require(math.isfinite(args.timeout) and args.timeout > 0, 'positive finite process timeout required')
    hosts = named_paths(args.host)
    producers = named_paths(args.producer)
    policies = actor_policies(hosts, args.native_mode, args.policy)
    require(set(producers) <= set(hosts), 'producer names must match hosts')
    actors = [*hosts, 'v8']
    cases = args.case or list(CASES)
    plan = schedule(actors, cases, args.processes_per_block)
    environment = limited_environment()
    require(not environment.get('NODE_OPTIONS'), 'unset NODE_OPTIONS')
    require(not environment.get('LD_PRELOAD') and not environment.get('LD_AUDIT'),
            'unset dynamic-loader instrumentation')
    require(not any(value and key.startswith(('SCRIPTW_RSC_', 'SCRIPTW_WASM_TRACE',
                'SCRIPTW_PERF_MAP', 'SCRIPTW_EMIT_SIZE_REPORT', 'SCRIPTW_SPILL_HOME_CENSUS'))
                for key, value in environment.items()),
            'unset profiling/tracing instrumentation for scored campaign')
    environment.pop('NODE_COMPILE_CACHE', None)
    environment['NODE_DISABLE_COMPILE_CACHE'] = '1'
    environment['SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES'] = '1'
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('sources', 'tooling', 'hosts', 'logs', 'processes', 'scratch'):
        (output / name).mkdir()
    pinned = {}
    def retain(source, destination):
        source = Path(source).resolve(strict=True)
        target = output / destination
        shutil.copy2(source, target)
        pinned[str(source)] = dict(sha256=digest(source), stat=witness(source))
        return target
    source = retain(args.fixtures / 'rsc.js', 'sources/rsc.js')
    support_path = retain(args.fixtures / 'support.json', 'sources/support.json')
    support = json.loads(support_path.read_text())
    inputs = dict(source_sha256=digest(source), support_sha256=digest(support_path), wrapper=WRAPPER,
                  adapted_source_sha256=sha(source.read_bytes() + WRAPPER.encode()))
    driver = retain(ROOT / DRIVER, 'tooling/react-driver.cjs')
    for name in ('scripts/benchmark-rsc.py', 'scripts/benchmark_metrics.py', 'scripts/limited_verification.py', HOST_SOURCE):
        retain(ROOT / name, 'tooling/' + Path(name).name)
    frozen_hosts = {name: retain(path, 'hosts/' + name) for name, path in hosts.items()}
    for name, path in producers.items():
        retain(path, 'hosts/' + name + '.cargo.jsonl')
    node, time_binary = [Path(shutil.which(str(value)) or value).resolve(strict=True) for value in (args.node, args.time)]
    for path in (node, time_binary):
        pinned[str(path)] = dict(sha256=digest(path), stat=witness(path))
    group = kernel_group()
    resources = {field: (group / field).read_text().strip() for field in FIELDS}
    report = dict(schema='scriptw-retained-rsc-campaign-v1', status='incomplete', complete=False,
        started_at=stamp(), plan=plan, input_witnesses=pinned, processes=[], commands=[], summaries={}, inputs=inputs,
        tools=dict(hosts={name: dict(path=str(path), sha256=digest(path),
                                    cargo_producer_supplied=name in producers) for name, path in frozen_hosts.items()},
                   node=dict(path=str(node), sha256=digest(node)), time=dict(path=str(time_binary), sha256=digest(time_binary))),
        machine=dict(system=platform.system(), kernel=platform.release(), architecture=platform.machine(),
                     processor=platform.processor(), cpuinfo=Path('/proc/cpuinfo').read_text()),
        resources=dict(cgroup=str(group), affinity=sorted(os.sched_getaffinity(0)), before=resources,
            environment={key: environment.get(key) for key in ('SCRIPTW_JOBS', 'CARGO_BUILD_JOBS',
                'RUST_TEST_THREADS', 'RAYON_NUM_THREADS', 'SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES',
                'SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES', 'NODE_OPTIONS', 'NODE_DISABLE_COMPILE_CACHE')}),
        policy=dict(samples=args.samples, warmups=args.warmups, blocks=2, processes_per_block=args.processes_per_block,
            native_mode=args.native_mode, excluded_from_scored_cohorts=args.native_mode != 'fast',
            actor_native_modes=policies,
            diagnostic_actors=[name for name, mode in policies.items() if mode != 'fast'],
            timeout_seconds=args.timeout, case_isolation='one case per fresh process',
            order='two reversed blocks; actor rotation and alternating case order within first block',
            reduction='mean/median/p95 per process; campaign scores summarize fresh-process medians, never pooled calls',
            tail='p95 is a descriptive nearest-rank order statistic, not a confidence bound',
            warmup='all checked warmup times retained; count does not establish optimized steady state',
            memory='whole-child high-water RSS includes source compilation/setup/warmup; no request-only RSS claim',
            cold='first process of each selected case/actor is cold in that process; filesystem and OS caches remain',
            integrity='full SHA256 at admission and termination; per-use dev/inode/size/mode/mtime_ns/ctime_ns witnesses; no OS-enforced immutability',
            stages='render/consume/validation stage decomposition unavailable; no clock injection into scored bundle'))
    def save():
        report['summaries'] = aggregate(report['processes'], actors, cases)
        pending = output / 'report.pending'
        pending.write_text(json.dumps(report, indent=2) + '\n')
        pending.replace(output / 'report.json')
    retained = {path: dict(sha256=digest(path), stat=witness(path)) for directory in ('sources', 'tooling', 'hosts')
                for path in (output / directory).iterdir() if path.is_file()}
    def unchanged():
        for name, identity in pinned.items():
            path = Path(name)
            require(witness(path) == identity['stat'], 'input/tool changed: ' + name)
        for path, expected in retained.items():
            require(witness(path) == expected['stat'], 'retained input/tool changed: ' + str(path))
    identities = {}
    save()
    try:
        for index, item in enumerate(plan):
            unchanged()
            label = f'{index:03}-{item["actor"]}-{item["case"]}'
            print(f'[{index + 1}/{len(plan)}] {label}', flush=True)
            result_path = output / 'processes' / (label + '.json')
            if item['actor'] == 'v8':
                argv = [node, '--experimental-vm-modules', '--no-warnings', driver, 'retained-module',
                        source.parent, args.samples, args.warmups, item['case']]
                host_sha = None
            else:
                host = frozen_hosts[item['actor']]
                host_sha = report['tools']['hosts'][item['actor']]['sha256']
                argv = [host, source, support_path, result_path, args.samples, args.warmups,
                        policies[item['actor']], item['case']]
            scratch = output / 'scratch' / label
            scratch.mkdir()
            env = dict(environment, TMPDIR=str(scratch), SCRIPTW_CACHE_DIR=str(scratch / 'cache'))
            event = dict(item, argv=list(map(str, argv)), status='running', started_at=stamp(),
                         stdout='logs/' + label + '.stdout', stderr='logs/' + label + '.stderr')
            report['commands'].append(event)
            save()
            with (output / event['stdout']).open('xb') as stdout, (output / event['stderr']).open('xb') as stderr:
                event['accounting'] = measure_process(event['argv'], cwd=ROOT, env=env, stdout=stdout, stderr=stderr,
                                                      timeout=args.timeout, time_binary=time_binary)
            event.update(status='closed', finished_at=stamp())
            require(event['accounting']['exit_code'] == 0 and not event['accounting']['timed_out'], 'child failed: ' + label)
            require(not (output / event['stderr']).read_bytes(), 'unexpected stderr: ' + label)
            raw = json.loads((output / event['stdout']).read_text())
            if item['actor'] == 'v8':
                result_path.write_text(json.dumps(raw, indent=2) + '\n')
            else:
                require(json.loads(result_path.read_text()) == raw, 'host report differs from stdout')
            mode = 'fast'
            if item['actor'] != 'v8':
                mode = policies[item['actor']]
            checked = check_report(raw, item, inputs, support, args.samples, args.warmups, host_sha, mode)
            identity = raw.get('node')
            if item['actor'] != 'v8':
                identity = {key: raw.get(key) for key in ('source_commit', 'source_fingerprint', 'compiler_identity_scheme', 'toolchain', 'configuration', 'program_wasm_sha256', 'runtime')}
            require(identity is not None, 'missing runtime identity')
            require(identities.setdefault(item['actor'], identity) == identity, 'actor source/runtime identity changed')
            report['processes'].append(dict(item, **checked, accounting=event['accounting'],
                report=str(result_path.relative_to(output)), node=raw.get('node'),
                source_identity={key: raw.get(key) for key in ('source_commit', 'source_fingerprint', 'compiler_identity_scheme', 'toolchain', 'configuration', 'program_wasm_sha256', 'runtime')}))
            save()
        report['identities'] = identities
        report.update(status='complete', complete=True)
    except BaseException as error:
        report.update(status='failed', complete=False, error=str(error))
        raise
    finally:
        integrity_errors = []
        report['terminal_sha256'] = {}
        for path, identity in {**pinned, **retained}.items():
            try:
                current = digest(path)
                report['terminal_sha256'][str(path)] = current
                require(current == identity['sha256'] and witness(path) == identity['stat'],
                        'terminal input/tool identity differs: ' + str(path))
            except (OSError, ValueError) as error:
                integrity_errors.append(str(error))
        try:
            report['resources']['after'] = {field: (group / field).read_text().strip() for field in FIELDS}
            for field in FIELDS[:-1]:
                require(resources[field] == report['resources']['after'][field], 'resource policy changed: ' + field)
            before = dict(line.split() for line in resources['memory.events'].splitlines())
            after = dict(line.split() for line in report['resources']['after']['memory.events'].splitlines())
            require(all(before.get(key) == after.get(key) for key in ('oom', 'oom_kill', 'oom_group_kill')), 'OOM events changed')
        except (OSError, ValueError) as error:
            integrity_errors.append(str(error))
        if integrity_errors:
            report.update(status='failed', complete=False, integrity_errors=integrity_errors)
        report['finished_at'] = stamp()
        report['file_sha256'] = {str(path.relative_to(output)): digest(path) for directory in
            ('sources', 'tooling', 'hosts', 'logs', 'processes') for path in sorted((output / directory).iterdir()) if path.is_file()}
        save()
        require(not integrity_errors, '; '.join(integrity_errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', action='append', required=True, help='repeat NAME=PATH for frozen retained host actors')
    parser.add_argument('--producer', action='append', default=[], help='optional NAME=PATH Cargo producer log to retain')
    parser.add_argument('--fixtures', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--node', default='node')
    parser.add_argument('--time', required=True)
    parser.add_argument('--case', action='append', choices=CASES)
    parser.add_argument('--samples', type=int, default=21)
    parser.add_argument('--warmups', type=int, default=100)
    parser.add_argument('--processes-per-block', type=int, default=4)
    parser.add_argument('--timeout', type=float, default=300)
    parser.add_argument('--native-mode', choices=NATIVE_MODES, default='fast')
    parser.add_argument('--policy', action='append', default=[],
                        help='optional NAME=MODE override; keeps native-policy actors in the same balanced schedule')
    run(parser.parse_args())


if __name__ == '__main__':
    main()
