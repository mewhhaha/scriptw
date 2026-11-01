#!/usr/bin/env python3
"""Checked cold-P compilation and execution/memory policy controls."""
import argparse
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess

from benchmark_metrics import digest, measure_process
from limited_verification import require_kernel_budget

COHORTS = {
    'fast-16-single-pass': ('fast', 16, 'single-pass', 'disabled'),
    'fast-64-single-pass': ('fast', 64, 'single-pass', 'disabled'),
    'speed-16-single-pass': ('speed', 16, 'single-pass', 'disabled'),
    'speed-16-backtracking': ('speed', 16, 'backtracking', 'disabled'),
    'speed-16-bounded-inlining': ('speed', 16, 'single-pass', 'bounded'),
}


def controls():
    entries = [('small.js', 'print(42);', '42\n', 'small')]
    for retained in (False, True):
        for count in (256, 1024):
            setup = 'var rows=[];' if retained else 'function take(o){return o.a+o.b+o.c;}'
            operation = 'rows.push({a:i,b:i+1,c:i+2});' if retained else 'sum+=take({a:i,b:i+1,c:i+2});'
            traversal = 'for(var j=0;j<rows.length;j++)sum+=rows[j].a+rows[j].b+rows[j].c;' if retained else ''
            text = f'(function(){{{setup}var sum=0;for(var i=0;i<{count};i++){{{operation}}}{traversal}print(sum);}})();'
            total = sum(3 * index + 3 for index in range(count))
            entries.append((f'{"retained" if retained else "ephemeral"}-{count}.js', text,
                            f'{total}\n', 'retained' if retained else 'ephemeral'))
    for strict in (False, True):
        for arity in (0, 1, 2, 3, 4, 8):
            parameters = ','.join(f'a{i}' for i in range(arity))
            arguments = ','.join(f'i+{i}' for i in range(arity))
            directive = '"use strict";' if strict else ''
            text = f'(function(){{{directive}function consume({parameters}){{return 1;}}var sum=0;for(var i=0;i<1024;i++)sum+=consume({arguments});print(sum);}})();'
            entries.append((f'calls-{arity}-{"strict" if strict else "sloppy"}.js', text,
                            '1024\n', 'call-arity'))
    return entries


def validate(raw, policy, name, expected, samples, warmups):
    if (raw.get('schema') != 'scriptw-native-policy-break-even-v1'
            or raw['case'] != name or raw['requested_policy'] != policy
            or raw['samples'] != samples or raw['warmups'] != warmups):
        raise ValueError('native policy/sample identity differs')
    native, execution = raw['native_compilation'], raw['execution']
    if len(native) != samples + 1 or len(execution) != samples + warmups:
        raise ValueError('truncated policy observations')
    for index, row in enumerate(native):
        if (type(row['warmup']) is not bool or row['warmup'] != (index == 0)
                or row['actual_policy'] != policy
                or row['native_module_cache'] != 'disabled'
                or row['incremental_function_cache'] != 'disabled'
                or type(row['compile_and_validate_ns']) is not int
                or row['compile_and_validate_ns'] <= 0
                or type(row['engine_ns']) is not int or row['engine_ns'] <= 0
                or type(row['native_text_bytes']) is not int
                or type(row['serialized_native_bytes']) is not int
                or not 0 < row['native_text_bytes'] <= row['serialized_native_bytes']):
            raise ValueError('cold native-P compilation evidence differs')
    for index, row in enumerate(execution):
        if (type(row['warmup']) is not bool or row['warmup'] != (index < warmups)
                or row['actual_policy'] != policy
                or row['stdout'] != expected
                or type(row['execution_and_completion_ns']) is not int
                or row['execution_and_completion_ns'] <= 0
                or type(row['total_invocation_ns']) is not int
                or row['total_invocation_ns'] < row['execution_and_completion_ns']
                or row['matched_runtime_image_cache'] not in ('Hit', 'Miss')
                or (index >= warmups and (row['module_cache'] != 'Hit'
                    or row['matched_runtime_image_cache'] != 'Hit'))):
            raise ValueError('execution policy/output/cache evidence differs')
    return dict(cold_compile_ns=statistics.median(row['compile_and_validate_ns'] for row in native[1:]),
                cold_engine_ns=statistics.median(row['engine_ns'] for row in native[1:]),
                execution_ns=statistics.median(row['execution_and_completion_ns'] for row in execution[warmups:]),
                total_invocation_ns=statistics.median(row['total_invocation_ns'] for row in execution[warmups:]),
                native_text_bytes=[row['native_text_bytes'] for row in native[1:]],
                serialized_native_bytes=[row['serialized_native_bytes'] for row in native[1:]])


