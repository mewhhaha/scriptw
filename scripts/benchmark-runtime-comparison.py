#!/usr/bin/env python3
"""Measure original workloads with a qualified frozen Wasm host and normal V8.

Three commands share one cohort contract:

  prepare-cohort  freeze the shared precompiled host and the product CLI from one
                  clean committed tree, produce Scriptw and Scriptc native images
                  of the original17 in separate processes and check every image's
                  output; writes the receipts the measurement requires.
  (default)       measure a frozen cohort: --frozen COHORT --output NEW --time GNU_TIME
  publish         validate a complete measurement and install it as the inert
                  `execution_three_runtime` authority that render-runtime-comparison.py reads.

`--cloud` admits a managed cloud machine's inherited finite memory cap instead of
the local 4-GiB cgroup-v2 scope, still requires one inherited CPU and records the
cloud scope where the cgroup fields were.
"""
import argparse
import codecs
from datetime import datetime, timezone
import hashlib
import io
import json
import math
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import statistics
import subprocess
import sys
import tarfile
import tempfile

from benchmark_metrics import digest, execution_thread_cpu, measure_process
from limited_verification import kernel_group, limited_environment, require_cloud_budget, require_kernel_budget

ROOT = Path(__file__).resolve().parents[1]
RUNTIMES = ('scriptw', 'scriptc', 'v8')
LETTERS = 'ABCBCACABACB'
SAMPLES, WARMUPS = 21, 10
FIELDS = ('memory.max', 'memory.swap.max', 'memory.oom.group', 'cpu.max', 'memory.events')
HARNESS = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_baseline/node-runtime.cjs'
FIXTURES = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths'
AUTHORITY = Path('docs/rust-rewrite/performance-gaps-20261101.json')
SOURCE_PATHS = ('crates', 'Cargo.toml', 'Cargo.lock', '.cargo', 'rust-toolchain.toml')
R_FILES = ('scriptw-runtime-native.bin', 'scriptw-runtime-package.bin',
           'scriptw-runtime-portable-native.bin', 'scriptw-runtime-portable-package.bin')
RUNTIME_MANIFEST_MAGIC = b'SCRWNR01'
COHORT_QUALIFICATION = 'scriptw-cohort-qualification-v1'
CLOUD_SCOPE = ('managed cloud machine: shared virtual CPUs, inherited memory cap, '
               'no kernel-enforced 4 GiB budget and no cgroup OOM event record')
RESOURCES_LOCAL = 'One CPU, 4GiB cgroup, no swap, grouped OOM; Wasm retained modules1/64MiB. Sequential children; no profiler/barriers.'
RESOURCES_CLOUD = ('One inherited CPU of a managed cloud machine; inherited finite memory cap (recorded), no kernel-enforced '
                   '4GiB budget or OOM event record; Wasm retained modules1/64MiB. Sequential children; no profiler/barriers.')
# Inert evidence budget, as the renderer admits it.
MAX_FILE_BYTES, MAX_EXPANDED_BYTES, MAX_ARCHIVE_BYTES, MAX_MEMBERS = 64 * 1024**2, 768 * 1024**2, 128 * 1024**2, 20_000
TEXT_SUFFIXES = {'.cjs', '.js', '.json', '.py', '.stderr', '.stdout', '.txt', '.yaml'}


def load(path):
    return json.loads(Path(path).read_text())


def save(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def stamp():
    return datetime.now(timezone.utc).isoformat()


def require(condition, message):
    if not condition:
        raise ValueError('runtime comparison: ' + message)


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True)


def resource_scope(cloud):
    """Admit the measurement scope once; return the cloud record, or None locally."""
    if cloud:
        # A managed cloud machine has no delegated cgroup-v2 scope to cap at 4 GiB.
        scope = dict(inherited_memory_cap_bytes=require_cloud_budget(), scope=CLOUD_SCOPE)
    else:
        require_kernel_budget(4 * 1024**3)
        scope = None
    assert len(os.sched_getaffinity(0)) == 1
    return scope


def cgroup_fields(cloud):
    if cloud:
        return {}
    return {field: (kernel_group() / field).read_text() for field in FIELDS}


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


def comparisons(summary):
    """Scriptw against each other runtime: wins in both statistics and blocks, block geomeans."""
    result = {}
    for other in RUNTIMES[1:]:
        wins = [row['case'] for row in summary
                if all(block['products']['scriptw']['wall_ns'][stat] < block['products'][other]['wall_ns'][stat]
                       for block in row['blocks'] for stat in ('median', 'mean'))]
        geomeans = [{stat: math.exp(statistics.mean(math.log(row['blocks'][block]['products']['scriptw']['wall_ns'][stat]
                                                             / row['blocks'][block]['products'][other]['wall_ns'][stat])
                                                    for row in summary)) for stat in ('median', 'mean')}
                    for block in range(2)]
        result[other] = dict(wins_in_both_statistics_and_blocks=wins, geomeans=geomeans)
    return result


def scriptw_qualification(frozen, identity, key):
    """The source-bound qualification of the frozen Scriptw source.

    A published key names an authority entry in the committed performance file.
    `cohort` admits the cohort's own receipt from prepare-cohort instead: one
    committed source, every image checked; it carries no A/A performance gate.
    """
    if key == 'cohort':
        authority = load(frozen / 'qualification.json')
        assert authority['schema'] == COHORT_QUALIFICATION and authority['status'] == 'complete'
        assert authority['native']['receipt']['sha256'] == digest(frozen / 'native-qualified.json')
        assert authority['aa_gates'] == 'not-applicable'
        return authority, dict(kind='cohort', path=str(frozen / 'qualification.json'),
                               sha256=digest(frozen / 'qualification.json'))
    authority = load(ROOT / AUTHORITY)[key]
    assert all(row['aa_gate'] == 'pass' for row in authority['aa_gates'].values())
    return authority, dict(kind='published', path=str(AUTHORITY), key=key)


