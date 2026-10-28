"""Linux process measurements and checked, comparable benchmark evidence."""
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import shutil
import statistics
import subprocess
import threading
import tempfile
import time

SCHEMA = 'scriptw-v8-machine-baseline-v2'
POLICY_SCHEMA = 'scriptw-v8-machine-policy-baseline-v1'
SCOREBOARD_SCHEMA = 'scriptw-v8-boundary-scoreboard-v1'


def validate_native_policy(policy):
    if (not isinstance(policy, dict)
            or set(policy) != {'native_mode', 'gc_initial_mib', 'register_allocation'}
            or policy['native_mode'] not in ('fast', 'size-optimized', 'speed')
            or type(policy['gc_initial_mib']) is not int or policy['gc_initial_mib'] not in (8, 16, 32, 64, 128)
            or policy['register_allocation'] not in ('single-pass', 'backtracking')):
        raise ValueError('invalid explicit native-policy cohort')


def validate_workload_metadata(workload):
    """Closed benchmark categories cannot silently invent a timing boundary."""
    partition = workload.get('partition', 'unassigned')
    if partition not in ('unassigned', 'tuning', 'holdout'):
        raise ValueError('workload partition must be tuning, holdout or unassigned')
    mode = workload.get('execution_mode', 'fresh-realm')
    if mode not in ('fresh-realm', 'same-realm-batched'):
        raise ValueError('unsupported workload execution mode')
    if mode == 'same-realm-batched':
        parameters = workload.get('parameters', {})
        if not isinstance(parameters, dict):
            raise ValueError('same-realm workload parameters must be an object')
        batches = parameters.get('batches')
        if type(batches) is not int or not 1 <= batches <= 256:
            raise ValueError('same-realm workload needs 1..256 checked batches')
        if partition == 'unassigned':
            raise ValueError('same-realm workload needs an explicit tuning or holdout partition')


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def summary(values):
    if len(values) < 3 or any(type(n) is not int or n < 0 for n in values):
        raise ValueError('need at least three nonnegative integer samples')
    ordered = sorted(values)
    return dict(samples=values, median=statistics.median(values), minimum=min(values),
                maximum=max(values), p90=ordered[math.ceil(len(values) * .90) - 1],
                p95=ordered[math.ceil(len(values) * .95) - 1])


def measure_process(command, *, cwd, env, stdout, stderr, timeout, time_binary=None):
    """GNU time observes the actual exec child's kernel-recorded RSS peak.

    Blocking wait avoids Popen.wait(timeout)'s polling/rounding in short startup
    measurements. The small native supervisor prevents the Python parent's RSS
    from becoming the child's pre-exec high-water floor. wait4 CPU accounting
    includes the supervisor; parent wall time includes launch and supervision.
    """
    if not hasattr(os, 'wait4') or not Path('/proc/self/status').exists():
        raise ValueError('benchmark process accounting requires Linux wait4 and /proc')
    if timeout <= 0:
        raise ValueError('timeout must be positive')
    time_binary = time_binary or shutil.which('gtime') or shutil.which('time')
    if not time_binary:
        raise ValueError('GNU time is required for child RSS accounting; install time or select --time')
    expired = threading.Event()
    with tempfile.NamedTemporaryFile() as accounting:
        result = _wait_for_process([str(time_binary), '-q', '-f', '%M', '-o', accounting.name, '--',
                                    *map(str, command)], cwd, env, stdout, stderr, timeout, expired)
        raw = Path(accounting.name).read_text().strip()
    result['launch_tree_peak_rss_bytes'] = result.pop('peak_rss_bytes')
    result['peak_rss_bytes'] = int(raw) * 1024 if raw.isdecimal() else None
    result['child_peak_rss_kib'] = int(raw) if raw.isdecimal() else None
    result['memory_scope'] = 'runtime-child-lifetime-high-water-rss;excludes-python-launcher'
    result['cpu_scope'] = 'gnu-time-supervisor-and-runtime-child'
    if result['exit_code'] == 0 and result['peak_rss_bytes'] is None:
        raise ValueError('GNU time did not produce child RSS accounting')
    return result