def observed_break_even(before, after):
    """A nonpositive measured gain cannot establish a repayment point."""
    saved = before['execution_ns'] - after['execution_ns']
    extra = after['cold_compile_ns'] - before['cold_compile_ns']
    total_saved = before['total_invocation_ns'] - after['total_invocation_ns']
    return dict(extra_cold_P_compile_ns=extra, observed_saved_execution_ns=saved,
                invocations=math.ceil(max(0, extra) / saved) if saved > 0 else None,
                observed_saved_total_invocation_ns=total_saved,
                invocations_using_total=math.ceil(max(0, extra) / total_saved) if total_saved > 0 else None,
                availability='observed positive execution gain' if saved > 0 else 'no positive observed execution gain')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--time', type=Path, required=True)
    parser.add_argument('--node', default='node')
    parser.add_argument('--samples', type=int, choices=range(3, 101), default=7)
    parser.add_argument('--warmups', type=int, choices=range(3, 101), default=10)
    parser.add_argument('--case', action='append', dest='cases')
    parser.add_argument('--cohort', action='append', choices=tuple(COHORTS))
    parser.add_argument('--idle-machine', action='store_true', required=True)
    args = parser.parse_args()
    # Whole-R inlining retains intermediate functions in pinned Wasmtime. Its
    # first real-R memory admission must run in a separate supervised scope.
    args.cohort = args.cohort or list(COHORTS)[:4]
    require_kernel_budget(4096 * 1024**2)
    if (len(os.sched_getaffinity(0)) != 1 or os.environ.get('NODE_OPTIONS')
            or os.environ.get('SCRIPTW_COMPILER_CACHE_IMAGE_HASH', 'sha256') != 'sha256'):
        parser.error('one inherited CPU, normal Node flags and the SHA-256 cache-image control are required')
    output = args.output_dir.resolve(); output.mkdir(parents=True, exist_ok=False)
    fixtures = output / 'fixtures'; fixtures.mkdir()
    payload_environment = dict(os.environ, SCRIPTW_CACHE_DIR=str(output / 'native-cache'),
                               SCRIPTW_COMPILER_CACHE_IMAGE_HASH='sha256')
    selected = [entry for entry in controls() if args.cases is None or entry[0] in args.cases]
    if not selected or (args.cases and {entry[0] for entry in selected} != set(args.cases)):
        parser.error('unknown or empty case selection')
    manifest = dict(schema='scriptw-v8-workloads-v1', execution=[])
    for name, source, expected, kind in selected:
        (fixtures / name).write_text(source + '\n')
        manifest['execution'].append(dict(file=name, stdout=expected, family=kind))
        reference_env = os.environ.copy()
        reference_env.pop('NODE_COMPILE_CACHE', None)
        reference_env['NODE_DISABLE_COMPILE_CACHE'] = '1'
        reference = subprocess.run([args.node, '-e',
            "const fs=require('node:fs'),vm=require('node:vm');vm.runInNewContext(fs.readFileSync(process.argv[1],'utf8'),{print:x=>process.stdout.write(String(x)+'\\n')});",
            str(fixtures / name)], capture_output=True, timeout=30, env=reference_env)
        (fixtures / (name + '.node.stdout')).write_bytes(reference.stdout)
        (fixtures / (name + '.node.stderr')).write_bytes(reference.stderr)
        if reference.returncode or reference.stdout.decode() != expected:
            raise ValueError(f'independent Node checksum differs: {name}')
    (fixtures / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    report = dict(schema='scriptw-policy-break-even-controls-v1', complete=False,
                  runner_sha256=digest(args.runner), time_sha256=digest(args.time),
                  selected_cohorts=args.cohort,
                  observations=[], break_even=[], limitations=[
                      'Descriptive observed medians, not statistical significance or default-policy acceptance.',
                      'P cold compilation excludes R, JS compilation, serialization and Engine creation.',
                      'Execution includes fresh-Realm bootstrap/completion; explicit optimized-R ISA/storage is shared within these cohorts.',
                      'Native inlining changes both the P Engine and its matching optimized R. Its P-only repayment estimate excludes R production and cannot establish whole-program repayment.',
                      'Process RSS includes compiler/image allocator retention and is not live JS heap usage.'])

    def save():
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    save()
    for name, _, expected, _ in selected:
        cohort = {}
        admitted_identity = None
        for cohort_name in args.cohort:
            mode, capacity, allocation, inlining = COHORTS[cohort_name]
            label = f'{name}-{mode}-{capacity}-{allocation}-{inlining}'
            policy = dict(native_mode=mode, gc_initial_mib=capacity, register_allocation=allocation,
                          native_inlining=inlining)
            command = [str(args.runner.resolve()), str(fixtures), name, str(args.samples),
                       str(args.warmups), mode, str(capacity), allocation, inlining,
                       str(output / (label + '-artifacts'))]
            row = dict(label=label, policy=policy, command=command, state='running')
            report['observations'].append(row); save()
            print('Policy control:', label, flush=True)
            with (output / (label + '.json')).open('xb') as stdout, (output / (label + '.stderr')).open('xb') as stderr:
                row['process'] = measure_process(command, cwd=Path.cwd(), env=payload_environment,
                    stdout=stdout, stderr=stderr, timeout=900, time_binary=str(args.time))
            row['state'] = 'closed'; save()
            if row['process']['exit_code'] or row['process']['timed_out']:
                raise ValueError(f'failed policy child: {label}')
            raw = json.loads((output / (label + '.json')).read_text())
            if raw['executable_sha256'] != report['runner_sha256'] or raw['source_sha256'] != digest(fixtures / name):
                raise ValueError('loaded executable/source identity differs')
            if raw['program_sha256'] != digest(output / (label + '-artifacts/program.wasm')):
                raise ValueError('retained P artifact differs')
            if (not re.fullmatch(r'[0-9a-f]{40}', raw['source_commit'] or '')
                    or not re.fullmatch(r'[0-9a-f]{64}', raw['source_fingerprint'])
                    or raw['program_bytes'] != (output / (label + '-artifacts/program.wasm')).stat().st_size):
                raise ValueError('missing source/native artifact authority')
            identity = {field: raw[field] for field in ('source_commit', 'source_fingerprint',
                        'executable_sha256', 'source_sha256', 'program_sha256', 'program_bytes', 'linked_runtime')}
            if admitted_identity is not None and admitted_identity != identity:
                raise ValueError('policy cohorts changed compiler/source/P/R identity')
            admitted_identity = identity
            row['identity'] = identity
            row['summary'] = validate(raw, policy, name, expected, args.samples, args.warmups)
            row['raw_sha256'] = digest(output / (label + '.json'))
            cohort[(mode, capacity, allocation, inlining)] = row['summary']; save()
        for before_key, after_key, comparison in [
                (COHORTS['fast-16-single-pass'], COHORTS['speed-16-single-pass'], 'Fast to Speed'),
                (COHORTS['speed-16-single-pass'], COHORTS['speed-16-bounded-inlining'],
                 'Speed disabled to bounded native inlining')]:
            if before_key in cohort and after_key in cohort:
                report['break_even'].append(dict(file=name, comparison=comparison,
                    **observed_break_even(cohort[before_key], cohort[after_key])))
        save()
    report['complete'] = True; save()


if __name__ == '__main__':
    main()
