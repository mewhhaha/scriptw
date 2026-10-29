"""Admission and publication of co-measured Scriptw, Scriptc/WASI and V8 evidence."""
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
import statistics
import tarfile

from benchmark_charts import gallery, render as render_card

SCHEMA = 'scriptw-three-compiler-benchmark-v1'
RUNTIMES = ('v8', 'scriptw', 'scriptc')
FIXTURES = 'crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths'
ADAPTER = 'function print(value) { console.log(String(value)); }\n'


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError('compiler comparison: ' + message)


def safe_name(name):
    path = PurePosixPath(name)
    require(not path.is_absolute() and '..' not in path.parts and '\\' not in name
            and ':' not in name and str(path) == name, 'unsafe retained path')
    return name


def load_report(path, archive=None):
    path = Path(path)
    report = json.loads(path.read_text())
    require(report['schema'] == SCHEMA and report['complete'] is True, 'incomplete or unknown report')
    require(re.fullmatch('[0-9a-f]{40}', report['source_commit']) is not None, 'invalid compiler revision')
    require(type(report['samples']) is int and 3 <= report['samples'] <= 100
            and type(report['warmups']) is int and 1 <= report['warmups'] <= 100
            and report['rounds'] == 2, 'invalid sampling plan')
    require(len(report['machine']['measurement_affinity']) == 1, 'measurement must be serial')
    require('0.4.5' in report['tools']['scriptc']['version']
            and report['tools']['zig']['version'] == '0.16.0', 'tool pins differ')
    files = {safe_name(row['path']): row for row in report['retained_files']}
    require(len(files) == len(report['retained_files']), 'duplicate retained path')
    retained = {}
    if archive is not None:
        require(digest(archive) == report['archive_sha256'], 'archive digest differs')
        size = 0
        with tarfile.open(archive, 'r|gz') as bundle:
            for member in bundle:
                name = safe_name(member.name)
                size += member.size
                require(member.isfile() and name in files and name not in retained
                        and 0 <= member.size <= 64 * 1024**2 and size <= 512 * 1024**2,
                        'invalid archive member or reading budget exceeded')
                retained[name] = bundle.extractfile(member).read()
        require(set(retained) == set(files), 'archive inventory differs')
    else:
        for name in files:
            candidate = path.parent / name
            require(candidate.is_file() and not candidate.is_symlink(), 'missing retained evidence')
            retained[name] = candidate.read_bytes()
    for name, row in files.items():
        value = retained[name]
        require(len(value) == row['bytes'] and hashlib.sha256(value).hexdigest() == row['sha256'],
                'retained evidence changed: ' + name)
    plan = json.loads(retained['sources/manifest.json'])
    require(plan['schema'] == 'scriptw-v8-workloads-v1' and plan['execution'] == report['workloads']
            and len(plan['execution']) == 17, 'complete original workload inventory required')
    corpus = Path(__file__).resolve().parents[1] / FIXTURES
    require(plan == json.loads((corpus / 'manifest.json').read_text()), 'canonical corpus differs')
    commands = {row['stdout']: row for row in report['commands']}
    require(len(commands) == len(report['commands']), 'duplicate command log')
    expected_names = [case['file'] for case in report['workloads']]
    require(len(set(expected_names)) == len(expected_names), 'duplicate workload')
    attempts = report['attempts']
    expected = {(round_index, name, runtime) for round_index in range(2)
                for name in expected_names for runtime in RUNTIMES}
    actual = [(row['round'], row['file'], row['runtime']) for row in attempts]
    require(len(actual) == len(set(actual)) and set(actual) == expected, 'attempt inventory differs')
    by_key = {key: row for key, row in zip(actual, attempts)}
    rows = []
    for case in report['workloads']:
        name = safe_name(case['file'])
        original = retained['sources/' + name]
        require(original == (corpus / name).read_bytes(), 'original source differs')
        require(retained['adapted/' + name] == ADAPTER.encode() + original, 'workload adaptation differs')
        require(retained['expected/' + name] == case['stdout'].encode(), 'expected stdout differs')
        runtimes = {}
        for runtime in RUNTIMES:
            times, rss, failures = [], [], []
            for round_index in range(2):
                attempt = by_key[round_index, name, runtime]
                status = attempt['status']
                require(status in ('ok', 'compile-failed', 'run-failed', 'checksum-mismatch'), 'unknown attempt status')
                require(all(log in retained for log in attempt['logs']), 'missing attempt diagnostic')
                if status != 'ok':
                    require(runtime == 'scriptc', 'reference runtime failed')
                    if status in ('compile-failed', 'run-failed'):
                        require(any(commands[log]['returncode'] != 0 for log in attempt['logs'] if log in commands),
                                'failure has no failed command')
                    else:
                        failed = json.loads(retained[attempt['raw']])['observations']
                        require(failed and any(row['matched'] is False and (
                            row['stdout'] != case['stdout'] or row['stderr'] != '' or row['completion'] != 'normal')
                            for row in failed), 'failure has no changed output or completion')
                    failures.append(status)
                    continue
                require(commands[attempt['raw']]['returncode'] == 0, 'successful case has failed command')
                raw = json.loads(retained[attempt['raw']])
                if runtime == 'scriptc':
                    require(raw['schema'] == 'scriptw-wasi-baseline-v1' and raw['wasmtime_version'] == '47.0.0',
                            'unexpected WASI runner')
                    require(raw['wasm_sha256'] == files[attempt['wasm']]['sha256'], 'WASI module identity differs')
                    observations = raw['observations']
                else:
                    schema = 'scriptw-wasmtime-baseline-v2' if runtime == 'scriptw' else 'scriptw-v8-baseline-v2'
                    require(raw['schema'] == schema and len(raw['cases']) == 1
                            and raw['cases'][0]['file'] == name, 'unexpected runtime case')
                    require(raw['cases'][0]['source_sha256'] == hashlib.sha256(original).hexdigest(),
                            'runtime measured a different source')
                    if runtime == 'scriptw':
                        require(raw['source_commit'] == report['source_commit'], 'measured compiler revision differs')
                        require(raw['requested_native_policy'] is None
                                and raw['source_fingerprint'] == report['scriptw_source_fingerprint'],
                                'compiler fingerprint or native policy differs')
                        require(raw['executable_sha256'] == report['tools']['runtime_baseline']['sha256'],
                                'measured runtime executable differs')
                        require(raw['cases'][0]['wasm_sha256'] == files[attempt['wasm']]['sha256'],
                                'Scriptw module identity differs')
                    observations = raw['cases'][0]['observations']
                    if runtime == 'v8':
                        require(raw['node'] == report['tools']['node']['version']
                                and raw['v8'] == report['tools']['node']['v8'], 'V8 version differs')
                require(raw['samples'] == report['samples']
                        and (runtime == 'v8' or raw['warmups'] == report['warmups'])
                        and len(observations) == report['samples'] + report['warmups'], 'observation plan differs')
                for index, observation in enumerate(observations):
                    require(observation['warmup'] is (index < report['warmups'])
                            and observation['stdout'] == case['stdout'], 'warmup plan or checksum differs')
                    if runtime == 'scriptc':
                        require(observation['matched'] is True and observation['completion'] == 'normal'
                                and observation['stderr'] == '', 'WASI completion differs')
                        memory = observation['rss_after_bytes']
                    elif runtime == 'scriptw':
                        require(observation['runtime_policy'] == report['scriptw_runtime_policy'], 'runtime policy differs')
                        memory = observation['memory']['process_rss']['final_rss_bytes']
                    else:
                        memory = observation['memory']['after']['rss']
                    for value in (observation['execution_ns'], memory):
                        require(type(value) in (int, float) and math.isfinite(value) and value > 0,
                                'invalid timing or memory observation')
                    if index >= report['warmups']:
                        times.append(observation['execution_ns'])
                        rss.append(memory)
            runtimes[runtime] = dict(status=', '.join(sorted(set(failures))) if failures else 'ok',
                                     ms=statistics.median(times) / 1e6 if not failures else None,
                                     rss_mib=statistics.median(rss) / 1024**2 if not failures else None)
        rows.append(dict(file=name, runtimes=runtimes))
    return report, rows


