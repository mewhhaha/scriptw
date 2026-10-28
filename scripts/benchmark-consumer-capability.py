#!/usr/bin/env python3
"""Compare two locally built consumer capabilities using identical trusted images.

Run through limited_verification.py. Producer directories must belong to the
caller's trusted local build: hashes bind measurements, never authorize native
code. This diagnostic cannot publish the canonical precompiled comparison.
"""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import sys
import time

import limited_verification as budget
import precompiled_comparison as shared

COHORT = 'scriptw-consumer-capability-diagnostic-v1'
VARIANTS = ('enabled', 'disabled')
IDENTITY = ('source_commit', 'source_fingerprint', 'compiler_identity_scheme')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    require(path.stat().st_size <= shared.MAX_REPORT, f'report too large: {path}')
    return shared._json(path.read_bytes())


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def check_identity(raw, variant, digest, reference=None):
    require(raw.get('comparison_cohort') == COHORT
            and raw.get('excluded_from_scored_cohorts') is True
            and raw.get('excluded_from_ordinary_cohorts') is True,
            'consumer must identify this excluded diagnostic')
    require(raw.get('native_source_compiler_enabled') is (variant == 'enabled'),
            'consumer capability differs from requested variant')
    require(raw.get('executable_sha256') == digest, 'consumer executable identity differs')
    require(shared._hex(raw.get('source_commit'), 40)
            and shared._hex(raw.get('source_fingerprint'))
            and isinstance(raw.get('compiler_identity_scheme'), str), 'missing compiler identity')
    config = raw.get('shared_configuration', {})
    require(all(config.get(k) == v for k, v in shared.SHARED_CONFIGURATION.items()),
            'consumer configuration differs from the paired policy')
    require(isinstance(config.get('target_triple'), str) and config['target_triple'],
            'missing native target triple')
    if reference is not None:
        require(all(raw.get(k) == reference[k] for k in IDENTITY)
                and config == reference['shared_configuration'],
                'consumer source or configuration differs between capabilities')


def plan(cases, adapters):
    names = [case['file'] for case in cases]
    return [dict(round=round_index, file=name, adapter=adapter, variant=variant)
            for round_index in range(2)
            for name in (names if round_index == 0 else names[::-1])
            for adapter_index, adapter in enumerate(adapters)
            for variant in (VARIANTS if (names.index(name) + adapter_index + round_index) % 2 == 0
                            else VARIANTS[::-1])]


def admit_images(directory, cases, manifest_digest, reference, adapter):
    producer = read_json(directory / 'producer.json')
    require(all(producer.get(k) == reference[k] for k in IDENTITY),
            f'{adapter} producer source identity differs from consumers')
    require(producer.get('manifest_sha256') == manifest_digest, 'producer fixture manifest differs')
    if 'shared_configuration' in producer:
        require(producer['shared_configuration'] == reference['shared_configuration'],
                'producer shared configuration differs')
    rows = producer.get('cases', [])
    require(isinstance(rows, list) and all(isinstance(row, dict) for row in rows),
            'producer cases must be records')
    by_name = {row['file']: row for row in rows}
    require(len(by_name) == len(rows), 'duplicate producer case')
    admitted = {}
    for case in cases:
        row = by_name.get(case['file'])
        require(row is not None and row.get('stdout') == case['stdout']
                and row.get('source_sha256') == case['source_sha256'],
                f'producer source/checksum differs: {case["file"]}')
        relative = shared.safe_name(row['image'])
        path = (directory / relative).resolve()
        require(path.is_relative_to(directory.resolve()) and path.is_file(), 'image escaped producer directory')
        require(path.stat().st_size <= shared.MAX_EXPANDED, 'native image exceeds diagnostic bound')
        metadata = shared.image_metadata(path.read_bytes(), 'scriptw' if adapter == 'scriptw' else 'scriptc',
                                         reference['source_fingerprint'])
        require(metadata['sha256'] == row.get('image_sha256')
                and metadata['program_wasm_sha256'] == row.get('program_wasm_sha256'),
                'image bytes differ from producer receipt')
        if adapter == 'scriptw':
            require(row.get('source_compilation_profile') == 'portable'
                    and metadata.get('runtime_key') == row.get('runtime_key'),
                    'Scriptw producer must preserve Portable runtime identity')
        admitted[case['file']] = dict(metadata, path=str(path))
    return admitted


