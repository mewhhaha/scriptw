#!/usr/bin/env python3
"""Measure original17 JavaScript-to-portable-Wasm CLI builds, with cold app caches."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile

from benchmark_metrics import digest, measure_process
from compilation_comparison import ACTORS, ADAPTER, FIXTURES, POLICY, ROOT, SCHEMA, load_report, process_environment, process_plan, render, require, scriptc_companions, summarize
from limited_verification import kernel_group, limited_environment, require_kernel_budget


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def stamp():
    return datetime.now(timezone.utc).isoformat()


def executable(value):
    return Path(shutil.which(str(value)) or value).resolve(strict=True)


def identity(path):
    return dict(path=str(path), bytes=path.stat().st_size, sha256=digest(path))


def retain_scriptc_toolchain(compiler, output):
    manifest_path = Path(str(compiler) + '.json')
    manifest = json.loads(manifest_path.read_text())
    inputs = {manifest_path}
    for selected in scriptc_companions(compiler, manifest).values():
        path = Path(selected)
        require(path.exists(), 'missing Scriptc companion: ' + str(path))
        if path.is_dir():
            inputs.update(child for child in path.rglob('*') if child.is_file())
        else:
            inputs.add(path)
    hashes = {str(path): digest(path) for path in sorted(inputs)}
    save(output / 'tooling/scriptc-toolchain-inputs.json', hashes)
    shutil.copyfile(manifest_path, output / 'tooling/scriptc-toolchain.json')
    return dict(manifest=identity(manifest_path),
                inputs_sha256=digest(output / 'tooling/scriptc-toolchain-inputs.json'), files=len(hashes)), inputs


def retain_artifact(path, output):
    with path.open('rb') as stream:
        require(stream.read(8) == b'\0asm\1\0\0\0', 'compiler did not emit portable Wasm')
    sha = digest(path)
    target = output / 'artifacts' / (sha + '.wasm')
    if not target.exists():
        shutil.copyfile(path, target)
    return dict(path=str(target.relative_to(output)), bytes=path.stat().st_size, sha256=sha)


def build_invocation(actor, compiler, source, wasm, env):
    require(actor in ACTORS, 'unknown compiler actor')
    child_env = dict(env)
    argv = [str(compiler), 'build', str(source), '-o', str(wasm)]
    if actor == 'scriptw':
        child_env['SCRIPTW_WASM_DUMP'] = str(wasm)
        argv = [str(compiler), 'build', 'wasm', str(source)]
    return argv, child_env


def run(args):
    require_kernel_budget(4 * 1024**3)
    require(len(os.sched_getaffinity(0)) == 1, 'one CPU required')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('sources', 'adapted', 'expected', 'logs', 'artifacts', 'work', 'tooling'):
        (output / name).mkdir()
    scriptw, scriptc, zig, timer = map(executable, (args.scriptw, args.scriptc, args.zig, args.time))
    runtime_pack = args.runtime_pack.resolve(strict=True)
    pack = json.loads((runtime_pack / 'runtime-pack.json').read_text())
    require(pack['version'] == '0.4.5' and pack['target']['name'] == 'wasm32-wasi', 'wrong Scriptc runtime pack')
    env = limited_environment()
    for key in ('NODE_OPTIONS', 'LD_PRELOAD', 'LD_AUDIT'):
        require(not env.get(key), 'forbidden inherited setting: ' + key)
    for key in list(env):
        if key.startswith(('SCRIPTW_', 'SCRIPTC_', 'ZIG_')) or key == 'NODE_COMPILE_CACHE':
            del env[key]
    env.update(SCRIPTW_JOBS='1', SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES='1',
               SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES=str(64 * 1024**2),
               SCRIPTC_TARGET='wasm32-wasi', SCRIPTC_RUNTIME_PACK=str(runtime_pack), SCRIPTC_NO_CACHE='1',
               NODE_DISABLE_COMPILE_CACHE='1', ZIG_GLOBAL_CACHE_DIR=str(args.zig_global_cache_dir.resolve()))
    # Scriptc invokes zig by name. Bind that child to the same binary we identify.
    env['PATH'] = str(zig.parent) + os.pathsep + env['PATH']
    require(executable(shutil.which('zig', path=env['PATH'])) == zig,
            'Scriptc zig lookup differs from the identified binary')
    versions = {}
    for name, command in (('scriptw', [scriptw, 'compiler-identity']), ('scriptc', [scriptc, '--version']), ('zig', [zig, 'version'])):
        completed = subprocess.run(command, env=env, cwd=ROOT, capture_output=True, check=True, timeout=30)
        (output / 'logs' / (name + '-identity.stdout')).write_bytes(completed.stdout)
        (output / 'logs' / (name + '-identity.stderr')).write_bytes(completed.stderr)
        versions[name] = completed.stdout.decode().strip()
    tools = {name: identity(path) for name, path in (('scriptw', scriptw), ('scriptc', scriptc), ('zig', zig), ('time', timer))}
    tools['scriptw']['identity'] = json.loads(versions['scriptw'])
    tools['scriptc']['version'] = versions['scriptc'].removeprefix('scriptc ').strip()
    tools['zig']['version'] = versions['zig']
    require(tools['scriptc']['version'] == '0.4.5' and tools['zig']['version'] == '0.16.0', 'tool pins differ')
    require(tools['scriptw']['identity']['executable_sha256'] == tools['scriptw']['sha256'], 'frozen CLI identity differs')
    runtime_inputs = {str(path.relative_to(runtime_pack)): digest(path) for path in sorted(runtime_pack.rglob('*')) if path.is_file()}
    save(output / 'tooling/runtime-pack-inputs.json', runtime_inputs)
    shutil.copyfile(runtime_pack / 'runtime-pack.json', output / 'tooling/runtime-pack.json')
    tools['scriptc']['runtime_pack'] = dict(path=str(runtime_pack), manifest_sha256=digest(runtime_pack / 'runtime-pack.json'),
                                          tree_sha256=digest(output / 'tooling/runtime-pack-inputs.json'), files=len(runtime_inputs))
    tools['scriptc']['toolchain'], companion_inputs = retain_scriptc_toolchain(scriptc, output)
    tracked = [scriptw, scriptc, zig, timer, *companion_inputs, *[runtime_pack / name for name in runtime_inputs]]
    if args.package_lock:
        shutil.copyfile(args.package_lock, output / 'tooling/package-lock.json')
        tracked.append(args.package_lock.resolve())
    for name in ('benchmark-compilation.py', 'compilation_comparison.py', 'benchmark_metrics.py', 'compiler_comparison.py', 'limited_verification.py', 'resource_numbers.py'):
        path = ROOT / 'scripts' / name
        shutil.copyfile(path, output / 'tooling' / name)
        tracked.append(path)
    manifest = json.loads((FIXTURES / 'manifest.json').read_text())
    cases = manifest['execution']
    names = [case['file'] for case in cases]
    order = process_plan(names)
    shutil.copyfile(FIXTURES / 'manifest.json', output / 'sources/manifest.json')
    for case in cases:
        name = case['file']
        require(Path(name).name == name, 'unsafe case filename')
        original = (FIXTURES / name).read_bytes()
        (output / 'sources' / name).write_bytes(original)
        (output / 'adapted' / name).write_bytes(ADAPTER.encode() + original)
        (output / 'expected' / name).write_text(case['stdout'])
        tracked.append(FIXTURES / name)
    tracked.append(FIXTURES / 'manifest.json')
    before = {str(path): digest(path) for path in tracked}
    save(output / 'inputs-before.json', before)
    policy = dict(POLICY)
    plan = dict(schema=SCHEMA + '-plan', process_plan=order, policy=policy, tools=tools,
                work_root=str(output / 'work'),
                zig_global_cache_dir=env['ZIG_GLOBAL_CACHE_DIR'], timeout_seconds=args.timeout,
                machine=dict(platform=platform.platform(), affinity=sorted(os.sched_getaffinity(0)),
                             resources={name:(kernel_group() / name).read_text().strip() for name in
                                        ('memory.max', 'memory.swap.max', 'memory.oom.group', 'cpu.max')}))
    save(output / 'plan.json', plan)  # Frozen before any measured compilation.
    revision = tools['scriptw']['identity']['source_revision']
    require(revision['kind'] == 'git-commit', 'frozen CLI needs a Git source revision')
    report = dict(schema=SCHEMA, status='incomplete', source_commit=revision['commit'], measurement_commit=subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(), tools=tools, started_at=stamp(),
        workloads=cases, policy=policy, plan=plan, rows=[])
    for number, entry in enumerate(order):
        name, actor = entry['case'], entry['actor']
        stem = f'{number:03d}-{actor}-{name}'
        work = output / 'work' / stem
        work.mkdir()
        for folder in ('tmp', 'cache', 'zig-local'):
            (work / folder).mkdir()
        source_dir = output / 'sources'
        if actor == 'scriptc':
            source_dir = output / 'adapted'
        source = work / name
        shutil.copyfile(source_dir / name, source)
        wasm = work / 'program.wasm'
        child_env = dict(env)
        child_env.update(process_environment(work, runtime_pack, env['ZIG_GLOBAL_CACHE_DIR']))
        compiler = scriptc
        if actor == 'scriptw':
            compiler = scriptw
        argv, child_env = build_invocation(actor, compiler, source, wasm, child_env)
        selected_env = {key:value for key, value in child_env.items()
                        if key.startswith(('SCRIPTW_', 'SCRIPTC_', 'ZIG_')) or key in ('TMPDIR', 'NODE_DISABLE_COMPILE_CACHE')}
        row = dict(entry, argv=argv, environment=selected_env, source_sha256=digest(source),
                   raw='logs/' + stem + '.json', stdout='logs/' + stem + '.stdout', stderr='logs/' + stem + '.stderr')
        print(f'{number + 1}/{len(order)} {stem}', flush=True)
        with (output / row['stdout']).open('xb') as stdout, (output / row['stderr']).open('xb') as stderr:
            row['resources'] = measure_process(argv, cwd=work, env=child_env, stdout=stdout, stderr=stderr,
                                               timeout=args.timeout, time_binary=timer)
        report['rows'].append(row)
        save(output / 'incomplete-report.json', report)
        require(row['resources']['exit_code'] == 0 and not row['resources']['timed_out'], 'failed build: ' + stem)
        require(digest(source) == row['source_sha256'], 'compiler modified source')
        row['artifacts'] = dict(program=retain_artifact(wasm, output))
        if actor == 'scriptw':
            row['artifacts']['runtime'] = retain_artifact(Path(str(wasm) + '.runtime.wasm'), output)
        save(output / row['raw'], row)
        shutil.rmtree(work)  # Only this tool's per-process scratch; immutable artifacts/logs survive.
        save(output / 'incomplete-report.json', report)
    after = {str(path): digest(path) for path in tracked}
    save(output / 'inputs-after.json', after)
    require(before == after, 'compiler, toolchain, harness or original workload changed during measurement')
    report.update(status='complete', finished_at=stamp(), summary=summarize(report))
    report['retained_files'] = [dict(path=str(path.relative_to(output)), bytes=path.stat().st_size, sha256=digest(path))
                                for path in sorted(output.rglob('*')) if path.is_file() and path.name != 'incomplete-report.json']
    save(output / 'report.json', report)
    load_report(output / 'report.json')
    print(output / 'report.json')


def publish(args):
    report = load_report(args.report)
    args.output.mkdir(parents=True, exist_ok=False)
    archive_path = args.output / 'compilation-comparison.tar.gz'
    with tarfile.open(archive_path, 'w:gz') as archive:
        for row in report['retained_files']:
            archive.add(args.report.parent / row['path'], arcname=row['path'], recursive=False)
    report['archive'] = dict(file=archive_path.name, bytes=archive_path.stat().st_size, sha256=digest(archive_path))
    path = args.output / 'compilation-comparison.json'
    save(path, report)
    load_report(path)
    print(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    measure = commands.add_parser('run')
    for name in ('scriptw', 'scriptc', 'runtime-pack', 'time', 'output'):
        measure.add_argument('--' + name, type=Path, required=True)
    measure.add_argument('--zig', type=Path, default=Path('zig'))
    measure.add_argument('--zig-global-cache-dir', type=Path, default=Path(os.environ.get(
        'ZIG_GLOBAL_CACHE_DIR', str(Path(os.environ.get('XDG_CACHE_HOME', str(Path.home() / '.cache'))) / 'zig'))))
    measure.add_argument('--package-lock', type=Path)
    measure.add_argument('--timeout', type=int, default=180)
    publication = commands.add_parser('publish')
    publication.add_argument('report', type=Path)
    publication.add_argument('--output', type=Path, required=True)
    chart = commands.add_parser('render')
    chart.add_argument('report', type=Path)
    chart.add_argument('--svg', type=Path, required=True)
    chart.add_argument('--png', type=Path)
    chart.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if args.command == 'run':
        run(args)
    elif args.command == 'publish':
        publish(args)
    else:
        report = load_report(args.report)
        if args.png:
            args.png.parent.mkdir(parents=True, exist_ok=True)
        encoded = render(report, preview=args.png)
        if args.check:
            require(args.svg.read_bytes() == encoded, 'chart is stale')
        else:
            args.svg.parent.mkdir(parents=True, exist_ok=True)
            args.svg.write_bytes(encoded)


if __name__ == '__main__':
    main()