def chart_files(rows):
    return [(Path('benchmarks') / (Path(row['file']).stem + '.svg'), row) for row in rows]


def render(report, row, preview=None):
    return render_card(report, row, RUNTIMES, preview=preview)


def publish(report_path, root):
    root, report_path = Path(root), Path(report_path)
    report, rows = load_report(report_path)
    destination = root / 'docs/rust-rewrite'
    archive_path = destination / 'compiler-comparison.tar.gz'
    temporary = archive_path.with_suffix('.tmp')
    with tarfile.open(temporary, 'w:gz') as archive:
        for row in report['retained_files']:
            archive.add(report_path.parent / row['path'], arcname=row['path'], recursive=False)
    report['archive_sha256'] = digest(temporary)
    target = destination / 'compiler-comparison.json'
    target.write_text(json.dumps(report, indent=2) + '\n')
    temporary.replace(archive_path)
    load_report(target, archive_path)
    for relative, row in chart_files(rows):
        target = destination / 'assets' / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(render(report, row))
    (destination / 'compiler-comparison.md').write_text(markdown(report, rows))
    readme_path = root / 'README.md'
    readme = readme_path.read_text()
    start, end = readme.index('## Benchmarks'), readme.index('## Tests')
    readme_path.write_text(readme[:start] + readme_section(report, rows) + readme[end:])