def check_execution(raw, variant, host, reference, image, case, adapter):
    check_identity(raw, variant, host['sha256'], reference)
    require(raw.get('compiler_enabled') is False and raw.get('samples') == 7
            and raw.get('warmups') == 10 and 'phase_barriers' not in raw,
            'consumer compilation, samples or diagnostic barriers differ')
    schema = 'scriptw-precompiled-execution-v1' if adapter == 'scriptw' else 'scriptw-precompiled-wasi-execution-v1'
    require(raw.get('schema') == schema, 'wrong execution adapter')
    for key in ('image_bytes', 'image_sha256', 'program_wasm_sha256'):
        expected = image[{'image_bytes': 'bytes', 'image_sha256': 'sha256'}.get(key, key)]
        require(raw.get(key) == expected, f'consumer image mismatch: {key}')
    if adapter == 'scriptw':
        require(raw.get('runtime_key') == image['runtime_key'], 'runtime image key differs')
        for key in ('execution_facts_bytes', 'execution_facts_sha256'):
            if key in image:
                require(raw.get(key) == image[key], f'execution registry differs: {key}')
    observations = raw.get('observations')
    require(isinstance(observations, list) and len(observations) == 17,
            'every warmup and measured invocation must be retained')
    measured = []
    for index, row in enumerate(observations):
        require(type(row.get('index')) is int and row['index'] == index
                and row.get('warmup') is (index < 10), 'observation order differs')
        require(row.get('stdout') == case['stdout'] and row.get('completion') == 'normal'
                and row.get('stderr', '') == '', 'original output checksum/completion failed')
        if 'matched' in row:
            require(row['matched'] is True, 'consumer reported mismatched output')
        timings = row.get('phases_ns', {})
        require(set(timings) == set(shared.TIME_PHASES)
                and all(shared._uint(v) for v in timings.values())
                and timings['execution'] > 0
                and timings['total'] >= sum(v for k, v in timings.items() if k != 'total'),
                'invalid execution phase times')
        snapshots = row.get('process_snapshots', {})
        require(set(snapshots) == set(shared.PROCESS_PHASES), 'missing Store lifecycle snapshots')
        for memory in snapshots.values():
            shared._memory(memory)
        require(all(shared._uint(row.get(k)) for k in ('gc_capacity_before_bytes', 'gc_capacity_after_bytes')),
                'missing GC capacity observations')
        if index >= 10:
            measured.append(dict(execution_ns=timings['execution'], **snapshots['after_execution']))
    return measured


def execute(command, output, index, deadline, environment):
    stdout = output / f'{index:03}.stdout'
    stderr = output / f'{index:03}.stderr'
    start = time.monotonic()
    expired = False
    with stdout.open('xb') as out, stderr.open('xb') as err:
        child = subprocess.Popen(command, stdout=out, stderr=err, env=environment, start_new_session=True)
        try:
            child.wait(timeout=deadline)
        except subprocess.TimeoutExpired:
            expired = True
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
    return dict(argv=command, returncode=child.returncode, timed_out=expired,
                elapsed_seconds=time.monotonic() - start, stdout=stdout.name, stderr=stderr.name)


def resources():
    group = budget.kernel_group()
    return {key: (group / key).read_text().strip() for key in
            ('memory.max', 'memory.swap.max', 'memory.oom.group', 'memory.peak', 'memory.events')}