def _wait_for_process(command, cwd, env, stdout, stderr, timeout, expired):
    started = time.perf_counter_ns()
    with subprocess.Popen(command, cwd=cwd, env=env, stdout=stdout, stderr=stderr,
                          start_new_session=True) as child:
        def stop():
            expired.set()
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        watchdog = threading.Timer(timeout, stop)
        watchdog.daemon = True
        watchdog.start()
        try:
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            elapsed = time.perf_counter_ns() - started
        except BaseException:
            stop()
            _, status, _ = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            raise
        finally:
            watchdog.cancel()
            watchdog.join()
        result = dict(exit_code=child.returncode, elapsed_ns=elapsed,
                      user_cpu_ns=round(usage.ru_utime * 1e9),
                      system_cpu_ns=round(usage.ru_stime * 1e9),
                      peak_rss_bytes=usage.ru_maxrss * 1024,
                      timed_out=expired.is_set())
    return result


def summarize(report):
    startup = {}
    for mode in ('cold', 'hot') if report['configuration']['startup'] else ():
        startup[mode] = {}
        for runtime in ('scriptw', 'v8'):
            rows = [p for p in report['process_samples'] if p['mode'] == mode and p['runtime'] == runtime]
            if len(rows) != report['samples']:
                raise ValueError('incomplete startup sample inventory')
            startup[mode][runtime] = {metric: summary([p[metric] for p in rows])
                                      for metric in ('elapsed_ns', 'peak_rss_bytes', 'user_cpu_ns', 'system_cpu_ns')}
    execution = {}
    for runtime, result in report['execution'].items():
        execution[runtime] = {}
        for case in result['cases']:
            samples = [p for p in case['observations'] if not p['warmup']]
            if len(samples) != report['samples']:
                raise ValueError('incomplete execution sample inventory')
            metrics = dict(execution_ns=summary([p['execution_ns'] for p in samples]))
            if runtime == 'wasmtime':
                for stage in samples[0]['phases_ns']:
                    metrics[stage + '_ns'] = summary([p['phases_ns'][stage] for p in samples])
                for key in ('gc_capacity_before_bytes', 'gc_capacity_after_bytes'):
                    metrics[key] = summary([p['memory'][key] for p in samples])
                observations = [p['memory']['process_rss'] for p in samples]
                if all(p['status'] == 'sampled' for p in observations):
                    metrics['sampled_invocation_max_rss_bytes'] = summary([p['maximum_rss_bytes'] for p in observations])
                else:
                    metrics['sampled_invocation_max_rss_bytes'] = None
            else:
                for key in ('rss', 'heapUsed', 'heapTotal', 'external', 'arrayBuffers'):
                    metrics[key + '_after_bytes'] = summary([p['memory']['after'][key] for p in samples])
            execution[runtime][case['file']] = metrics
    return dict(startup=startup, execution=execution)