def markdown(report, rows):
    lines = ['# GitHub compiler comparison', '',
             f"Measured {report['finished_at']}; Scriptw source `{report['source_commit']}`.", '',
             f"Scriptc `{report['tools']['scriptc']['version']}`, Zig `{report['tools']['zig']['version']}`; "
             f"{report['tools']['node']['version']}. Wasmtime 47.0.0 with the product Fast/copying-GC configuration.", '',
             'All runtimes ran together on one GitHub-hosted runner, pinned to one CPU. '
             'Each case has two rounds, each with ten warmups and seven measured invocations. '
             'Source/Wasm compilation and instance/context setup are excluded from execution time. '
             'RSS is the post-execution process endpoint with the instance alive; it includes the host, '
             'compilation and retained warmup effects, and is neither live heap nor peak RSS.', '',
             'Scriptw and V8 compile JavaScript in their measured processes; Scriptc compiles JavaScript '
             'in a separate process. Its host RSS includes Wasm native compilation but excludes its '
             'JavaScript frontend process. These are host-process footprints under the recorded paths.', '',
             'Scriptc receives the unchanged JavaScript prefixed only with the recorded `print` adapter. '
             'Compile failures, runtime failures and checksum mismatches retain diagnostics and have no bars. '
             'These are descriptive shared-host observations, not a conformance measurement.', '',
             '| Workload | Scriptw ms | V8 ms | Scriptc ms / status |', '| --- | ---: | ---: | --- |']
    for row in rows:
        values = row['runtimes']
        scriptc = f"{values['scriptc']['ms']:.3f}" if values['scriptc']['status'] == 'ok' else values['scriptc']['status']
        lines.append(f"| {row['file']} | {values['scriptw']['ms']:.3f} | {values['v8']['ms']:.3f} | {scriptc} |")
    lines += ['', '## Benchmark charts', '', gallery(rows, 'benchmarks'), '',
              '[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · '
              '[Workflow and reproduction](contracts/compiler-comparison.md).', '']
    return '\n'.join(lines)


def readme_section(report, rows):
    count = sum(row['runtimes']['scriptc']['status'] == 'ok' for row in rows)
    return f'''## Benchmarks

**Measured {report['finished_at'][:10]}**, compiler `{report['source_commit'][:9]}`.
Scriptw / Wasmtime, Scriptc / Wasmtime (WASI), and Node / V8; **{count}/17 Scriptc checksums passed**.

Each image has its own zero-based scales. Bars show median execution time and
post-execution process RSS. Compilation and instance/context setup are outside
the timer; RSS includes compilation and warmup history, not live heap or peak memory.

{gallery(rows, 'benchmarks', prefix='docs/rust-rewrite/assets')}

[Measurements and complete evidence](docs/rust-rewrite/compiler-comparison.md) ·
[Workflow and reproduction](docs/rust-rewrite/contracts/compiler-comparison.md) ·
[Toolchains and refresh provenance](docs/rust-rewrite/benchmark-refresh-20261028.md).

'''
