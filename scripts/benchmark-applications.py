#!/usr/bin/env python3
"""Bounded fresh-process application measurements; failures remain in the matrix."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess

from application_benchmarks import (ACTORS, SAMPLES, WARMUPS, SCHEMA, checked_identity,
    checked_observations, process_plan, require, successful_process, summarize, validate_manifest)
from benchmark_metrics import digest, measure_process
from compilation_comparison import ADAPTER, process_environment, scriptc_companions
from limited_verification import kernel_group, limited_environment, require_kernel_budget

ROOT = Path(__file__).resolve().parents[1]
HARNESS = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_baseline/node-runtime.cjs'
FIELDS = ('memory.max', 'memory.swap.max', 'memory.oom.group', 'cpu.max', 'memory.events')


def load(path):
    return json.loads(Path(path).read_text())


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def executable(value):
    return Path(shutil.which(str(value)) or value).resolve(strict=True)


def stamp():
    return datetime.now(timezone.utc).isoformat()


def resource_snapshot():
    return {field: (kernel_group() / field).read_text().strip() for field in FIELDS}


def check_resources(before, after):
    require(all(before[field] == after[field] for field in FIELDS if field != 'memory.events'),
            'resource policy changed during child execution')
    left = dict(line.split() for line in before['memory.events'].splitlines())
    right = dict(line.split() for line in after['memory.events'].splitlines())
    require(all(left.get(key) == right.get(key) for key in ('oom', 'oom_kill', 'oom_group_kill')),
            'cgroup OOM event during child execution')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('fixtures', 'frozen', 'scriptc', 'runtime-pack', 'zig', 'time', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--node', default='node')
    parser.add_argument('--timeout', type=float, default=120)
    args = parser.parse_args()
    require_kernel_budget(4 * 1024**3)
    require(len(os.sched_getaffinity(0)) == 1, 'one CPU required')
    require(args.timeout > 0, 'positive child deadline required')
    frozen, fixtures, output = args.frozen.resolve(), args.fixtures.resolve(), args.output.resolve()
    manifest = load(fixtures / 'manifest.json')
    cases = validate_manifest(manifest)
    output.mkdir(parents=True, exist_ok=False)
    for name in ('sources', 'adapted', 'logs', 'work', 'prepared', 'tooling', 'tmp', 'cache'):
        (output / name).mkdir()
    host = frozen / 'candidate/compiler_precompiled_baseline'
    identity = load(frozen / 'candidate/identity.json')
    require(digest(host) == identity['executable_sha256'], 'frozen host digest differs')
    require(identity['shared_configuration']['consumer_compiler_enabled'] is False, 'consumer compiler policy differs')
    producer = load(frozen / 'runtime-product-producer-candidate.json')
    require(producer['status'] == 'verified', 'frozen runtime producer not verified')
    require(digest(frozen / producer['build_stdout']) == producer['build_stdout_sha256'], 'original producer log differs')
    tracked = {host, frozen / 'candidate/identity.json', frozen / 'runtime-product-producer-candidate.json',
               frozen / producer['build_stdout'], fixtures / 'manifest.json', HARNESS}
    for name, receipt in producer['files'].items():
        binary = frozen / 'candidate/runtime-build' / name
        require(digest(binary) == receipt['sha256'], 'frozen runtime differs from original producer: ' + name)
        if 'package' in receipt:
            require(receipt['package']['package_source_fingerprint'] == identity['source_fingerprint'],
                    'runtime source fingerprint differs')
        tracked.add(binary)
    witness_path = frozen / 'publication-rust-witness.json'
    witness = load(witness_path)
    tracked.add(witness_path)
    for name, sha in witness['files'].items():
        require(digest(ROOT / name) == sha, 'frozen source witness differs: ' + name)
        tracked.add(ROOT / name)
    qualification_path = frozen / 'native-qualified.json'
    qualification = load(qualification_path)
    require(qualification['status'] == 'complete'
            and qualification['source_fingerprint'] == identity['source_fingerprint'], 'native qualification differs')
    tracked.add(qualification_path)
    scriptc, zig, timer, node_path = map(executable, (args.scriptc, args.zig, args.time, args.node))
    runtime_pack = args.runtime_pack.resolve(strict=True)
    pack = load(runtime_pack / 'runtime-pack.json')
    require(pack['version'] == '0.4.5' and pack['target']['name'] == 'wasm32-wasi', 'Scriptc runtime pack pin differs')
    companion_manifest = Path(str(scriptc) + '.json')
    tracked.update((scriptc, zig, timer, node_path, companion_manifest))
    for selected in [runtime_pack, *map(Path, scriptc_companions(scriptc, load(companion_manifest)).values())]:
        require(selected.exists(), 'missing Scriptc companion: ' + str(selected))
        if selected.is_dir():
            tracked.update(path for path in selected.rglob('*') if path.is_file())
        else:
            tracked.add(selected)
    env = limited_environment()
    for key in ('NODE_OPTIONS', 'LD_PRELOAD', 'LD_AUDIT'):
        require(not env.get(key), 'forbidden inherited setting: ' + key)
    for key in list(env):
        if key.startswith(('SCRIPTW_', 'SCRIPTC_', 'ZIG_')) or key == 'NODE_COMPILE_CACHE':
            del env[key]
    env.update(SCRIPTW_JOBS='1', SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES='1',
        SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES=str(64 * 1024**2), NODE_DISABLE_COMPILE_CACHE='1',
        TMPDIR=str(output / 'tmp'), SCRIPTW_CACHE_DIR=str(output / 'cache'),
        PATH=str(zig.parent) + os.pathsep + env['PATH'])
    require(executable(shutil.which('zig', path=env['PATH'])) == zig, 'Zig lookup differs')
    tools = {name: dict(path=str(path), sha256=digest(path), bytes=path.stat().st_size)
             for name, path in (('host', host), ('scriptc', scriptc), ('zig', zig), ('time', timer), ('node', node_path))}
    # Tool identity commands are bounded diagnostics, outside every entry/process sample.
    for name, command in (('host', [host, 'identity']), ('scriptc', [scriptc, '--version']),
                          ('zig', [zig, 'version']), ('node', [node_path, '-p', 'JSON.stringify(process.versions)'])):
        result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, timeout=30, check=True)
        (output / 'logs' / (name + '-identity.stdout')).write_bytes(result.stdout)
        (output / 'logs' / (name + '-identity.stderr')).write_bytes(result.stderr)
        tools[name]['version_stdout'] = result.stdout.decode().strip()
    checked_identity(json.loads(tools['host']['version_stdout']), identity)
    require(tools['scriptc']['version_stdout'].removeprefix('scriptc ').strip() == '0.4.5'
            and tools['zig']['version_stdout'] == '0.16.0', 'Scriptc/Zig versions differ')
    node = dict(versions=json.loads(tools['node']['version_stdout']), **tools['node'])
    shutil.copyfile(fixtures / 'manifest.json', output / 'sources/manifest.json')
    for case in cases:
        name = case['file']
        shutil.copyfile(fixtures / name, output / 'sources' / name)
        (output / 'adapted' / name).write_bytes(ADAPTER.encode() + (fixtures / name).read_bytes())
        tracked.update((fixtures / name, output / 'sources' / name, output / 'adapted' / name))
    for name in ('benchmark-applications.py', 'application_benchmarks.py', 'benchmark_metrics.py',
                 'compilation_comparison.py', 'compiler_comparison.py', 'limited_verification.py', 'resource_numbers.py'):
        source = ROOT / 'scripts' / name
        shutil.copyfile(source, output / 'tooling' / name)
        tracked.add(source)
    shutil.copyfile(HARNESS, output / 'tooling' / HARNESS.name)
    for name, source in (('identity.json', frozen / 'candidate/identity.json'),
                         ('runtime-producer.json', frozen / 'runtime-product-producer-candidate.json'),
                         ('rust-witness.json', witness_path), ('runtime-pack.json', runtime_pack / 'runtime-pack.json'),
                         ('native-qualified.json', qualification_path),
                         ('scriptc-toolchain.json', companion_manifest)):
        shutil.copyfile(source, output / 'tooling' / name)
    tracked.update(path for path in (output / 'tooling').iterdir() if path.is_file())
    tracked.add(output / 'sources/manifest.json')
    before = {str(path): digest(path) for path in sorted(tracked)}
    save(output / 'inputs-before.json', before)
    report = dict(schema=SCHEMA, status='incomplete', started_at=stamp(), workloads=cases,
        source_fingerprint=identity['source_fingerprint'], source_commit=identity['source_commit'],
        measurement_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        samples=SAMPLES, warmups=WARMUPS,
        tools=tools, node_identity=node, consumer_identity=identity, preparations={}, rows=[], inputs_before=before,
        policy=dict(entry='Public entry wall: Scriptw evaluate, Scriptc WASI _start, V8 runInContext; output capture included.',
            setup='Image loading/Store/instance setup excluded from entry. V8 source compile and fresh vm.Context creation excluded and separately retained. Scriptw Realm bootstrap stays inside entry.',
            process='Fresh-process wall includes launch, loading, setup, all three warmups and seven entries. It is not single-invocation startup latency.',
            memory='GNU-time whole-child peak RSS includes runtime, setup and all invocation history; not live JavaScript heap.',
            preparation='Scriptw source compilation/emission combined, native preparation separate in producer receipt. Scriptc source-to-Wasm and Wasm-to-native separate child processes. These are single diagnostics, not execution samples.',
            statistics='Two balanced blocks; four fresh processes per available actor/case/block; process is the unit. No cross-runtime CPU ratios, speed acceptance or conformance claim.',
            failures='Every selected case has all three actor statuses. Failures retain their command stage, raw logs, exit code and timeout. Missing cases are never speed wins or permanent skips.',
            source='Original synchronous print fixtures shared by Scriptw/V8; Scriptc alone receives canonical print adapter.',
            resources='Sequential children; one CPU; at most4GiB; no swap; grouped OOM; module cache1/64MiB.'),
        machine=dict(affinity=sorted(os.sched_getaffinity(0)), resources=resource_snapshot()), timeout_seconds=args.timeout)

    def invoke(stem, stage, argv, child_env=env, cwd=ROOT):
        stdout_path, stderr_path = output / 'logs' / (stem + '.stdout'), output / 'logs' / (stem + '.stderr')
        record = dict(stage=stage, argv=list(map(str, argv)), stdout=str(stdout_path.relative_to(output)),
                      stderr=str(stderr_path.relative_to(output)),
                      environment={key: value for key, value in child_env.items()
                                   if key.startswith(('SCRIPTW_', 'SCRIPTC_', 'ZIG_', 'NODE_')) or key == 'TMPDIR'})
        record['resources'] = dict(exit_code=None, timed_out=False, not_launched=True)
        resources_before = resource_snapshot()
        try:
            with stdout_path.open('xb') as stdout, stderr_path.open('xb') as stderr:
                record['resources'] = measure_process(argv, cwd=cwd, env=child_env, stdout=stdout,
                    stderr=stderr, timeout=args.timeout, time_binary=timer)
            check_resources(resources_before, resource_snapshot())
            require(successful_process(record['resources']), 'child failed or timed out')
            record['status'] = 'succeeded'
        except (ValueError, OSError, subprocess.SubprocessError) as error:
            record.update(status='failed', reason=str(error))
        record['stdout_sha256'], record['stderr_sha256'] = digest(stdout_path), digest(stderr_path)
        save(output / 'logs' / (stem + '.json'), record)
        return record

    def checked_json(record):
        require(record['status'] == 'succeeded', record.get('reason', 'failed command'))
        return load(output / record['stdout'])

    images = {}
    for number, case in enumerate(cases):
        name = case['file']
        images[name] = {}
        report['preparations'][name] = {}
        for actor in ACTORS:
            preparation = dict(status='failed', stage='preparation', commands=[])
            report['preparations'][name][actor] = preparation
            try:
                if actor == 'v8':
                    preparation.update(status='prepared', stage='execution',
                        scope='vm.Script compilation measured separately inside each fresh Node process')
                    continue
                stem = f'prepare-{number:02}-{actor}'
                if actor == 'scriptw':
                    preparation.update(stage='source-emission-native-preparation', phase_precision='combined-host-command')
                    single = output / 'work' / stem
                    single.mkdir()
                    shutil.copyfile(output / 'sources' / name, single / name)
                    save(single / 'manifest.json', dict(execution=[case]))
                    destination = output / 'prepared' / stem
                    command = invoke(stem, preparation['stage'], [host, 'prepare-scriptw', single, destination])
                    preparation['commands'].append(command)
                    receipt = checked_json(command)
                    checked_identity(receipt, identity)
                    require(receipt['manifest_sha256'] == digest(single / 'manifest.json'), 'producer manifest differs')
                    require(len(receipt['cases']) == 1, 'producer case count differs')
                    image = receipt['cases'][0]
                    require(image['file'] == name and image['stdout'] == case['stdout']
                            and image['source_sha256'] == digest(output / 'sources' / name), 'producer source/output differs')
                    require(image['source_compilation_profile'] == 'portable' and image['intl_profile'] == 'Minimal',
                            'Scriptw producer source compilation profile differs')
                    image_path = destination / image['image']
                    wasm_path = destination / image['wasm']
                    require(digest(wasm_path) == image['program_wasm_sha256'], 'emitted program differs')
                    preparation['phases_ns'] = image['phases_ns']
                    tracked.update((single / name, single / 'manifest.json', wasm_path, destination / 'producer.json'))
                else:
                    preparation['stage'] = 'source-to-wasm'
                    work = output / 'work' / stem
                    work.mkdir()
                    for folder in ('tmp', 'cache', 'zig-local'):
                        (work / folder).mkdir()
                    source = work / name
                    shutil.copyfile(output / 'adapted' / name, source)
                    wasm_path = work / 'program.wasm'
                    child_env = dict(env, **process_environment(work, runtime_pack,
                        Path(os.environ.get('ZIG_GLOBAL_CACHE_DIR', str(Path.home() / '.cache/zig'))).resolve()))
                    command = invoke(stem + '-emit', preparation['stage'], [scriptc, 'build', source, '-o', wasm_path], child_env, work)
                    preparation['commands'].append(command)
                    require(command['status'] == 'succeeded', command.get('reason', 'source compilation failed'))
                    require(wasm_path.read_bytes()[:8] == b'\0asm\1\0\0\0', 'Scriptc did not emit portable Wasm')
                    preparation['stage'] = 'wasm-to-native'
                    image_path = output / 'prepared' / (stem + '.swpc')
                    command = invoke(stem + '-native', preparation['stage'], [host, 'prepare-wasi', wasm_path, image_path])
                    preparation['commands'].append(command)
                    receipt = checked_json(command)
                    checked_identity(receipt, identity)
                    image = receipt
                    require(image['wasm_sha256'] == digest(wasm_path), 'WASI producer input differs')
                    preparation.update(phases_ns=image['phases_ns'], adapted_source_sha256=digest(source))
                    tracked.add(source)
                require(digest(image_path) == image['image_sha256'], 'producer native image differs')
                images[name][actor] = dict(image, path=str(image_path))
                preparation.update(status='prepared', stage='execution', image=images[name][actor])
                tracked.update((image_path, wasm_path))
            except (ValueError, KeyError, OSError, TypeError) as error:
                preparation.update(status='failed', reason=str(error))
            save(output / 'incomplete-report.json', report)
    available = {case['file']: [actor for actor in ACTORS
        if report['preparations'][case['file']][actor]['status'] == 'prepared'] for case in cases}
    order = process_plan([case['file'] for case in cases], available)
    report['plan'] = dict(available=available, process_plan=order, samples=SAMPLES, warmups=WARMUPS)
    save(output / 'plan.json', report['plan'])
    # Native images enter the tracked set only after this run's original producer has returned.
    prepared_hashes = {str(path): digest(path) for path in sorted(tracked) if str(path) not in before}
    before.update(prepared_hashes)
    report['inputs_before'] = before
    save(output / 'inputs-before.json', before)
    by_name = {case['file']: case for case in cases}
    for number, entry in enumerate(order):
        actor, name = entry['actor'], entry['case']
        case = by_name[name]
        stem = f'execute-{number:04}-{actor}-{name}'
        print(f'{number + 1}/{len(order)} {actor} {name}', flush=True)
        image = images[name].get(actor)
        if actor == 'v8':
            argv = [node_path, HARNESS, output / 'sources', str(SAMPLES), str(WARMUPS), name]
        else:
            action = 'execute-scriptw'
            if actor == 'scriptc':
                action = 'execute-wasi'
            argv = [host, action, image['path'], case['stdout'], str(SAMPLES), str(WARMUPS)]
            require(digest(image['path']) == image['image_sha256'], 'native image changed before execution')
        command = invoke(stem, 'execution', argv)
        row = dict(entry, **command)
        try:
            raw = checked_json(command)
            row.update(checked_observations(raw, actor, case, digest(output / 'sources' / name), identity, image, node))
            require(not (output / command['stderr']).read_bytes(), 'execution stderr is nonempty')
            row['status'] = 'sampled'
        except (ValueError, KeyError, OSError, TypeError) as error:
            row.update(status='failed', reason=str(error))
        report['rows'].append(row)
        save(output / 'incomplete-report.json', report)
    after = {str(path): digest(path) for path in sorted(tracked)}
    save(output / 'inputs-after.json', after)
    report['inputs_after'] = after
    require(before == after, 'source, tool or prepared image changed during campaign')
    report.update(status='complete', finished_at=stamp())
    report['summary'] = summarize(report)
    # Inert evidence only: executable images stay local with their producer-bound digests.
    report['retained_files'] = [dict(path=str(path.relative_to(output)), bytes=path.stat().st_size, sha256=digest(path))
        for folder in ('sources', 'adapted', 'logs', 'tooling') for path in sorted((output / folder).rglob('*')) if path.is_file()]
    save(output / 'report.json', report)
    print(output / 'report.json', flush=True)


if __name__ == '__main__':
    main()