def load_report(path):
    """Recompute summaries and verify retained logs/workload/artifact bytes."""
    path = Path(path).resolve()
    report = json.loads(path.read_text())
    if report.get('schema') not in (SCHEMA, POLICY_SCHEMA) or report.get('complete') is not True:
        raise ValueError('baseline must be a complete supported benchmark report')
    native_policy = report['configuration'].get('native_policy')
    if report['schema'] == POLICY_SCHEMA:
        validate_native_policy(native_policy)
        if report['configuration']['startup']:
            raise ValueError('explicit native-policy cohorts measure runner execution, not default CLI startup')
    elif native_policy is not None:
        raise ValueError('ordinary benchmark reports cannot carry an explicit native policy')
    if (set(report['execution']) != {'wasmtime', 'v8'} or type(report['samples']) is not int
            or not 3 <= report['samples'] <= 100):
        raise ValueError('complete benchmark requires both runtimes and 3..100 samples')
    warmups = report['configuration']['warmups']
    if type(warmups) is not int or not 1 <= warmups <= 100:
        raise ValueError('invalid warmup plan')
    workloads = report['workloads']
    for workload in workloads:
        validate_workload_metadata(workload)
    names = [w['file'] for w in workloads]
    if (len(names) != len(set(names)) or any(Path(n).name != n or not n.endswith('.js') for n in names)
            or sum(w['role'] == 'startup' for w in workloads) != 1
            or not any(w['role'] == 'execution' for w in workloads)
            or any(w['role'] not in ('startup', 'execution') for w in workloads)):
        raise ValueError('invalid benchmark workload inventory')
    if summarize(report) != report['summaries']:
        raise ValueError('benchmark summaries differ from retained raw samples')
    root = path.parent
    for item in report['retained_files']:
        member = (root / item['path']).resolve()
        if not member.is_relative_to(root) or digest(member) != item['sha256']:
            raise ValueError('retained benchmark file missing, changed or outside report directory')
    commands = {row['label']: row for row in report['commands']}
    if len(commands) != len(report['commands']):
        raise ValueError('duplicate benchmark command')
    expected_labels = {'compiler-identity'} | {'execution-' + runtime + '-' + w['file']
        for runtime in ('wasmtime', 'v8') for w in workloads if w['role'] == 'execution'}
    startup_labels = set()
    if report['configuration']['startup']:
        expected_labels.add('scriptw-prime-hot-cache')
        startup_labels = {f'{mode}-{index:02}-{runtime}' for mode in ('cold', 'hot')
                          for index in range(report['samples']) for runtime in ('scriptw', 'v8')}
        expected_labels |= startup_labels
    if set(commands) != expected_labels or {p['label'] for p in report['process_samples']} != startup_labels:
        raise ValueError('benchmark command inventory differs from the plan')
    if len(report['process_samples']) != len(startup_labels):
        raise ValueError('duplicate startup observation')
    for event in commands.values():
        if event['state'] != 'closed' or event['exit_code'] != 0 or event['timed_out']:
            raise ValueError('baseline contains an unsuccessful command')
        for stream in ('stdout', 'stderr'):
            if digest(root / 'logs' / (event['label'] + '.' + stream)) != event[stream + '_sha256']:
                raise ValueError('command log identity differs')
    for sample in report['process_samples']:
        event = commands[sample['label']]
        if any(sample[key] != event[key] for key in ('elapsed_ns', 'peak_rss_bytes', 'user_cpu_ns', 'system_cpu_ns')):
            raise ValueError('startup sample differs from measured child')
        expected = next(w['stdout'] for w in report['workloads'] if w['role'] == 'startup')
        if sample['runtime'] == 'scriptw':
            expected += 'run outcome: RunOutcome { backend_used: WasmAot, note: "wasm-aot completion: undefined(undefined)" }\n'
        if (root / 'logs' / (sample['label'] + '.stdout')).read_text() != expected:
            raise ValueError('startup checksum differs')
    for workload in report['workloads']:
        if digest(root / 'workload' / workload['file']) != workload['sha256']:
            raise ValueError('retained workload identity differs')
    manifest = json.loads((root / 'workload' / 'manifest.json').read_text())
    expected_workloads = [('startup', manifest['startup']), *[('execution', w) for w in manifest['execution']]]
    if [dict(w, role=role, sha256=digest(root / 'workload' / w['file'])) for role, w in expected_workloads] != workloads:
        raise ValueError('retained workload plan differs')
    for runtime, result in report['execution'].items():
        expected = [w for w in report['workloads'] if w['role'] == 'execution']
        if [c['file'] for c in result['cases']] != [w['file'] for w in expected]:
            raise ValueError('execution workload inventory differs')
        for case, workload in zip(result['cases'], expected, strict=True):
            if case['command_label'] != 'execution-' + runtime + '-' + case['file'] or case['command_label'] not in commands:
                raise ValueError('execution case has no matching child command')
            raw = json.loads((root / 'logs' / (case['command_label'] + '.stdout')).read_text())
            expected_schema = ('scriptw-wasmtime-policy-baseline-v1' if native_policy is not None else
                               'scriptw-wasmtime-baseline-v2') if runtime == 'wasmtime' else 'scriptw-v8-baseline-v2'
            if raw['schema'] != expected_schema or raw['samples'] != report['samples'] or raw['warmups'] != report['configuration']['warmups']:
                raise ValueError('runtime wire plan differs')
            if runtime == 'wasmtime' and native_policy is not None:
                if raw.get('requested_native_policy') != native_policy:
                    raise ValueError('runtime requested native policy differs from the recorded cohort')
                if any(row.get('actual_native_policy') != native_policy for row in case['observations']):
                    raise ValueError('runtime actual native policy differs from its requested cohort')
            if raw['cases'] != [{k: v for k, v in case.items() if k != 'command_label'}]:
                raise ValueError('execution observations differ from retained process output')
            if case['source_sha256'] != workload['sha256']:
                raise ValueError('execution source identity differs')
            if runtime == 'wasmtime' and (digest(root / 'wasm' / (case['file'] + '.wasm')) != case['wasm_sha256']
                                         or (root / 'wasm' / (case['file'] + '.wasm')).stat().st_size != case['wasm_bytes']):
                raise ValueError('retained emitted Wasm differs')
            rows = case['observations']
            warmups = report['configuration']['warmups']
            if len(rows) != warmups + report['samples']:
                raise ValueError('execution sample plan differs')
            for index, row in enumerate(rows):
                if row['warmup'] != (index < warmups) or row['stdout'] != workload['stdout']:
                    raise ValueError('execution checksum or warmup differs')
                if runtime == 'wasmtime' and index >= warmups and row['module_cache'] != 'Hit':
                    raise ValueError('measured Wasmtime module was not warm')
                if runtime == 'wasmtime':
                    rss = row['memory']['process_rss']
                    if rss['status'] == 'sampled':
                        if (rss['scope'] != 'runtime-invocation-process-rss'
                                or any(type(rss[k]) is not int or rss[k] < 0 or rss[k] > 2**64 - 1
                                       for k in ('interval_ns', 'elapsed_ns', 'sample_count', 'initial_rss_bytes', 'final_rss_bytes', 'maximum_rss_bytes'))
                                or any(rss[k] == 0 or rss[k] % 1024 for k in ('initial_rss_bytes', 'final_rss_bytes', 'maximum_rss_bytes'))
                                or rss['sample_count'] < 2 or rss['interval_ns'] != 10_000_000
                                or rss['sample_count'] > rss['elapsed_ns'] // rss['interval_ns'] + 2
                                or rss['maximum_rss_bytes'] < max(rss['initial_rss_bytes'], rss['final_rss_bytes'])
                                or rss['elapsed_ns'] > row['phases_ns']['total']):
                            raise ValueError('invalid invocation RSS observation')
                    elif rss['status'] != 'unavailable' or rss['reason'] not in (
                            'UnsupportedPlatform', 'ProcStatusReadFailed', 'ProcStatusMalformed',
                            'SamplingThreadStartFailed', 'SamplingThreadPanicked', 'SampleCountOverflow'):
                        raise ValueError('invalid RSS unavailable observation')
                    unavailable = {k: 'WasmtimeHasNoPublicCounter' for k in (
                        'live_gc_bytes', 'allocation_count', 'allocation_bytes', 'collection_count', 'gc_pause_time')}
                    unavailable['peak_gc_heap_bytes'] = 'NoPerInvocationPeakSampler'
                    if row['memory']['unavailable'] != unavailable:
                        raise ValueError('unavailable Wasmtime metrics must retain their reasons')
                else:
                    if row['memory']['scope'] != 'process-wide-endpoints;natural-gc;not-peak-or-live-workload-bytes':
                        raise ValueError('invalid V8 memory scope')
                    for endpoint in ('before_context', 'before', 'after'):
                        memory = row['memory'][endpoint]
                        if any(type(memory[k]) is not int or memory[k] < 0 for k in ('rss', 'heapUsed', 'heapTotal', 'external', 'arrayBuffers')):
                            raise ValueError('invalid V8 memory endpoint')
    return report


