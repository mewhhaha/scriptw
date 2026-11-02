#!/usr/bin/env python3
"""Measure original workloads with a qualified frozen Wasm host and normal V8."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess

from benchmark_metrics import digest, execution_thread_cpu, measure_process
from limited_verification import kernel_group, limited_environment, require_kernel_budget

ROOT = Path(__file__).resolve().parents[1]
RUNTIMES = ('scriptw', 'scriptc', 'v8')
LETTERS = 'ABCBCACABACB'
SAMPLES, WARMUPS = 21, 10
FIELDS = ('memory.max', 'memory.swap.max', 'memory.oom.group', 'cpu.max', 'memory.events')
HARNESS = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_baseline/node-runtime.cjs'
FIXTURES = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths'


def load(path):
    return json.loads(Path(path).read_text())


def save(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def stamp():
    return datetime.now(timezone.utc).isoformat()


def process_plan(names):
    assert len(names) == len(set(names)) == 17
    result = []
    for block in range(2):
        ordered, letters = list(enumerate(names)), LETTERS
        if block == 1:
            ordered.reverse()
            letters = letters[::-1]
        for index, name in ordered:
            offset = index % 3
            actors = RUNTIMES[offset:] + RUNTIMES[:offset]
            for position, letter in enumerate(letters):
                result.append(dict(block=block, case=name, position=position, actor=actors['ABC'.index(letter)]))
    return result


def checked_observations(report, actor, case, source_sha, identity, image, node, samples, warmups):
    if actor == 'v8':
        assert report['schema'] == 'scriptw-v8-baseline-v2'
        assert report['node'] == 'v' + node['versions']['node'] and report['v8'] == node['versions']['v8']
        assert len(report['cases']) == 1
        value = report['cases'][0]
        assert value['file'] == case['file'] and value['source_sha256'] == source_sha
        assert type(value['compilation_ns']) is int and value['compilation_ns'] >= 0
        observations = value['observations']
    else:
        expected_schema = 'scriptw-precompiled-execution-v1'
        if actor == 'scriptc':
            expected_schema = 'scriptw-precompiled-wasi-execution-v1'
        assert report['schema'] == expected_schema and report['compiler_enabled'] is False
        for key in ('source_fingerprint', 'executable_sha256', 'shared_configuration'):
            assert report[key] == identity[key]
        assert report['image_sha256'] == image['image_sha256']
        assert report['comparison_cohort'] == 'scriptw-precompiled-comparison-v1'
        assert report['excluded_from_ordinary_cohorts'] is True
        if actor == 'scriptw':
            assert report['program_wasm_sha256'] == image['program_wasm_sha256']
            assert report['runtime_key'] == image['runtime_key']
        else:
            assert report['wasm_sha256'] == image['wasm_sha256']
        observations = report['observations']
    assert report['samples'] == samples and report['warmups'] == warmups
    assert len(observations) == samples + warmups
    assert [row['warmup'] for row in observations] == [True] * warmups + [False] * samples
    for row in observations:
        assert row['stdout'] == case['stdout']
        if actor == 'v8':
            assert type(row['execution_ns']) is int and row['execution_ns'] > 0
            assert type(row['context_ns']) is int and row['context_ns'] > 0
        else:
            assert row['matched'] is True and row['completion'] == 'normal' and row['stderr'] == ''
            cpu = execution_thread_cpu(row)
            assert cpu['status'] == 'sampled' and cpu['scope'] == 'execution-worker-thread'
            assert type(row['phases_ns']['execution']) is int and row['phases_ns']['execution'] > 0
    measured = observations[warmups:]
    if actor == 'v8':
        return dict(wall_ns=[row['execution_ns'] for row in measured],
                    context_ns=[row['context_ns'] for row in measured], compilation_ns=value['compilation_ns'])
    return dict(wall_ns=[row['phases_ns']['execution'] for row in measured],
                worker_cpu_ns=[row['execution_thread_cpu']['elapsed_ns'] for row in measured],
                phases_ns=[row['phases_ns'] for row in measured])


def reduce_rows(rows, names):
    assert [{key: row[key] for key in ('block', 'case', 'position', 'actor')} for row in rows] == process_plan(names)
    result = []
    for name in names:
        blocks = []
        for block in range(2):
            products = {}
            for actor in RUNTIMES:
                group = [row for row in rows if (row['block'], row['case'], row['actor']) == (block, name, actor)]
                assert len(group) == 4
                products[actor] = dict(
                    wall_ns=dict(median=statistics.median(statistics.median(row['wall_ns']) for row in group),
                                 mean=statistics.mean(statistics.mean(row['wall_ns']) for row in group)),
                    rss_bytes=dict(median=statistics.median(row['resources']['peak_rss_bytes'] for row in group),
                                   mean=statistics.mean(row['resources']['peak_rss_bytes'] for row in group)))
            blocks.append(dict(block=block, products=products))
        result.append(dict(case=name, blocks=blocks))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--frozen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--node', default='node')
    parser.add_argument('--time', type=Path, required=True)
    parser.add_argument('--qualification-key', default='private_numeric_facts',
                        help='Published qualification entry in performance-gaps-20261101.json')
    args = parser.parse_args()
    require_kernel_budget(4 * 1024**3)
    assert len(os.sched_getaffinity(0)) == 1
    frozen, output = args.frozen.resolve(), args.output.resolve()
    output.mkdir(exist_ok=False)
    for name in ('tmp', 'sources', 'raw', 'preflight', 'candidate', 'tooling'):
        (output / name).mkdir()
    os.environ['TMPDIR'] = str(output / 'tmp')
    env = limited_environment()
    for key in ('NODE_OPTIONS', 'NODE_COMPILE_CACHE', 'LD_PRELOAD', 'LD_AUDIT'):
        assert not env.get(key), f'forbidden inherited setting: {key}'
    assert not any(key.startswith('SCRIPTW_PRECOMPILED_PHASE_') or key == 'SCRIPTW_PRECOMPILED_EXECUTION_WINDOWS' for key in env)
    env.update(NODE_DISABLE_COMPILE_CACHE='1', TMPDIR=str(output / 'tmp'), SCRIPTW_CACHE_DIR=str(output / 'cache'))
    node_path = Path(shutil.which(args.node) or args.node).resolve()
    node = dict(path=str(node_path), executable_sha256=digest(node_path),
                versions=json.loads(subprocess.check_output([str(node_path), '-p', 'JSON.stringify(process.versions)'], env=env, text=True)),
                harness=str(HARNESS.relative_to(ROOT)), harness_sha256=digest(HARNESS), flags=[], persistent_compile_cache=False)
    identity = load(frozen / 'candidate/identity.json')
    host = frozen / 'candidate/compiler_precompiled_baseline'
    assert digest(host) == identity['executable_sha256']
    native = load(frozen / 'native-qualified.json')
    assert native['status'] == 'complete' and native['source_fingerprint'] == identity['source_fingerprint']
    witness = load(frozen / 'publication-rust-witness.json')
    assert all(digest(ROOT / name) == expected for name, expected in witness['files'].items())
    authority = load(ROOT / 'docs/rust-rewrite/performance-gaps-20261101.json')[args.qualification_key]
    publication = authority['publication_commit']
    assert not subprocess.check_output(['git', 'diff', '--name-only', publication, '--', 'crates', 'Cargo.toml', 'Cargo.lock', '.cargo', 'rust-toolchain.toml'], cwd=ROOT)
    assert authority['source_fingerprint'] == identity['source_fingerprint']
    assert all(row['aa_gate'] == 'pass' for row in authority['aa_gates'].values())
    manifest = load(FIXTURES / 'manifest.json')
    cases = manifest['execution']
    names = [row['file'] for row in cases]
    assert len(names) == 17
    save(output / 'sources/manifest.json', manifest)
    save(output / 'candidate/identity.json', identity)
    scriptw = load(frozen / 'prepared/candidate/producer-receipt.json')
    scriptc = load(frozen / 'prepared-scriptc/candidate/producer-receipt.json')
    assert scriptw['product_identity'] == scriptc['product_identity'] == identity
    assert scriptw['status'] == scriptc['status'] == 'prepared'
    images = dict(scriptw=scriptw['image_files'], scriptc=scriptc['cases'])
    paths = [Path(__file__), HARNESS, ROOT / 'scripts/benchmark_metrics.py', ROOT / 'scripts/limited_verification.py',
             node_path, args.time.resolve(), host, frozen / 'candidate/identity.json',
             frozen / 'native-qualified.json', frozen / 'publication-rust-witness.json',
             frozen / 'prepared/candidate/producer-receipt.json', frozen / 'prepared-scriptc/candidate/producer-receipt.json',
             frozen / 'runtime-product-producer-candidate.json', frozen / 'examples-build.stdout', FIXTURES / 'manifest.json']
    paths += list((frozen / 'candidate/runtime-build').glob('*.bin'))
    assert len(list((frozen / 'candidate/runtime-build').glob('*.bin'))) == 4
    for source in (Path(__file__), HARNESS, ROOT / 'scripts/benchmark_metrics.py', ROOT / 'scripts/limited_verification.py'):
        destination = output / 'tooling' / source.name
        shutil.copyfile(source, destination)
        paths.append(destination)
    for case in cases:
        name = case['file']
        assert Path(name).name == name
        source = FIXTURES / name
        shutil.copyfile(source, output / 'sources' / name)
        for actor in ('scriptw', 'scriptc'):
            row = images[actor][name]
            assert row['source_sha256'] == digest(source)
            if actor == 'scriptw':
                assert row['stdout'] == case['stdout']
            folder = frozen / 'prepared/candidate'
            if actor == 'scriptc':
                folder = frozen / 'prepared-scriptc/candidate'
                provenance = scriptc['original_scriptc_provenance'][name]
                assert provenance['source_sha256'] == digest(source) and provenance['wasm_sha256'] == row['wasm_sha256']
            assert digest(folder / row['image']) == row['image_sha256']
            paths.append(folder / row['image'])
        paths += [source, output / 'sources' / name]
    source_hashes = {name: digest(output / 'sources' / name) for name in names}
    policy = dict(
        execution='Public-entry wall: Scriptw evaluate entry, Scriptc WASI _start/output capture, V8 runInContext/print capture.',
        setup='Wasm image loading and Store/instance setup excluded; Scriptw Realm/intrinsic bootstrap remains inside entry. V8 vm.Script eager parse and vm.Context creation outside entry; context_ns retained separately.',
        v8='Normal V8 lazy compilation, JIT tiering and natural GC may occur inside entry. Ten warmups do not assert steady-state optimized code.',
        memory='GNU-time whole-child peak RSS includes each process runtime, setup, warmups and code history. Node includes its source compilation/JIT; Wasm images were prepared in separate earlier processes. No live-heap interpretation.',
        statistics='Fresh process is the unit; four process medians/means per actor/block, eight overall; retain every observation and both blocks. No cross-runtime CPU ratio or significance claim.',
        resources='One CPU, 4GiB cgroup, no swap, grouped OOM; Wasm retained modules1/64MiB. Sequential children; no profiler/barriers.',
        source='Original17 bytes shared by Scriptw and V8; Scriptc derived print-wrapper source and pinned0.4.5/Zig0.16 provenance retained in producer receipt.')
    plan = dict(schema='scriptw-three-runtime-plan-v1', cases=names, runtimes=list(RUNTIMES), samples=SAMPLES, warmups=WARMUPS,
                process_plan=process_plan(names), policy=policy, node_identity=node, consumer_identity=identity,
                source_hashes=source_hashes, source_commit=publication, parent_qualification=authority['native'],
                machine=dict(platform=platform.platform(), affinity=sorted(os.sched_getaffinity(0)),
                             cpu_model=next(line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')),
                             resources={field:(kernel_group()/field).read_text() for field in FIELDS}))
    save(output / 'plan.json', plan)
    paths += [output / 'plan.json', output / 'sources/manifest.json', output / 'candidate/identity.json']
    before = {str(path): digest(path) for path in sorted(set(paths))}
    save(output / 'inputs-before.json', before)
    report = dict(schema='scriptw-three-runtime-comparison-v1', status='incomplete', stage='three-runtime-reference',
                  runtimes=list(RUNTIMES), source_fingerprints={actor:identity['source_fingerprint'] for actor in ('scriptw', 'scriptc')},
                  source_fingerprint=identity['source_fingerprint'], source_commit=publication, measurement_commit=subprocess.check_output(['git','rev-parse','HEAD'], text=True).strip(),
                  node_identity=node, consumer_identity=identity, shared_configuration=identity['shared_configuration'],
                  started_at=stamp(), plan=plan, policy=policy, rows=[], preflight=[])

    def invoke(entry, number, *, preflight=False):
        actor, name = entry['actor'], entry['case']
        case = cases[names.index(name)]
        samples, warmups = SAMPLES, WARMUPS
        folder = output / 'raw'
        if preflight:
            samples, warmups, folder = 3, 1, output / 'preflight'
        stem = f'{number:03d}-{actor}-{name}'
        image = None
        if actor == 'v8':
            argv = [str(node_path), str(HARNESS), str(output / 'sources'), str(samples), str(warmups), name]
        else:
            image = images[actor][name]
            action, parent = 'execute-scriptw', frozen / 'prepared/candidate'
            if actor == 'scriptc':
                action, parent = 'execute-wasi', frozen / 'prepared-scriptc/candidate'
            argv = [str(host), action, str(parent / image['image']), case['stdout'], str(samples), str(warmups)]
        resources_before = {field:(kernel_group()/field).read_text() for field in FIELDS}
        with (folder / (stem+'.stdout')).open('xb') as stdout, (folder / (stem+'.stderr')).open('xb') as stderr:
            resources = measure_process(argv, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, timeout=900, time_binary=args.time)
        resources_after = {field:(kernel_group()/field).read_text() for field in FIELDS}
        save(folder / (stem+'.command.json'), dict(argv=argv, environment={key:value for key,value in env.items() if key.startswith(('NODE_', 'SCRIPTW_'))},
             resources_before=resources_before, resources_after=resources_after, **resources))
        assert all(resources_before[field] == resources_after[field] for field in FIELDS if field != 'memory.events')
        events_before = dict(line.split() for line in resources_before['memory.events'].splitlines())
        events_after = dict(line.split() for line in resources_after['memory.events'].splitlines())
        assert all(events_before[key] == events_after[key] for key in ('oom', 'oom_kill', 'oom_group_kill'))
        assert resources['exit_code'] == 0 and not resources['timed_out'] and not (folder / (stem+'.stderr')).read_bytes()
        raw = load(folder / (stem+'.stdout'))
        values = checked_observations(raw, actor, case, source_hashes[name], identity, image, node, samples, warmups)
        return dict(entry, raw=str((folder / (stem+'.stdout')).relative_to(output)),
                    raw_sha256=digest(folder / (stem+'.stdout')), resources=resources, **values)

    for name in ('integer.js', 'dynamic-string-4096.js', 'join-mixed-1024.js'):
        for actor in RUNTIMES:
            report['preflight'].append(invoke(dict(actor=actor, case=name), len(report['preflight']), preflight=True))
    save(output / 'preflight.json', dict(status='passed', processes=9, checked_entries=36, records=report['preflight']))
    print('PREFLIGHT9/36 PASS', flush=True)
    for entry in plan['process_plan']:
        report['rows'].append(invoke(entry, len(report['rows'])))
        print(len(report['rows']), '/408', entry['actor'], entry['case'], flush=True)
    after = {str(path):digest(path) for path in sorted(set(paths))}
    save(output / 'inputs-after.json', after)
    assert before == after
    assert sorted(os.sched_getaffinity(0)) == plan['machine']['affinity']
    report.update(status='complete', processes=len(report['rows']), summary=reduce_rows(report['rows'], names), finished_at=stamp(),
                  inputs_before_sha256=digest(output/'inputs-before.json'), inputs_after_sha256=digest(output/'inputs-after.json'))
    assert report['processes'] == 408
    save(output / 'analysis.json', report)
    print('COMPLETE408', report['finished_at'], flush=True)


if __name__ == '__main__':
    main()
