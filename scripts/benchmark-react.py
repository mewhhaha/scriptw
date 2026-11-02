#!/usr/bin/env python3
"""Real React server/browser references and bounded standalone capability attempts."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess

from application_benchmarks import require, checked_identity, successful_process, checked_product_stdout
from benchmark_metrics import digest, measure_process, summary
from compilation_comparison import ADAPTER, process_environment, scriptc_companions
from limited_verification import kernel_group, limited_environment, require_kernel_budget

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'crates/scriptw-cli/tests/fixtures/edge_rendering'
DRIVER = SOURCE / 'react-driver.cjs'
CASES = ('rsc-sync', 'rsc-async', 'web-url', 'web-json')
FIELDS = ('memory.max', 'memory.swap.max', 'memory.oom.group', 'cpu.max', 'memory.events')
load = lambda path: json.loads(Path(path).read_text())


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def executable(value):
    return Path(shutil.which(str(value)) or value).resolve(strict=True)


def resources():
    return {name: (kernel_group() / name).read_text().strip() for name in FIELDS}


def unchanged(left, right):
    require(all(left[name] == right[name] for name in FIELDS if name != 'memory.events'), 'resource policy changed')
    before, after = [dict(line.split() for line in value['memory.events'].splitlines()) for value in (left, right)]
    require(all(before.get(key) == after.get(key) for key in ('oom', 'oom_kill', 'oom_group_kill')), 'cgroup OOM event')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('prepare', 'run'))
    for name in ('tools', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    for name in ('fixtures', 'frozen', 'scriptw', 'scriptc', 'runtime-pack', 'zig', 'chromium'):
        parser.add_argument('--' + name, type=Path)
    parser.add_argument('--node', default='node')
    parser.add_argument('--time', default='/usr/bin/time')
    parser.add_argument('--timeout', type=float, default=120)
    args = parser.parse_args()
    require_kernel_budget(4 * 1024**3)
    require(len(os.sched_getaffinity(0)) == 1 and 0 < args.timeout <= 120, 'one CPU and positive deadline at most120s required')
    output, tools = args.output.resolve(), args.tools.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    for folder in ('logs', 'tmp', 'cache', 'work'):
        (output / folder).mkdir()
    node, timer = executable(args.node), executable(args.time)
    env = limited_environment()
    for key in ('NODE_OPTIONS', 'LD_PRELOAD', 'LD_AUDIT'):
        require(not env.get(key), 'forbidden inherited setting: ' + key)
    for key in list(env):
        if key.startswith(('SCRIPTW_', 'SCRIPTC_', 'ZIG_')) or key == 'NODE_COMPILE_CACHE':
            del env[key]
    env.update(NODE_DISABLE_COMPILE_CACHE='1', TMPDIR=str(output / 'tmp'),
        SCRIPTW_CACHE_DIR=str(output / 'cache'), SCRIPTW_JOBS='1', SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES='1',
        SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES=str(64 * 1024**2))
    # Chromium's SingletonSocket must fit in a Unix-domain socket pathname.
    browser_tmp = ROOT / 'target/react-browser-tmp'
    browser_tmp.mkdir(exist_ok=True)
    browser_env = dict(env, TMPDIR=str(browser_tmp))
    tracked = {node, timer, DRIVER, Path(__file__).resolve()}
    tracked.update(path for path in SOURCE.iterdir() if path.is_file())
    # Bind all installed upstream/tool bytes, including Playwright's browser protocol implementation.
    tracked.update(path.resolve() for path in tools.rglob('*') if path.is_file())
    report = dict(schema='scriptw-react-benchmark-v1', status='incomplete', mode=args.mode,
        started_at=datetime.now(timezone.utc).isoformat(), samples=7, warmups=3, commands=[], references=[], capability=[],
        resources_before=resources(), affinity=sorted(os.sched_getaffinity(0)), policy=dict(
            server='One vm.Context initialized once per fresh process; parse and initialization outside entry wall. Await complete Flight/Web response consumption; output validation outside timer. Node genuine standard Web APIs.',
            browser='Eight fresh Chromium processes, three modes each; three checked warmups and seven checked samples per mode. React commit plus forced layout inside page clock; setup, validation, cleanup and screenshot excluded.',
            memory='GNU time lifetime child high-water RSS including accounted descendants. Browser process value covers Node plus browser descendant high-water accounting, not simultaneous tree RSS, per-case RSS or live JS heap.',
            capability='Actual standalone build then execution attempts; failures preserve raw diagnostics and are not benchmark zeros or permanent exclusions.',
            statistics='Two blocks, four fresh processes per server case/block and four browser processes/block; process medians are units; no significance or compatibility claim.'))

    def checkpoint():
        save(output / 'report.json', report)

    def invoke(stage, argv, child_env=env, cwd=ROOT):
        stem = f'{len(report["commands"]):03}-{stage}'
        out, err = output / 'logs' / (stem + '.stdout'), output / 'logs' / (stem + '.stderr')
        row = dict(stage=stage, argv=list(map(str, argv)), stdout=str(out.relative_to(output)), stderr=str(err.relative_to(output)),
            environment={key: value for key, value in child_env.items() if key.startswith(('SCRIPTW_', 'SCRIPTC_', 'NODE_', 'ZIG_')) or key == 'TMPDIR'})
        before = resources()
        with out.open('xb') as stdout, err.open('xb') as stderr:
            row['resources'] = measure_process(argv, cwd=cwd, env=child_env, stdout=stdout, stderr=stderr,
                timeout=args.timeout, time_binary=timer)
        unchanged(before, resources())
        row.update(stdout_sha256=digest(out), stderr_sha256=digest(err), status='failed')
        if successful_process(row['resources']):
            row['status'] = 'succeeded'
        report['commands'].append(row)
        checkpoint()
        print(stem + ': ' + row['status'], flush=True)
        return row

    def checked(row):
        require(row['status'] == 'succeeded', 'child failed: ' + row['stdout'])
        require(not (output / row['stderr']).read_bytes(), 'reference stderr: ' + row['stderr'])
        return load(output / row['stdout'])

    if args.mode == 'run':
        require(all(getattr(args, name) is not None for name in ('fixtures', 'frozen', 'scriptc', 'runtime_pack', 'zig', 'chromium')), 'run requires fixtures/frozen/scriptc/runtime-pack/zig/chromium')
        fixtures, frozen = args.fixtures.resolve(strict=True), args.frozen.resolve(strict=True)
        scriptw = args.scriptw
        if scriptw is None:
            scriptw = frozen / 'candidate-ordinary/scriptw'
        scriptw, scriptc, zig, chromium = map(executable, (scriptw, args.scriptc, args.zig, args.chromium))
        host = frozen / 'candidate/compiler_precompiled_baseline'
        identity = load(frozen / 'candidate/identity.json')
        require(digest(host) == identity['executable_sha256'], 'frozen host identity changed')
        producer = load(frozen / 'runtime-product-producer-candidate.json')
        require(producer['status'] == 'verified' and digest(frozen / producer['build_stdout']) == producer['build_stdout_sha256'], 'runtime producer differs')
        tracked.update((scriptw, scriptc, zig, chromium, host, frozen / producer['build_stdout']))
        for name, receipt in producer['files'].items():
            path = frozen / 'candidate/runtime-build' / name
            require(digest(path) == receipt['sha256'], 'runtime producer file changed')
            if 'package' in receipt:
                require(receipt['package']['package_source_fingerprint'] == identity['source_fingerprint'], 'runtime producer source differs')
            tracked.add(path)
        witness = load(frozen / 'publication-rust-witness.json')
        for name, sha in witness['files'].items():
            require(digest(ROOT / name) == sha, 'frozen Rust source changed: ' + name)
            tracked.add(ROOT / name)
        qualification = load(frozen / 'native-qualified.json')
        require(qualification['status'] == 'complete' and qualification['source_fingerprint'] == identity['source_fingerprint'], 'native qualification differs')
        tracked.update(frozen / name for name in ('candidate/identity.json', 'publication-rust-witness.json',
            'native-qualified.json', 'runtime-product-producer-candidate.json'))
        tracked.update(path for path in fixtures.iterdir() if path.is_file())
        runtime_pack = args.runtime_pack.resolve(strict=True)
        manifest = Path(str(scriptc) + '.json')
        tracked.add(manifest)
        for selected in (runtime_pack, *map(Path, scriptc_companions(scriptc, load(manifest)).values())):
            if selected.is_dir():
                tracked.update(path for path in selected.rglob('*') if path.is_file())
            else:
                tracked.add(selected)
        env['PATH'] = str(zig.parent) + os.pathsep + env['PATH']
    report['inputs_before'] = {str(path): digest(path) for path in sorted(tracked)}
    checkpoint()
    if args.mode == 'prepare':
        report['build'] = checked(invoke('bundle-preflight', [node, DRIVER, 'build', tools, SOURCE, output]))
    else:
        support = load(fixtures / 'support.json')
        report['versions'] = support['versions']
        for name, sha in support['inputs'].items():
            require(digest(name) == sha, 'bundled upstream input changed: ' + name)
        host_identity = checked(invoke('host-identity', [host, 'identity']))
        checked_identity(host_identity, identity)
        ordinary = checked(invoke('ordinary-identity', [scriptw, 'compiler-identity']))
        require(ordinary['executable_sha256'] == digest(scriptw) and ordinary['source_fingerprint'] == identity['source_fingerprint'], 'ordinary compiler differs from frozen cohort')
        report['ordinary_identity'] = ordinary
        report['node_identity'] = checked(invoke('node-identity', [node, '-p', 'JSON.stringify(process.versions)']))
        for block in range(2):
            order = list(CASES)
            if block:
                order.reverse()
            for process_index in range(4):
                for name in order:
                    raw = checked(invoke(name, [node, DRIVER, 'server', fixtures, name]))
                    require(raw['case'] == name and raw['samples'] == 7 and raw['warmups'] == 3 and len(raw['observations']) == 10, 'server observation inventory differs')
                    for index, row in enumerate(raw['observations']):
                        require(row['index'] == index and row['warmup'] == (index < 3) and row['stdout'] == support['expected'][name] and type(row['elapsed_ns']) is int and row['elapsed_ns'] > 0, 'server observation invalid')
                    report['references'].append(dict(kind='server', block=block, process=process_index, raw=raw))
                index = block * 4 + process_index
                screenshot = ''
                if index == 7:
                    screenshot = str(output / 'react-client.png')
                raw = checked(invoke('browser', [node, DRIVER, 'browser', tools, fixtures, chromium, str(index), screenshot], browser_env))
                require(raw['process_index'] == index and raw['cross_origin_isolated'] is True and raw['chromium_sha256'] == digest(chromium) and len(raw['observations']) == 30, 'browser observation inventory differs')
                report['references'].append(dict(kind='browser', block=block, process=process_index, raw=raw))
                checkpoint()
        for name in (*CASES, 'client-mount'):
            for actor in ('scriptw', 'scriptc'):
                work = output / 'work' / (actor + '-' + name)
                work.mkdir()
                for folder in ('tmp', 'cache', 'zig-local'):
                    (work / folder).mkdir()
                source, wasm = work / 'input.js', work / 'program.wasm'
                text = (fixtures / ('capability-' + name + '.js')).read_text()
                if actor == 'scriptc':
                    text = ADAPTER + text
                source.write_text(text)
                child_env = dict(env, **process_environment(work, runtime_pack, Path.home() / '.cache/zig'))
                argv = [scriptc, 'build', source, '-o', wasm]
                if actor == 'scriptw':
                    argv = [scriptw, 'build', 'wasm', source]
                    child_env['SCRIPTW_WASM_DUMP'] = str(wasm)
                build = invoke(actor + '-' + name + '-build', argv, child_env, work)
                attempt = dict(actor=actor, case=name, build=build, status='failed', stage='source-to-wasm')
                if build['status'] != 'succeeded':
                    attempt['reason'] = (output / build['stderr']).read_text(errors='replace')[-4000:]
                report['capability'].append(attempt)
                if build['status'] == 'succeeded':
                    require(wasm.read_bytes()[:8] == b'\0asm\1\0\0\0', 'build did not emit Wasm')
                    attempt.update(wasm_sha256=digest(wasm), stage='execution')
                    argv = [scriptw, 'run', source]
                    if actor == 'scriptc':
                        native = work / 'program.swpc'
                        preparation = invoke(actor + '-' + name + '-native', [host, 'prepare-wasi', wasm, native], child_env, work)
                        attempt['native_preparation'] = preparation
                        if preparation['status'] != 'succeeded':
                            attempt['stage'] = 'wasm-to-native'
                            continue
                        argv = [host, 'execute-wasi', native, support['expected'][name], '7', '3']
                    execution = invoke(actor + '-' + name + '-execute', argv, child_env, work)
                    attempt['execution'] = execution
                    if execution['status'] != 'succeeded':
                        attempt['reason'] = (output / execution['stderr']).read_text(errors='replace')[-4000:]
                    if execution['status'] == 'succeeded':
                        actual = (output / execution['stdout']).read_text()
                        matched = False
                        if actor == 'scriptw':
                            try:
                                matched = checked_product_stdout(actual, support['expected'][name])
                            except ValueError:
                                matched = False
                        if actor == 'scriptc':
                            rows = json.loads(actual)['observations']
                            matched = len(rows) == 10 and all(row['stdout'] == support['expected'][name] and row['matched'] and row['completion'] == 'normal' for row in rows)
                        if matched and not (output / execution['stderr']).read_bytes():
                            attempt['status'] = 'checked'
                        else:
                            attempt['reason'] = 'Output/completion mismatch; inspect raw execution logs'
                checkpoint()
        report['summaries'] = []
        for name in (*CASES, 'mount', 'update', 'hydrate'):
            groups = []
            for entry in report['references']:
                rows = entry['raw']['observations']
                if entry['kind'] == 'server' and entry['raw']['case'] != name:
                    continue
                if entry['kind'] == 'browser':
                    rows = [row for row in rows if row['mode'] == name]
                if rows:
                    values = [row['elapsed_ns'] for row in rows if not row['warmup']]
                    groups.append(dict(block=entry['block'], process=entry['process'], entry_wall_ns=summary(values)))
            require(len(groups) == 8, 'eight fresh processes required: ' + name)
            report['summaries'].append(dict(case=name, processes=groups, process_medians_ns=summary([round(group['entry_wall_ns']['median']) for group in groups])))
    report['inputs_after'] = {str(path): digest(path) for path in sorted(tracked)}
    require(report['inputs_before'] == report['inputs_after'], 'sources/tools changed during campaign')
    report['resources_after'] = resources()
    unchanged(report['resources_before'], report['resources_after'])
    report.update(status='complete', finished_at=datetime.now(timezone.utc).isoformat())
    checkpoint()


if __name__ == '__main__':
    main()