def boundary_scoreboard(report):
    """Derive named boundaries from an admitted v2 report without retiming it.

    V8's context + execution is a sum of two observed spans, not a contiguous
    end-to-end timer. Batched scores amortize the *complete* guest invocation;
    they must never be described as excluding bootstrap or setup.
    """
    workloads = [w for w in report['workloads'] if w['role'] == 'execution']
    startup = []
    for mode, runtimes in report['summaries']['startup'].items():
        before = runtimes['v8']['elapsed_ns']['median']
        after = runtimes['scriptw']['elapsed_ns']['median']
        startup.append(dict(scenario=mode,
            scriptw=runtimes['scriptw']['elapsed_ns'], v8=runtimes['v8']['elapsed_ns'],
            scriptw_over_v8=after / before if before else None))
    precompiled = []
    batched = []
    cases = {runtime: {case['file']: case for case in result['cases']}
             for runtime, result in report['execution'].items()}
    for workload in workloads:
        validate_workload_metadata(workload)
        name = workload['file']
        fresh = dict(file=name, partition=workload.get('partition', 'unassigned'), runtimes={})
        for runtime in ('wasmtime', 'v8'):
            rows = [row for row in cases[runtime][name]['observations'] if not row['warmup']]
            if runtime == 'wasmtime':
                fresh['runtimes'][runtime] = dict(status='measured',
                    scope='observed invocation total: Engine/module-cache admission, Store/linker, instantiation, execution and completion; source compilation excluded',
                    elapsed_ns=summary([row['phases_ns']['total'] for row in rows]))
            elif all('context_ns' not in row for row in rows):
                fresh['runtimes'][runtime] = dict(status='unavailable',
                    reason='this retained V8 report did not record context creation time')
            else:
                if any(type(row.get('context_ns')) is not int or row['context_ns'] < 0 for row in rows):
                    raise ValueError('V8 context spans must be present and nonnegative for every measured sample')
                fresh['runtimes'][runtime] = dict(status='measured',
                    scope='sum of observed context creation and execution spans; source compilation and inter-span memory observations excluded; not one contiguous timer',
                    elapsed_ns=summary([row['context_ns'] + row['execution_ns'] for row in rows]))
        left, right = fresh['runtimes']['wasmtime'], fresh['runtimes']['v8']
        fresh['scriptw_over_v8'] = (left['elapsed_ns']['median'] / right['elapsed_ns']['median']
            if right['status'] == 'measured' and right['elapsed_ns']['median'] else None)
        precompiled.append(fresh)
        if workload.get('execution_mode') == 'same-realm-batched':
            count = workload['parameters']['batches']
            entry = dict(file=name, partition=workload['partition'], batches=count, runtimes={})
            for runtime in ('wasmtime', 'v8'):
                elapsed = report['summaries']['execution'][runtime][name]['execution_ns']
                entry['runtimes'][runtime] = dict(invocation_ns=elapsed,
                    median_ns_per_batch=elapsed['median'] / count,
                    scope='complete invocation amortized over checked same-realm batches; includes realm bootstrap, workload setup, construction and final checksum')
            before = entry['runtimes']['v8']['median_ns_per_batch']
            after = entry['runtimes']['wasmtime']['median_ns_per_batch']
            entry['scriptw_over_v8'] = after / before if before else None
            batched.append(entry)
    aggregates = []
    for boundary, entries in (('precompiled-invocation', precompiled), ('same-realm-batched', batched)):
        for partition in ('unassigned', 'tuning', 'holdout'):
            selected = [entry for entry in entries if entry['partition'] == partition]
            if not selected:
                continue
            ratios = [entry['scriptw_over_v8'] for entry in selected]
            available = all(ratio is not None and ratio > 0 for ratio in ratios)
            aggregates.append(dict(boundary=boundary, partition=partition, cases=len(selected),
                status='measured' if available else 'unavailable',
                geometric_mean_ratio=math.exp(sum(math.log(ratio) for ratio in ratios) / len(ratios)) if available else None,
                worst_case_ratio=max(ratios) if available else None))
    return dict(schema=SCOREBOARD_SCHEMA, source_commit=report['source_commit'],
        measurement_schema=report['schema'], native_policy=report['configuration'].get('native_policy'),
        source_startup=dict(status='measured' if startup else 'unavailable', scenarios=startup,
            scope=report['policy'].get('startup', 'recorded source-to-result child process spans'),
            reason=None if startup else 'startup was not selected in this report'),
        precompiled_invocation=precompiled, same_realm_batched=batched,
        fresh_process_precompiled_startup=dict(status='unavailable',
            reason='the runtime driver compiles once in its process before repeated invocations; it did not time launching a fresh artifact-only process'),
        isolated_steady_state=dict(status='unavailable',
            reason='the ordinary runner recreates the Store/context per invocation; no persistent-instance entry timer was measured'),
        aggregates=aggregates,
        memory_and_artifacts=dict(
            scope='process RSS, Wasmtime GC capacity and V8 heap endpoints remain distinct metrics',
            execution_summaries=report['summaries']['execution'],
            emitted_wasm_bytes={case['file']: case['wasm_bytes'] for case in report['execution']['wasmtime']['cases']}),
        interpretation='Descriptive measured ratios, not significance, performance acceptance or a general JavaScript score. Tuning and holdout aggregates stay separate. Batched time includes amortized setup; no empty-control subtraction.')