def run(args):
    budget.require_kernel_budget(4 * 1024**3)
    require(not any(name in os.environ for name in shared.PHASE_BARRIER_ENV), 'phase barriers must be absent')
    args.output.mkdir(parents=True, exist_ok=False)
    output = args.output
    environment = budget.limited_environment()
    report = dict(schema=COHORT, excluded_from_scored_cohorts=True, excluded_from_ordinary_cohorts=True,
                  started=datetime.now(timezone.utc).isoformat(), finished=False, samples=7, warmups=10, rounds=2,
                  scope='same-image consumer capability experiment; compiler dependencies remain; process endpoints, not live heap or peak',
                  resources=dict(cpu_affinity=sorted(os.sched_getaffinity(0)), before=resources()),
                  commands=[], attempts=[], hosts={}, failures=[])
    save = lambda: write_json(output / 'report.json', report)
    save()
    try:
        manifest_path = args.fixtures / 'manifest.json' if args.fixtures.is_dir() else args.fixtures
        manifest = read_json(manifest_path)
        cases = manifest['execution']
        require(cases and len({case['file'] for case in cases}) == len(cases), 'empty/duplicate fixture inventory')
        if args.case:
            require(len(set(args.case)) == len(args.case)
                    and set(args.case) <= {c['file'] for c in cases}, 'unknown/repeated selected case')
            cases = [case for case in cases if case['file'] in args.case]
        for case in cases:
            require(Path(case['file']).name == case['file'] and isinstance(case['stdout'], str), 'invalid fixture')
            case['source_sha256'] = shared.digest(manifest_path.parent / case['file'])
        report['cases'] = cases
        report['manifest_sha256'] = shared.digest(manifest_path)
        reference = None
        for variant in VARIANTS:
            path = getattr(args, variant + '_host').resolve()
            host = dict(path=str(path), sha256=shared.digest(path))
            command = execute([str(path), 'identity'], output, len(report['commands']), args.timeout, environment)
            report['commands'].append(command)
            save()
            require(command['returncode'] == 0 and not command['timed_out'], f'{variant} identity failed')
            raw = read_json(output / command['stdout'])
            check_identity(raw, variant, host['sha256'], reference)
            host['identity'] = raw
            report['hosts'][variant] = host
            reference = raw if reference is None else reference
        adapters = args.adapter or (['scriptw', 'wasi'] if args.wasi_images else ['scriptw'])
        require(len(set(adapters)) == len(adapters), 'repeated adapter')
        images = {}
        for adapter in adapters:
            directory = getattr(args, adapter + '_images')
            require(directory is not None, f'missing {adapter} producer directory')
            images[adapter] = admit_images(directory, cases, report['manifest_sha256'], reference, adapter)
        report['images'] = images
        report['plan'] = plan(cases, adapters)
        by_name = {case['file']: case for case in cases}
        for step in report['plan']:
            case = by_name[step['file']]
            host = report['hosts'][step['variant']]
            image = images[step['adapter']][step['file']]
            command = execute([host['path'], 'execute-' + step['adapter'], image['path'], case['stdout'], '7', '10'],
                              output, len(report['commands']), args.timeout, environment)
            command.update(step)
            report['commands'].append(command)
            attempt = dict(step, admitted=False, command_index=len(report['commands']) - 1)
            report['attempts'].append(attempt)
            try:
                require(command['returncode'] == 0 and not command['timed_out'], 'consumer process failed/timed out')
                raw = read_json(output / command['stdout'])
                attempt['measured'] = check_execution(raw, step['variant'], host, reference, image, case, step['adapter'])
                attempt['admitted'] = True
            except (ValueError, KeyError, OSError, TypeError) as error:
                attempt['failure'] = str(error)
                report['failures'].append(dict(step, error=str(error)))
            save()
            print(json.dumps({k: v for k, v in attempt.items() if k != 'measured'}), flush=True)
        for host in report['hosts'].values():
            require(shared.digest(host['path']) == host['sha256'], 'host changed during measurement')
        for adapter in images.values():
            for image in adapter.values():
                require(shared.digest(image['path']) == image['sha256'], 'native image changed during measurement')
        report['resources']['after'] = resources()
        shared._resources(dict(resources=report['resources'], machine=dict(measurement_affinity=report['resources']['cpu_affinity'])))
        rows = []
        for case in cases:
            for adapter in adapters:
                row = dict(file=case['file'], adapter=adapter)
                for variant in VARIANTS:
                    attempts = [a for a in report['attempts'] if a['file'] == case['file'] and a['adapter'] == adapter and a['variant'] == variant]
                    if len(attempts) != 2 or not all(a['admitted'] for a in attempts):
                        row[variant] = dict(status='failed', samples=0)
                        continue
                    values = [v for a in attempts for v in a['measured']]
                    row[variant] = dict(status='ok', samples=len(values), execution_ms=statistics.median(v['execution_ns'] for v in values) / 1e6,
                                        **{k: statistics.median(v[k] for v in values) for k in shared.MEMORY_FIELDS})
                rows.append(row)
        report['rows'] = rows
        report['finished'] = True
        report['passed'] = not report['failures']
        report['finished_at'] = datetime.now(timezone.utc).isoformat()
    except (ValueError, KeyError, OSError, TypeError) as error:
        report['failures'].append(dict(error=str(error)))
        report['passed'] = False
        save()
        raise
    save()
    lines = ['# Consumer capability diagnostic', '',
             'Identical trusted native images; two rounds, ten warmups and seven samples per round.',
             'Process endpoints include runtime, host and allocation history. Compiler dependencies remain.',
             'Excluded from canonical performance publications.', '',
             '| Workload / adapter | Capability | Execution ms | RSS MiB | PSS MiB | Private dirty MiB |',
             '| --- | --- | ---: | ---: | ---: | ---: |']
    for row in report['rows']:
        for variant in VARIANTS:
            metrics = row[variant]
            cells = ([f'{metrics["execution_ms"]:.3f}'] +
                     [f'{metrics[k] / 1024**2:.3f}' for k in shared.MEMORY_FIELDS]
                     if metrics['status'] == 'ok' else ['failed'] * 4)
            lines.append('| ' + ' | '.join([row['file'] + ' / ' + row['adapter'], variant, *cells]) + ' |')
    (output / 'summary.md').write_text('\n'.join(lines) + '\n')
    return 0 if report['passed'] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('enabled-host', 'disabled-host', 'scriptw-images', 'fixtures', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--wasi-images', type=Path)
    parser.add_argument('--adapter', action='append', choices=('scriptw', 'wasi'))
    parser.add_argument('--case', action='append')
    parser.add_argument('--timeout', type=int, default=600, help='deadline in seconds for each fresh process')
    args = parser.parse_args()
    if args.timeout < 1:
        parser.error('timeout must be positive')
    return run(args)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (ValueError, KeyError, OSError, TypeError) as error:
        raise SystemExit('consumer-capability: ' + str(error))
