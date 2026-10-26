"""Admission and publication of co-measured Scriptw, Scriptc/WASI and V8 evidence."""
import hashlib
import io
import json
import math
from pathlib import Path, PurePosixPath
import re
import statistics
import tarfile

SCHEMA = 'scriptw-three-compiler-benchmark-v1'
RUNTIMES = ('v8', 'scriptw', 'scriptc')
LABELS = {'v8': 'Node / V8', 'scriptw': 'Scriptw / Wasmtime', 'scriptc': 'Scriptc / Wasmtime (WASI)'}
COLORS = {'v8': '#de8b36', 'scriptw': '#3378b5', 'scriptc': '#36835b'}
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


def render(report, rows, preview=None):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.patches import Patch
    with plt.rc_context({'font.family': 'DejaVu Sans', 'svg.fonttype': 'none',
                         'svg.hashsalt': 'scriptw-three-compilers-v1'}):
        fig, axes = plt.subplots(2, 1, figsize=(15, 20))
        try:
            fig.subplots_adjust(left=.24, right=.96, top=.91, bottom=.10, hspace=.23)
            fig.suptitle('JavaScript compiler comparison', fontsize=23, x=.04, ha='left', y=.985)
            fig.text(.04, .958, f"Measured {report['finished_at'][:10]} · compiler {report['source_commit'][:9]} · "
                     f"{report['samples'] * 2} samples · {report['warmups']} warmups per round", color='#57606a')
            fig.legend(handles=[Patch(color=COLORS[runtime], label=LABELS[runtime]) for runtime in RUNTIMES],
                       loc='upper right', bbox_to_anchor=(.97, .987), frameon=False)
            for ax, metric, title, unit in zip(axes, ('ms', 'rss_mib'),
                    ('Elapsed execution time', 'Process memory after execution'), ('ms', 'MiB')):
                ax.set_title(title + ' · lower is better', loc='left', pad=16)
                maximum = max(row['runtimes'][runtime][metric] or 0 for row in rows for runtime in RUNTIMES)
                ax.set_xlim(0, maximum * 1.32)
                ax.set_ylim(len(rows) - .5, -.5)
                ax.set_yticks(range(len(rows)), [row['file'] for row in rows])
                ax.set_axisbelow(True)
                ax.grid(axis='x', color='#eaeef2')
                for spine in ax.spines.values(): spine.set_visible(False)
                for index, row in enumerate(rows):
                    for runtime_index, runtime in enumerate(RUNTIMES):
                        item = row['runtimes'][runtime]
                        y = index + (runtime_index - 1) * .23
                        if item['status'] == 'ok':
                            value = item[metric]
                            ax.barh(y, value, height=.20, color=COLORS[runtime])
                            ax.annotate(f'{value:.3f}' if metric == 'ms' else f'{value:.1f}',
                                        (value, y), xytext=(4, 0), textcoords='offset points',
                                        va='center', fontsize=8, color=COLORS[runtime])
                        else:
                            ax.text(maximum * .02, y, item['status'], color='#b42318', fontsize=8, va='center')
                ax.set_xlabel(unit)
            fig.text(.04, .065, 'Fresh Store / V8 context per invocation; source and Wasm compilation and instance setup excluded.', fontsize=10)
            fig.text(.04, .047, 'RSS is whole host-process memory. Scriptw/V8 compile JS in-process; Scriptc compiles JS separately. Not live heap or peak RSS.', fontsize=10)
            fig.text(.04, .029, 'All 17 workloads retained. Failed Scriptc cases have labels instead of timing/memory bars. Shared-host results are descriptive.', fontsize=10)
            output = io.BytesIO()
            fig.savefig(output, format='svg', metadata={'Date': None,
                'Title': 'Scriptw, Scriptc WASI and V8 execution and process RSS',
                'Description': json.dumps(rows, sort_keys=True)})
            if preview: fig.savefig(preview, dpi=90)
            return output.getvalue()
        finally:
            plt.close(fig)


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
    (destination / 'assets/execution-performance.svg').write_bytes(render(report, rows))
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
    lines += ['', '[Checked report](compiler-comparison.json) · [Raw evidence](compiler-comparison.tar.gz) · '
              '[Workflow and reproduction](contracts/compiler-comparison.md).', '']
    (destination / 'compiler-comparison.md').write_text('\n'.join(lines))
    readme_path = root / 'README.md'
    readme = readme_path.read_text()
    start = readme.index('## Benchmarks')
    end = readme.index('## Tests', start)
    count = sum(row['runtimes']['scriptc']['status'] == 'ok' for row in rows)
    section = f'''## Benchmarks

![Scriptw, Scriptc WASI and V8: execution time and post-execution process RSS for all 17 workloads](docs/rust-rewrite/assets/execution-performance.svg)

**Measured {report['finished_at'][:10]} on GitHub Actions**, compiler `{report['source_commit'][:9]}`.
All three runtimes were measured together: **Scriptw / Wasmtime**, **Scriptc / Wasmtime (WASI)**
and **Node / V8**. Scriptc passed the original checksums on **{count}/17 workloads**;
the remaining cases retain explicit failure labels and diagnostics.

Bars show median execution time and post-execution process RSS, with compilation
and instance/context setup outside the execution timer. RSS includes the host/runtime,
compilation and retained warmup effects; it is neither live JavaScript heap nor peak memory.
Shared-runner observations are descriptive. The existing pinned Test262 status below is separate.

[Measurements and complete evidence](docs/rust-rewrite/compiler-comparison.md) ·
[Run or verify the benchmark workflow](docs/rust-rewrite/contracts/compiler-comparison.md).

'''
    readme_path.write_text(readme[:start] + section + readme[end:])