def render_boundary_scoreboard(board):
    lines = ['| Boundary | Case | Partition | Scriptw / V8 median ratio |',
             '| --- | --- | --- | ---: |']
    for row in board['source_startup']['scenarios']:
        ratio = row['scriptw_over_v8']
        lines.append(f"| Source startup | {row['scenario']} | — | {ratio:.3f} |" if ratio is not None else
                     f"| Source startup | {row['scenario']} | — | unavailable |")
    for boundary, entries in (('Precompiled invocation', board['precompiled_invocation']),
                              ('Same-realm batch, setup amortized', board['same_realm_batched'])):
        for row in entries:
            ratio = f"{row['scriptw_over_v8']:.3f}" if row['scriptw_over_v8'] is not None else 'unavailable'
            lines.append(f"| {boundary} | {row['file']} | {row['partition']} | {ratio} |")
    lines += ['', 'Precompiled V8 time sums context creation and execution; Wasmtime uses its observed invocation total. Source compilation is excluded.',
              'Same-realm batch time includes amortized bootstrap, setup, construction and checksum. Isolated persistent-instance steady-state timing is unavailable.',
              'Seven-sample p95 values elsewhere in this report are descriptive order statistics, not established tail-latency bounds.', board['interpretation'], '']
    return '\n'.join(lines)