def runtime_build_paths(frozen, identity):
    """The frozen runtime outputs exactly as the cohort's Cargo producer receipt records them."""
    producer = load(frozen / 'runtime-product-producer-candidate.json')
    assert producer['status'] == 'verified'
    assert digest(frozen / producer['build_stdout']) == producer['build_stdout_sha256']
    folder = frozen / 'candidate/runtime-build'
    assert set(R_FILES) <= set(producer['files'])
    assert {path.name for path in folder.glob('*.bin')} == set(producer['files'])
    for name, receipt in producer['files'].items():
        assert digest(folder / name) == receipt['sha256']
        if 'package' in receipt:
            assert receipt['package']['package_source_fingerprint'] == identity['source_fingerprint']
    return [folder / name for name in sorted(producer['files'])]


def measure(args):
    cloud = resource_scope(args.cloud)
    samples, warmups = SAMPLES, WARMUPS
    if args.smoke:
        # Tooling smoke only: the full plan and sample count, one warmup; publish refuses it.
        warmups = 1
    frozen, output = args.frozen.resolve(), args.output.resolve()
    output.mkdir(exist_ok=False)
    for name in ('tmp', 'sources', 'raw', 'preflight', 'candidate', 'tooling', 'cohort'):
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
    authority, qualification = scriptw_qualification(frozen, identity, args.qualification_key)
    publication = authority['publication_commit']
    if qualification['kind'] == 'published':
        assert not subprocess.check_output(['git', 'diff', '--name-only', publication, '--', *SOURCE_PATHS], cwd=ROOT)
    else:
        # The cohort witness binds every tracked compiler input byte to its committed source.
        assert witness['source_commit'] == publication == identity['source_commit']
    assert authority['source_fingerprint'] == identity['source_fingerprint']
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
    receipts = [frozen / 'native-qualified.json', frozen / 'publication-rust-witness.json',
                frozen / 'prepared/candidate/producer-receipt.json', frozen / 'prepared-scriptc/candidate/producer-receipt.json',
                frozen / 'runtime-product-producer-candidate.json', frozen / 'examples-build.stdout']
    if qualification['kind'] == 'cohort':
        receipts += [frozen / 'qualification.json', frozen / 'correctness/receipt.json']
    paths = [Path(__file__), HARNESS, ROOT / 'scripts/benchmark_metrics.py', ROOT / 'scripts/limited_verification.py',
             node_path, args.time.resolve(), host, frozen / 'candidate/identity.json', *receipts, FIXTURES / 'manifest.json']
    paths += runtime_build_paths(frozen, identity)
    for source in (Path(__file__), HARNESS, ROOT / 'scripts/benchmark_metrics.py', ROOT / 'scripts/limited_verification.py'):
        destination = output / 'tooling' / source.name
        shutil.copyfile(source, destination)
        paths.append(destination)
    # Text receipts travel with the measurement; candidate/identity.json stays the only identity copy.
    for source in receipts:
        destination = output / 'cohort' / str(source.relative_to(frozen)).replace('/', '--')
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
        resources=RESOURCES_CLOUD if cloud else RESOURCES_LOCAL,
        source='Original17 bytes shared by Scriptw and V8; Scriptc derived print-wrapper source and pinned0.4.5/Zig0.16 provenance retained in producer receipt.')
    machine = dict(platform=platform.platform(), affinity=sorted(os.sched_getaffinity(0)),
                   cpu_model=next(line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')),
                   resources=cgroup_fields(args.cloud))
    if cloud:
        machine['cloud'] = cloud
    plan = dict(schema='scriptw-three-runtime-plan-v1', cases=names, runtimes=list(RUNTIMES), samples=samples, warmups=warmups,
                process_plan=process_plan(names), policy=policy, node_identity=node, consumer_identity=identity,
                source_hashes=source_hashes, source_commit=publication, parent_qualification=authority['native'],
                qualification=qualification, smoke=args.smoke, machine=machine)
    save(output / 'plan.json', plan)
    paths += [output / 'plan.json', output / 'sources/manifest.json', output / 'candidate/identity.json']
    before = {str(path): digest(path) for path in sorted(set(paths))}
    save(output / 'inputs-before.json', before)
    report = dict(schema='scriptw-three-runtime-comparison-v1', status='incomplete', stage='three-runtime-reference',
                  runtimes=list(RUNTIMES), source_fingerprints={actor:identity['source_fingerprint'] for actor in ('scriptw', 'scriptc')},
                  source_fingerprint=identity['source_fingerprint'], source_commit=publication, measurement_commit=git('rev-parse', 'HEAD').strip(),
                  node_identity=node, consumer_identity=identity, shared_configuration=identity['shared_configuration'],
                  started_at=stamp(), plan=plan, policy=policy, smoke=args.smoke, rows=[], preflight=[])

    def invoke(entry, number, *, preflight=False):
        actor, name = entry['actor'], entry['case']
        case = cases[names.index(name)]
        count, warm = samples, warmups
        folder = output / 'raw'
        if preflight:
            count, warm, folder = 3, 1, output / 'preflight'
        stem = f'{number:03d}-{actor}-{name}'
        image = None
        if actor == 'v8':
            argv = [str(node_path), str(HARNESS), str(output / 'sources'), str(count), str(warm), name]
        else:
            image = images[actor][name]
            action, parent = 'execute-scriptw', frozen / 'prepared/candidate'
            if actor == 'scriptc':
                action, parent = 'execute-wasi', frozen / 'prepared-scriptc/candidate'
            argv = [str(host), action, str(parent / image['image']), case['stdout'], str(count), str(warm)]
        resources_before = cgroup_fields(args.cloud)
        with (folder / (stem+'.stdout')).open('xb') as stdout, (folder / (stem+'.stderr')).open('xb') as stderr:
            resources = measure_process(argv, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, timeout=900, time_binary=args.time)
        resources_after = cgroup_fields(args.cloud)
        save(folder / (stem+'.command.json'), dict(argv=argv, environment={key:value for key,value in env.items() if key.startswith(('NODE_', 'SCRIPTW_'))},
             resources_before=resources_before, resources_after=resources_after, **resources))
        if not args.cloud:
            assert all(resources_before[field] == resources_after[field] for field in FIELDS if field != 'memory.events')
            events_before = dict(line.split() for line in resources_before['memory.events'].splitlines())
            events_after = dict(line.split() for line in resources_after['memory.events'].splitlines())
            assert all(events_before[key] == events_after[key] for key in ('oom', 'oom_kill', 'oom_group_kill'))
        assert resources['exit_code'] == 0 and not resources['timed_out'] and not (folder / (stem+'.stderr')).read_bytes()
        raw = load(folder / (stem+'.stdout'))
        values = checked_observations(raw, actor, case, source_hashes[name], identity, image, node, count, warm)
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
    assert all(digest(ROOT / name) == expected for name, expected in witness['files'].items())
    assert sorted(os.sched_getaffinity(0)) == plan['machine']['affinity']
    report.update(status='complete', processes=len(report['rows']), summary=reduce_rows(report['rows'], names), finished_at=stamp(),
                  inputs_before_sha256=digest(output/'inputs-before.json'), inputs_after_sha256=digest(output/'inputs-after.json'))
    assert report['processes'] == 408
    save(output / 'analysis.json', report)
    print('COMPLETE408', report['finished_at'], flush=True)


# --- cohort preparation -------------------------------------------------------------------------


def source_witness():
    """Every tracked compiler input outside test directories, which must equal HEAD."""
    dirty = [line[3:].split(' -> ')[-1] for line in
             git('status', '--porcelain', '--untracked-files=all', '--', *SOURCE_PATHS).splitlines()]
    compiler = [name for name in dirty if '/tests/' not in '/' + name]
    require(not compiler, 'compiler sources must be the committed tree: ' + ', '.join(compiler[:5]))
    names = [name for name in git('ls-files', '-z', '--', *SOURCE_PATHS).split('\0')
             if name and '/tests/' not in '/' + name]
    return {name: digest(ROOT / name) for name in sorted(names)}, sorted(dirty)


def cargo(output, stem, selection, jobs):
    command = ['cargo', 'build', '-j', str(jobs), '--release', '--locked', '-p', 'scriptw-cli',
               *selection, '--message-format=json-render-diagnostics']
    with (output / (stem + '.stdout')).open('xb') as stdout, (output / (stem + '.stderr')).open('xb') as stderr:
        subprocess.run(command, cwd=ROOT, stdout=stdout, stderr=stderr, check=True)
    rows = [json.loads(line) for line in (output / (stem + '.stdout')).read_text().splitlines() if line.startswith('{')]
    finished = [row for row in rows if row.get('reason') == 'build-finished']
    require(len(finished) == 1 and finished[0].get('success') is True, stem + ': Cargo did not finish successfully')
    return rows


def cargo_executable(rows, kind, name):
    found = [row for row in rows if row.get('reason') == 'compiler-artifact' and row['target']['name'] == name
             and kind in row['target']['kind'] and row.get('executable')]
    require(len(found) == 1, f'one {kind} artifact required: {name}')
    return Path(found[0]['executable'])


def runtime_owner(rows):
    found = [row for row in rows if row.get('reason') == 'build-script-executed'
             and '/crates/scriptw-runtime-image#' in row['package_id']]
    require(len(found) == 1 and found[0].get('out_dir'), 'one scriptw-runtime-image build script required')
    return found[0]


def frozen_copy(source, destination):
    shutil.copyfile(source, destination)
    os.chmod(destination, 0o555)
    require(digest(source) == digest(destination), 'artifact changed while it was copied: ' + str(source))


def decode_runtime_manifest(data):
    """The Cargo producer's runtime manifest (embedded_runtime/manifest.rs, v5)."""
    require(data[:8] == RUNTIME_MANIFEST_MAGIC and len(data) >= 23, 'runtime manifest magic differs')
    length = int.from_bytes(data[19:21], 'little')
    fields = data[21 + length:]
    require(len(fields) == 128, 'runtime manifest framing differs')
    return dict(version=int.from_bytes(data[8:12], 'little'), target=data[21:21 + length].decode(),
                source_fingerprint=fields[:32].hex(), runtime_key=fields[32:64].hex(),
                package_sha256=fields[64:96].hex(), native_sha256=fields[96:].hex())


def runtime_receipt(output, owner, identity):
    """Freeze the host's two R groups; each manifest must bind its package/native bytes to the host source."""
    out_dir = Path(owner['out_dir'])
    destination = output / 'candidate/runtime-build'
    destination.mkdir(parents=True)
    files = {}
    for prefix in ('scriptw-runtime', 'scriptw-runtime-portable'):
        names = {suffix: f'{prefix}-{suffix}.bin' for suffix in ('manifest', 'package', 'native')}
        for name in names.values():
            frozen_copy(out_dir / name, destination / name)
            files[name] = dict(source_path=str(out_dir / name), sha256=digest(destination / name),
                               bytes=(destination / name).stat().st_size)
        manifest = decode_runtime_manifest((destination / names['manifest']).read_bytes())
        require(manifest['source_fingerprint'] == identity['source_fingerprint'], 'runtime source differs: ' + prefix)
        require(manifest['target'] == identity['shared_configuration']['target_triple'], 'runtime target differs')
        require(manifest['package_sha256'] == files[names['package']]['sha256']
                and manifest['native_sha256'] == files[names['native']]['sha256'],
                'runtime manifest does not bind its package/native outputs: ' + prefix)
        files[names['package']]['package'] = dict(
            sha256=manifest['package_sha256'], bytes=files[names['package']]['bytes'],
            package_source_fingerprint=manifest['source_fingerprint'], runtime_key=manifest['runtime_key'],
            manifest=names['manifest'], manifest_version=manifest['version'], fingerprint_matches_host=True)
    require(set(R_FILES) <= set(files), 'runtime output group is incomplete')
    return files


def preparation_environment(output):
    env = limited_environment()
    for key in ('NODE_OPTIONS', 'LD_PRELOAD', 'LD_AUDIT'):
        require(not env.get(key), 'forbidden inherited setting: ' + key)
    for key in list(env):
        if key.startswith(('SCRIPTW_PRECOMPILED_PHASE_', 'SCRIPTC_', 'ZIG_')) or key in (
                'SCRIPTW_PRECOMPILED_EXECUTION_WINDOWS', 'SCRIPTW_CACHE_DIR', 'NODE_COMPILE_CACHE'):
            del env[key]
    env.update(TMPDIR=str(output / 'tmp'), SCRIPTW_CACHE_DIR=str(output / 'cache'), NODE_DISABLE_COMPILE_CACHE='1')
    return env


def produce(folder, stem, argv, env, time_binary, *, cwd=ROOT):
    """One unmeasured producer/checker process with retained streams and accounting."""
    folder.mkdir(parents=True, exist_ok=True)
    argv = list(map(str, argv))
    with (folder / (stem + '.stdout')).open('xb') as stdout, (folder / (stem + '.stderr')).open('xb') as stderr:
        result = measure_process(argv, cwd=cwd, env=env, stdout=stdout, stderr=stderr, timeout=900, time_binary=time_binary)
    save(folder / (stem + '.command.json'), dict(argv=argv, **result))
    require(result['exit_code'] == 0 and not result['timed_out'], 'producer failed: ' + stem)
    return result


def prepare_cohort(args):
    from compilation_comparison import ADAPTER, process_environment, scriptc_companions
    from precompiled_comparison import SHARED_CONFIGURATION, image_metadata

    # Cargo runs before the serial scope; --cargo-jobs is explicit and nothing here is timed.
    require(args.cargo_jobs > 0, 'positive Cargo jobs required')
    if args.cloud:
        require_cloud_budget(serial=False)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('tmp', 'cache', 'candidate', 'candidate-ordinary'):
        (output / name).mkdir()
    commit = git('rev-parse', 'HEAD').strip()
    witness, dirty_tests = source_witness()
    product_rows = cargo(output, 'product-build', ['--bin', 'scriptw'], args.cargo_jobs)
    host_rows = cargo(output, 'examples-build', ['--example', 'compiler_precompiled_baseline'], args.cargo_jobs)
    product = cargo_executable(product_rows, 'bin', 'scriptw')
    built_host = cargo_executable(host_rows, 'example', 'compiler_precompiled_baseline')
    host, ordinary_cli = output / 'candidate/compiler_precompiled_baseline', output / 'candidate-ordinary/scriptw'
    frozen_copy(built_host, host)
    frozen_copy(product, ordinary_cli)
    resource_scope(args.cloud)  # Serial affinity from here on.
    env = preparation_environment(output)
    identity = json.loads(subprocess.check_output([host, 'identity'], cwd=ROOT, env=env, timeout=60))
    require(identity['schema'] == 'scriptw-precompiled-shared-host-v1' and identity['executable_sha256'] == digest(host),
            'host identity does not describe its frozen bytes')
    require(identity['source_commit'] == commit, 'host source commit differs from HEAD')
    require(identity['comparison_cohort'] == 'scriptw-precompiled-comparison-v1'
            and all(identity['shared_configuration'][key] == value for key, value in SHARED_CONFIGURATION.items())
            and identity['shared_configuration']['consumer_compiler_enabled'] is False, 'host configuration differs')
    save(output / 'candidate/identity.json', identity)
    ordinary = json.loads(subprocess.check_output([ordinary_cli, 'compiler-identity'], cwd=ROOT, env=env, timeout=60))
    require(ordinary['executable_sha256'] == digest(ordinary_cli)
            and ordinary['source_fingerprint'] == identity['source_fingerprint']
            and ordinary['source_revision'] == dict(kind='git-commit', commit=commit),
            'product CLI and host do not share one compiler source')
    save(output / 'candidate-ordinary/identity.json', ordinary)
    owner = runtime_owner(host_rows)
    files = runtime_receipt(output, owner, identity)
    fingerprint = identity['source_fingerprint']

    manifest = load(FIXTURES / 'manifest.json')
    cases = {row['file']: row for row in manifest['execution']}
    require(len(cases) == 17, 'the original17 corpus is required')

    # Scriptw: one producer process compiles all 17 sources and prepares their native images.
    folder = output / 'prepared/candidate'
    folder.parent.mkdir()
    produce(output / 'preparation/raw', 'producer', [host, 'prepare-scriptw', FIXTURES, folder], env, args.time)
    producer = load(output / 'preparation/raw/producer.stdout')
    require(not (output / 'preparation/raw/producer.stderr').read_bytes(), 'Scriptw producer wrote diagnostics')
    require(producer['source_fingerprint'] == fingerprint and producer['executable_sha256'] == identity['executable_sha256']
            and producer['compiler_enabled'] is True and producer['shared_configuration'] == identity['shared_configuration'],
            'Scriptw producer identity differs')
    require([row['file'] for row in producer['cases']] == list(cases), 'Scriptw producer corpus differs')
    for row in producer['cases']:
        name = row['file']
        require(row['source_sha256'] == digest(FIXTURES / name) and row['stdout'] == cases[name]['stdout'],
                'Scriptw source or expected output differs: ' + name)
        image = (folder / row['image']).read_bytes()
        metadata = image_metadata(image, 'scriptw', fingerprint)
        require(digest(folder / row['image']) == row['image_sha256'] == metadata['sha256']
                and digest(folder / row['wasm']) == row['program_wasm_sha256'] == metadata['program_wasm_sha256']
                and row['runtime_key'] == metadata['runtime_key'], 'Scriptw image framing differs: ' + name)
    save(folder / 'producer-receipt.json', dict(
        status='prepared', product_identity=identity, producer=producer,
        image_files={row['file']: row for row in producer['cases']},
        producer_stdout_sha256=digest(output / 'preparation/raw/producer.stdout'),
        producer_stderr_sha256=digest(output / 'preparation/raw/producer.stderr'),
        producer_command=load(output / 'preparation/raw/producer.command.json')))

    # Scriptc: pinned 0.4.5 builds the print-adapted source to standalone WASI Wasm with fresh
    # application caches; the shared host then prepares each native image in its own process.
    scriptc = Path(shutil.which(str(args.scriptc)) or args.scriptc).resolve(strict=True)
    zig = Path(shutil.which(str(args.zig)) or args.zig).resolve(strict=True)
    runtime_pack = args.runtime_pack.resolve(strict=True)
    pack = load(runtime_pack / 'runtime-pack.json')
    require(pack['version'] == '0.4.5' and pack['target']['name'] == 'wasm32-wasi', 'wrong Scriptc runtime pack')
    companion_manifest = Path(str(scriptc) + '.json')
    for selected in scriptc_companions(scriptc, load(companion_manifest)).values():
        require(Path(selected).exists(), 'missing Scriptc companion: ' + selected)
    zig_cache = args.zig_global_cache_dir.resolve()
    tools_env = dict(env, PATH=str(zig.parent) + os.pathsep + env['PATH'])
    require(Path(shutil.which('zig', path=tools_env['PATH'])).resolve() == zig, 'Scriptc zig lookup differs')
    versions = {name: subprocess.check_output(command, cwd=ROOT, env=tools_env, text=True, timeout=60).strip()
                for name, command in (('scriptc', [scriptc, '--version']), ('zig', [zig, 'version']))}
    require(versions['scriptc'].removeprefix('scriptc ').strip() == '0.4.5' and versions['zig'] == '0.16.0',
            'Scriptc/Zig pins differ')
    tools = dict(scriptc=dict(path=str(scriptc), sha256=digest(scriptc), version=versions['scriptc'],
                              toolchain_manifest=dict(path=str(companion_manifest), sha256=digest(companion_manifest))),
                 zig=dict(path=str(zig), sha256=digest(zig), version=versions['zig'], global_cache=str(zig_cache)),
                 runtime_pack=dict(path=str(runtime_pack), manifest_sha256=digest(runtime_pack / 'runtime-pack.json'),
                                   files={str(path.relative_to(runtime_pack)): digest(path)
                                          for path in sorted(runtime_pack.rglob('*')) if path.is_file()}))
    if args.package_lock:
        lock = args.package_lock.resolve(strict=True)
        tools['package_lock'] = dict(path=str(lock), name=lock.name, sha256=digest(lock))
    folder = output / 'prepared-scriptc/candidate'
    folder.mkdir(parents=True)
    records, provenance = {}, {}
    for number, name in enumerate(cases):
        work = output / 'scriptc-work' / name
        for child in ('tmp', 'cache', 'zig-local'):
            (work / child).mkdir(parents=True)
        adapted = work / name
        adapted.write_bytes(ADAPTER.encode() + (FIXTURES / name).read_bytes())
        wasm = output / 'scriptc-wasm' / (name + '.wasm')
        wasm.parent.mkdir(exist_ok=True)
        child_env = dict(tools_env, **process_environment(work, runtime_pack, zig_cache))
        compiled = produce(output / 'preparation-scriptc/raw', f'{number:02d}-scriptc-{name}',
                           [scriptc, 'build', adapted, '-o', wasm], child_env, args.time, cwd=work)
        require(wasm.read_bytes()[:8] == b'\0asm\1\0\0\0', 'Scriptc did not emit portable Wasm: ' + name)
        provenance[name] = dict(source_sha256=digest(FIXTURES / name), adapted_source_sha256=digest(adapted),
                                wasm_sha256=digest(wasm), wasm_bytes=wasm.stat().st_size,
                                compile_record_sha256=digest(output / f'preparation-scriptc/raw/{number:02d}-scriptc-{name}.command.json'))
        image = folder / (name + '.swpc')
        produce(output / 'preparation-scriptc/raw', f'{number:02d}-native-{name}', [host, 'prepare-wasi', wasm, image], env, args.time)
        stem = output / f'preparation-scriptc/raw/{number:02d}-native-{name}'
        report = load(str(stem) + '.stdout')
        require(not Path(str(stem) + '.stderr').read_bytes(), 'WASI producer wrote diagnostics: ' + name)
        require(report['schema'] == 'scriptw-precompiled-wasi-producer-v1' and report['compiler_enabled'] is True
                and all(report[key] == identity[key] for key in ('source_fingerprint', 'executable_sha256', 'shared_configuration'))
                and report['wasm_sha256'] == provenance[name]['wasm_sha256']
                and report['image'] == image.name and report['image_sha256'] == digest(image), 'WASI producer differs: ' + name)
        metadata = image_metadata(image.read_bytes(), 'scriptc', fingerprint)
        require(metadata['program_wasm_sha256'] == provenance[name]['wasm_sha256'], 'WASI image embeds other Wasm: ' + name)
        records[name] = dict(image=image.name, image_sha256=digest(image), image_bytes=image.stat().st_size,
                             wasm_sha256=provenance[name]['wasm_sha256'], source_sha256=provenance[name]['source_sha256'],
                             producer=report, resources=compiled)
        shutil.rmtree(work)
    save(folder / 'producer-receipt.json', dict(status='prepared', product_identity=identity, cases=records,
                                                original_scriptc_provenance=provenance, tools=tools, adapter=ADAPTER))

    # Every image of both adapters runs once in a fresh compiler-disabled host, all outputs checked.
    images = dict(scriptw=load(output / 'prepared/candidate/producer-receipt.json')['image_files'], scriptc=records)
    checks = []
    for actor, action, parent in (('scriptw', 'execute-scriptw', output / 'prepared/candidate'),
                                  ('scriptc', 'execute-wasi', folder)):
        for name, case in cases.items():
            image = images[actor][name]
            stem = f'{actor}-{name}'
            produce(output / 'correctness/raw', stem, [host, action, parent / image['image'], case['stdout'], '3', '1'], env, args.time)
            raw = output / 'correctness/raw' / (stem + '.stdout')
            require(not (output / 'correctness/raw' / (stem + '.stderr')).read_bytes(), 'checked execution wrote diagnostics')
            checked_observations(load(raw), actor, case, digest(FIXTURES / name), identity, image, None, 3, 1)
            checks.append(dict(actor=actor, case=name, report_sha256=digest(raw)))
    save(output / 'correctness/receipt.json', dict(status='verified', source_fingerprint=fingerprint, cases=checks,
         checked_processes=len(checks), checked_entries=4 * len(checks),
         producer_receipt_sha256=dict(scriptw=digest(output / 'prepared/candidate/producer-receipt.json'),
                                      scriptc=digest(folder / 'producer-receipt.json'))))

    # A second, no-op Cargo pass proves the copies are the exact artifacts of these sources.
    for stem, selection in (('product-build', ['--bin', 'scriptw']), ('examples-build', ['--example', 'compiler_precompiled_baseline'])):
        rows = cargo(output, stem + '-confirm', selection, args.cargo_jobs)
        require(all(row['fresh'] for row in rows if row.get('reason') == 'compiler-artifact'),
                stem + ': Cargo rebuilt something after the freeze; rerun on a quiet tree')
    require(digest(built_host) == digest(host) and digest(product) == digest(ordinary_cli)
            and runtime_owner(load_rows(output / 'examples-build-confirm.stdout'))['out_dir'] == owner['out_dir']
            and all(digest(Path(row['source_path'])) == row['sha256'] for row in files.values()),
            'Cargo outputs changed after the freeze')
    after, _ = source_witness()
    require(after == witness, 'compiler sources changed during the freeze')
    save(output / 'runtime-product-producer-candidate.json', dict(
        status='verified', build_stdout='examples-build.stdout', build_stdout_sha256=digest(output / 'examples-build.stdout'),
        owner=owner, files=files))
    save(output / 'publication-rust-witness.json', dict(
        status='all tracked compiler inputs unchanged through the freeze', source_commit=commit,
        source_fingerprint=fingerprint, files=witness, deleted=[], dirty_test_paths_excluded=dirty_tests,
        purpose='Every Scriptw artifact of the cohort is built from these exact bytes; the driver rechecks them.'))
    save(output / 'native-qualified.json', dict(
        status='complete', source_fingerprint=fingerprint, source_commit=commit,
        scope=('cohort qualification: one clean committed tree; the product CLI, the shared precompiled host and both Cargo '
               'runtime-image groups share one compiler source fingerprint; Cargo confirmed the frozen copies fresh; every '
               'Scriptw and Scriptc image of the original17 ran in a fresh compiler-disabled host process with exact output '
               'on all four entries. No native test selection, A/A gate or Test262 run belongs to this cohort.'),
        correctness=dict(path='correctness/receipt.json', sha256=digest(output / 'correctness/receipt.json'),
                         checked_processes=len(checks), checked_entries=4 * len(checks)),
        cargo_producers={stem: dict(path=stem + '.stdout', sha256=digest(output / (stem + '.stdout')))
                         for stem in ('product-build', 'examples-build', 'product-build-confirm', 'examples-build-confirm')},
        selected_tests=[]))
    save(output / 'qualification.json', dict(
        schema=COHORT_QUALIFICATION, status='complete', publication_commit=commit, source_fingerprint=fingerprint,
        native=dict(receipt=dict(path='native-qualified.json', sha256=digest(output / 'native-qualified.json')),
                    checked_entries=4 * len(checks),
                    limits='Freeze and image-output qualification of one committed source; no native test-suite, A/A or Test262 claim.'),
        aa_gates='not-applicable'))
    shutil.rmtree(output / 'tmp')
    shutil.rmtree(output / 'cache')
    print(json.dumps(dict(cohort=str(output), source_commit=commit, source_fingerprint=fingerprint,
                          host_sha256=identity['executable_sha256'], scriptw_sha256=ordinary['executable_sha256'],
                          checked_entries=4 * len(checks)), indent=2))


def load_rows(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.startswith('{')]


# --- publication -------------------------------------------------------------------------------


def checked_campaign(campaign):
    """Revalidate every retained process of a complete measurement from its own text evidence."""
    report = load(campaign / 'analysis.json')
    assert report['schema'] == 'scriptw-three-runtime-comparison-v1' and report['status'] == 'complete'
    assert report['processes'] == len(report['rows']) == 408
    assert load(campaign / 'inputs-before.json') == load(campaign / 'inputs-after.json')
    assert report['inputs_before_sha256'] == digest(campaign / 'inputs-before.json')
    assert report['inputs_after_sha256'] == digest(campaign / 'inputs-after.json')
    assert load(campaign / 'plan.json') == report['plan'] and load(campaign / 'candidate/identity.json') == report['consumer_identity']
    cases = {row['file']: row for row in load(campaign / 'sources/manifest.json')['execution']}
    assert list(cases) == report['plan']['cases'] and len(cases) == 17
    for name in cases:
        assert digest(campaign / 'sources' / name) == report['plan']['source_hashes'][name]
    assert report['summary'] == reduce_rows(report['rows'], list(cases))
    images = dict(scriptw=load(campaign / 'cohort/prepared--candidate--producer-receipt.json')['image_files'],
                  scriptc=load(campaign / 'cohort/prepared-scriptc--candidate--producer-receipt.json')['cases'])
    samples, warmups = report['plan']['samples'], report['plan']['warmups']
    for preflight, rows in ((False, report['rows']), (True, report['preflight'])):
        for row in rows:
            path = campaign / row['raw']
            assert digest(path) == row['raw_sha256']
            image = None if row['actor'] == 'v8' else images[row['actor']][row['case']]
            count, warm = (3, 1) if preflight else (samples, warmups)
            checked = checked_observations(load(path), row['actor'], cases[row['case']], report['plan']['source_hashes'][row['case']],
                                           report['consumer_identity'], image, report['node_identity'], count, warm)
            assert all(row[key] == value for key, value in checked.items())
            assert row['resources']['exit_code'] == 0 and not row['resources']['timed_out']
    return report


def inert_members(campaign, prefix):
    """UTF-8 text evidence only: no executable, Wasm or serialized native image enters the archive."""
    selected = {}
    for path in sorted(campaign.rglob('*')):
        relative = path.relative_to(campaign)
        if path.is_symlink() or not path.is_file() or relative.parts[0] in ('tmp', 'cache'):
            continue
        assert path.suffix in TEXT_SUFFIXES and not path.stat().st_mode & 0o111, 'unexpected evidence file: ' + str(relative)
        data = path.read_bytes()
        assert len(data) <= MAX_FILE_BYTES and not data.startswith((b'\0asm', b'\x7fELF', b'SCRW'))
        codecs.decode(data, 'utf-8')
        selected[f'{prefix}/{relative.as_posix()}'] = data
    return selected


def write_archive(path, members, index):
    index_bytes = (json.dumps(index, indent=2, sort_keys=True) + '\n').encode()
    sidecar = f'{hashlib.sha256(index_bytes).hexdigest()}  INDEX.json\n'.encode()
    members = dict(members, **{'INDEX.json': index_bytes, 'INDEX.json.sha256': sidecar})
    expanded = sum(len(data) for data in members.values())
    assert expanded <= MAX_EXPANDED_BYTES and len(members) <= MAX_MEMBERS
    descriptor, temporary = tempfile.mkstemp(prefix='.runtime-comparison-', suffix='.tar.xz', dir=path.parent)
    os.close(descriptor)
    temporary = Path(temporary)
    try:
        with tarfile.open(temporary, 'w:xz', preset=6) as archive:
            for name, data in members.items():
                assert not PurePosixPath(name).is_absolute() and '..' not in PurePosixPath(name).parts
                info = tarfile.TarInfo(name)
                info.size, info.mode, info.mtime = len(data), 0o644, 0
                archive.addfile(info, io.BytesIO(data))
        assert temporary.stat().st_size <= MAX_ARCHIVE_BYTES
        seen = {}
        with tarfile.open(temporary, 'r|xz') as archive:
            for member in archive:
                assert member.isfile() and member.name not in seen
                seen[member.name] = archive.extractfile(member).read()
        assert seen == members
        os.chmod(temporary, 0o644)
        os.link(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
    return len(members), expanded


def publish(args):
    root = args.root.resolve()
    campaign = args.campaign.resolve()
    report = checked_campaign(campaign)
    require(not report.get('smoke') or root != ROOT, 'a smoke measurement cannot replace the published reference')
    authority_path = root / AUTHORITY
    original = authority_path.read_bytes()
    authority = json.loads(original)
    require((json.dumps(authority, indent=2) + '\n').encode() == original, 'authority formatting would change')
    finished = datetime.fromisoformat(report['finished_at'])
    date = finished.date().isoformat()
    name = args.name or 'runtime-comparison-' + finished.strftime('%Y%m%d')
    archive = Path('docs/rust-rewrite') / (name + '-inert.tar.xz')
    require(not (root / archive).exists(), 'refusing to replace an existing archive: ' + str(archive))
    node = report['node_identity']
    summary = comparisons(report['summary'])
    rows = report['rows']
    readiness = dict(comparison_status='complete', processes=408, runtimes=report['runtimes'],
                     preflight_processes=9, preflight_checked_entries=36,
                     checked_warmup_and_measured_entries=sum(len(row['wall_ns']) for row in rows) + 408 * report['plan']['warmups'],
                     wasm_compiler_enabled=False,
                     v8_policy='normal lazy compilation/JIT/natural GC; eager source parse/context setup outside entry',
                     node_version=node['versions']['node'], v8_version=node['versions']['v8'],
                     source_fingerprint=report['source_fingerprint'], machine=report['plan']['machine'])
    members = inert_members(campaign, 'evidence/' + name)
    index = dict(schema='scriptw-inert-evidence-index-v1', cohort=name, readiness=readiness,
                 members=[dict(path=member, bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
                          for member, data in sorted(members.items())],
                 excluded=['tmp and caches', 'executables, Wasm and native images (text receipts retained)'],
                 limits=dict(file_bytes=MAX_FILE_BYTES, expanded_bytes=MAX_EXPANDED_BYTES,
                             archive_bytes=MAX_ARCHIVE_BYTES, members=MAX_MEMBERS))
    count, expanded = write_archive(root / archive, members, index)
    qualification = report['plan']['qualification']
    entry = dict(
        status='complete descriptive three-runtime public-entry reference; no conformance or significance claim',
        source_fingerprint=report['source_fingerprint'], publication_commit=report['source_commit'],
        measurement_commit=report['measurement_commit'], started_at=report['started_at'], finished_at=report['finished_at'],
        runtimes=report['runtimes'], node_identity=node, shared_configuration=report['shared_configuration'],
        processes=408, processes_per_actor_case=8, measured_entries_per_process=report['plan']['samples'],
        warmups_per_process=report['plan']['warmups'], preflight=dict(processes=9, checked_entries=36),
        policy=report['policy'], comparisons=summary, machine=report['plan']['machine'],
        parent_qualification=dict(report['plan']['parent_qualification'], source=qualification['kind']),
        reference17=dict(evidence=dict(path='analysis.json', sha256=digest(campaign / 'analysis.json'))),
        evidence=dict(archive=str(archive), sha256=digest(root / archive), bytes=(root / archive).stat().st_size,
                      regular_utf8_members=count, expanded_utf8_bytes=expanded, contains_native_or_wasm_images=False))
    history = authority.setdefault('history', {})
    key = 'execution_three_runtime_before_' + finished.strftime('%Y%m%d')
    require(key not in history, 'history already retains a replaced reference for this date')
    history[key] = dict(date=authority['date'], status=authority['status'], entry=authority['execution_three_runtime'])
    wins = {other: len(summary[other]['wins_in_both_statistics_and_blocks']) for other in ('scriptc', 'v8')}
    medians = {other: [block['median'] for block in summary[other]['geomeans']] for other in ('scriptc', 'v8')}
    authority.update(date=date, status=(
        f"Current fresh408-process three-runtime reference measured {date}: Scriptw {wins['scriptc']}/17 wall wins versus "
        f"pinned Scriptc (median geomeans {min(medians['scriptc']):.3f}-{max(medians['scriptc']):.3f}x), {wins['v8']}/17 versus "
        f"Node {node['versions']['node']}/V8 {node['versions']['v8']} ({min(medians['v8']):.3f}-{max(medians['v8']):.3f}x). "
        'Public-entry setup differences and limits are in execution_three_runtime.policy.'),
        execution_three_runtime=entry)
    encoded = (json.dumps(authority, indent=2) + '\n').encode()
    temporary = authority_path.with_name('.' + authority_path.name + '.new')
    temporary.write_bytes(encoded)
    os.replace(temporary, authority_path)
    print(json.dumps(dict(archive=str(archive), authority=str(AUTHORITY), date=date, wins=wins, median_geomeans=medians), indent=2))


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    cloud_help = "admit a managed cloud machine's inherited memory cap; the report records that scope"
    if argv[:1] == ['prepare-cohort']:
        parser.add_argument('command')
        parser.add_argument('--output', type=Path, required=True, help='new cohort directory')
        parser.add_argument('--scriptc', type=Path, required=True, help='Scriptc 0.4.5 native executable (its .json manifest beside it)')
        parser.add_argument('--runtime-pack', type=Path, required=True)
        parser.add_argument('--zig', type=Path, default=Path('zig'))
        parser.add_argument('--zig-global-cache-dir', type=Path, required=True)
        parser.add_argument('--package-lock', type=Path, help='the Scriptc installation lock file (npm or pnpm)')
        parser.add_argument('--time', type=Path, required=True)
        parser.add_argument('--cargo-jobs', type=int, default=1)
        parser.add_argument('--cloud', action='store_true', help=cloud_help)
        return prepare_cohort(parser.parse_args(argv))
    if argv[:1] == ['publish']:
        parser.add_argument('command')
        parser.add_argument('campaign', type=Path, help='complete measurement directory (analysis.json)')
        parser.add_argument('--name', help='archive cohort name (default runtime-comparison-YYYYMMDD)')
        parser.add_argument('--root', type=Path, default=ROOT, help=argparse.SUPPRESS)
        return publish(parser.parse_args(argv))
    parser.add_argument('--frozen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--node', default='node')
    parser.add_argument('--time', type=Path, required=True)
    parser.add_argument('--qualification-key', default='private_numeric_facts',
                        help="Published qualification entry in performance-gaps-20261101.json, or 'cohort' "
                             "for the frozen cohort's own qualification.json from prepare-cohort")
    parser.add_argument('--cloud', action='store_true', help=cloud_help)
    parser.add_argument('--smoke', action='store_true', help='tooling smoke: one warmup per process; never publishable')
    return measure(parser.parse_args(argv))


if __name__ == '__main__':
    main()