def compare(baseline, candidate):
    for key in ('schema', 'machine', 'configuration', 'policy', 'profile', 'node_versions', 'time_version', 'time_sha256',
                'wasmtime_version', 'workloads', 'harness_sha256', 'samples'):
        if baseline.get(key) != candidate.get(key):
            raise ValueError(f'incompatible benchmark baseline: {key} differs')
    changes = []

    def visit(before, after, prefix):
        if before is None or after is None:
            changes.append(dict(metric='/'.join(prefix), baseline=before['median'] if before else None,
                                candidate=after['median'] if after else None, delta=None, ratio=None,
                                availability_changed=(before is None) != (after is None)))
            return
        if 'median' in before:
            b, a = before['median'], after['median']
            changes.append(dict(metric='/'.join(prefix), baseline=b, candidate=a,
                                delta=a - b, ratio=a / b if b else None))
        else:
            if before.keys() != after.keys():
                raise ValueError('benchmark metric inventory differs')
            for key in before:
                visit(before[key], after[key], [*prefix, key])
    visit(baseline['summaries'], candidate['summaries'], [])
    process_changes = []
    before_commands = {p['label']: p for p in baseline['commands']}
    for event in candidate['commands']:
        if event['label'].startswith('execution-'):
            b = before_commands[event['label']]['peak_rss_bytes']
            a = event['peak_rss_bytes']
            process_changes.append(dict(command=event['label'], baseline_bytes=b, candidate_bytes=a,
                                        ratio=a / b if b else None))
    return dict(baseline_commit=baseline['source_commit'], candidate_commit=candidate['source_commit'],
                interpretation='Observed median ratios; no statistical significance or automatic acceptance claim.',
                changes=changes, whole_child_peak_changes=process_changes)


def render(report):
    lines = ['# Scriptw performance and memory', '', f"Source: `{report['source_commit']}`. {report['samples']} samples; one pinned CPU.", '',
             '| Startup | Runtime | Median ms | p95 ms | Median peak RSS MiB |',
             '| --- | --- | ---: | ---: | ---: |']
    for mode, runtimes in report['summaries']['startup'].items():
        for runtime, metrics in runtimes.items():
            lines.append(f"| {mode} | {runtime} | {metrics['elapsed_ns']['median']/1e6:.3f} | {metrics['elapsed_ns']['p95']/1e6:.3f} | {metrics['peak_rss_bytes']['median']/2**20:.2f} |")
    lines += ['', '| Execution | Runtime | Median ms | p95 ms | Invocation RSS / endpoint RSS MiB | GC capacity / V8 used heap MiB |',
              '| --- | --- | ---: | ---: | ---: | ---: |']
    for runtime, cases in report['summaries']['execution'].items():
        for name, metrics in cases.items():
            rss = metrics.get('sampled_invocation_max_rss_bytes') if runtime == 'wasmtime' else metrics['rss_after_bytes']
            heap = metrics['gc_capacity_after_bytes'] if runtime == 'wasmtime' else metrics['heapUsed_after_bytes']
            rss_text = f"{rss['median']/2**20:.2f}" if rss else 'unavailable'
            lines.append(f"| {name} | {runtime} | {metrics['execution_ns']['median']/1e6:.3f} | {metrics['execution_ns']['p95']/1e6:.3f} | {rss_text} | {heap['median']/2**20:.2f} |")
    lines += ['', 'RSS covers the process, including caches and host/runtime overhead. Wasmtime GC capacity and V8 used heap have different meanings; do not compare them as live JS bytes.',
              'Raw phase timings, compilation spans, CPU time, whole-child high-water RSS, artifact bytes and all warmups are retained in report.json and logs/.', '']
    lines += ['| Whole execution child (one observation per case) | Peak RSS MiB | User CPU ms | System CPU ms |',
              '| --- | ---: | ---: | ---: |']
    for event in report['commands']:
        if event['label'].startswith('execution-'):
            lines.append(f"| {event['label']} | {event['peak_rss_bytes']/2**20:.2f} | {event['user_cpu_ns']/1e6:.3f} | {event['system_cpu_ns']/1e6:.3f} |")
    lines += ['', '| Wasmtime case | Prepare ms | Lower ms | Emit ms | Validate ms | Emitted Wasm bytes |',
              '| --- | ---: | ---: | ---: | ---: | ---: |']
    for case in report['execution']['wasmtime']['cases']:
        c = case['compilation_ns']
        lines.append(f"| {case['file']} | {c['preparation']/1e6:.3f} | {c['lowering']/1e6:.3f} | {c['emission']/1e6:.3f} | {c['validation']/1e6:.3f} | {case['wasm_bytes']:,} |")
    lines += ['', 'Compilation spans are single diagnostic observations, outside repeated execution samples. Whole-child peak RSS includes compilation and warmups.', '']
    if report.get('comparison'):
        lines += ['| Change versus baseline | Median ratio (candidate / baseline) |', '| --- | ---: |']
        for row in report['comparison']['changes']:
            ratio = f"{row['ratio']:.3f}" if row['ratio'] is not None else 'unavailable'
            lines.append(f"| {row['metric']} | {ratio} |")
        lines += ['', report['comparison']['interpretation'], '']
    lines += ['', render_boundary_scoreboard(boundary_scoreboard(report))]
    return '\n'.join(lines)
