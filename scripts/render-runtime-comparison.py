#!/usr/bin/env python3
"""Render the README's current three-runtime comparison from inert evidence."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path, PurePosixPath
import statistics
import tarfile

from benchmark_charts import gallery, render

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = ROOT / 'docs/rust-rewrite/performance-gaps-20261101.json'
DATA = ROOT / 'docs/rust-rewrite/execution-current.json'
ASSETS = ROOT / 'docs/rust-rewrite/assets/benchmarks-current'
RUNTIMES = ('scriptw', 'scriptc', 'v8')
README_START = '<!-- runtime-comparison:start -->'
README_END = '<!-- runtime-comparison:end -->'
DETAILS_START = '<!-- runtime-comparison-details:start -->'
DETAILS_END = '<!-- runtime-comparison-details:end -->'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def load_analysis(checkpoint):
    evidence = checkpoint['evidence']
    archive_path = ROOT / evidence['archive']
    assert archive_path.stat().st_size == evidence['bytes'] <= 128 * 1024**2
    assert digest(archive_path) == evidence['sha256']
    suffix = '/' + checkpoint['reference17']['evidence']['path']
    selected, identities, seen, index, expanded = [], [], set(), None, 0
    with tarfile.open(archive_path, 'r|xz') as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            assert member.isfile() and not path.is_absolute() and '..' not in path.parts
            assert '\\' not in member.name and member.name not in seen
            seen.add(member.name)
            expanded += member.size
            assert len(seen) <= 20_000 and 0 <= member.size <= 64 * 1024**2
            assert expanded <= 768 * 1024**2
            if member.name == 'INDEX.json':
                index = json.load(archive.extractfile(member))
            elif member.name.endswith(suffix):
                selected.append((member.name, archive.extractfile(member).read()))
            elif member.name.endswith('/candidate/identity.json'):
                identities.append((member.name, archive.extractfile(member).read()))
    assert len(seen) == evidence['regular_utf8_members']
    assert expanded == evidence['expanded_utf8_bytes']
    assert index['schema'] == 'scriptw-inert-evidence-index-v1' and len(selected) == 1
    assert len(identities) == 1
    for name, encoded in [selected[0], identities[0]]:
        entry = next(row for row in index['members'] if row['path'] == name)
        assert entry['bytes'] == len(encoded)
        assert entry['sha256'] == hashlib.sha256(encoded).hexdigest()
    assert hashlib.sha256(selected[0][1]).hexdigest() == checkpoint['reference17']['evidence']['sha256']
    identity = json.loads(identities[0][1])
    assert identity['source_fingerprint'] == checkpoint['source_fingerprint']
    return json.loads(selected[0][1]), identity


def chart_rows(analysis, fingerprint):
    assert analysis['schema'] == 'scriptw-three-runtime-comparison-v1'
    assert analysis['status'] == 'complete' and analysis['stage'] == 'three-runtime-reference'
    assert analysis['runtimes'] == list(RUNTIMES)
    assert analysis['processes'] == len(analysis['rows']) == 408
    assert set(analysis['source_fingerprints']) == {'scriptw', 'scriptc'}
    assert set(analysis['source_fingerprints'].values()) == {fingerprint}
    names = [row['case'] for row in analysis['summary']]
    assert len(names) == len(set(names)) == 17
    runner = importlib.util.spec_from_file_location('runtime_comparison', ROOT / 'scripts/benchmark-runtime-comparison.py')
    module = importlib.util.module_from_spec(runner)
    runner.loader.exec_module(module)
    expected = [(row['block'], row['case'], row['position'], row['actor'])
                for row in module.process_plan(names)]
    assert [(row['block'], row['case'], row['position'], row['actor'])
            for row in analysis['rows']] == expected
    rows = []
    for name in names:
        runtimes = {}
        for runtime in RUNTIMES:
            processes = [row for row in analysis['rows'] if row['case'] == name and row['actor'] == runtime]
            assert len(processes) == 8
            assert all(sum(row['block'] == block for row in processes) == 4 for block in range(2))
            for row in processes:
                assert len(row['wall_ns']) == 21
                assert all(math.isfinite(value) and value > 0 for value in row['wall_ns'])
                assert row['resources']['exit_code'] == 0 and not row['resources']['timed_out']
                assert row['resources']['peak_rss_bytes'] > 0
            runtimes[runtime] = dict(
                status='ok',
                ms=statistics.median(statistics.median(row['wall_ns']) for row in processes) / 1e6,
                mean_ms=statistics.mean(statistics.mean(row['wall_ns']) for row in processes) / 1e6,
                rss_mib=statistics.median(row['resources']['peak_rss_bytes'] for row in processes) / 1024**2,
                processes=8, measured_entries_per_process=21, warmups_per_process=10,
                block_median_ms=[next(row for row in analysis['summary'] if row['case'] == name)
                                 ['blocks'][block]['products'][runtime]['wall_ns']['median'] / 1e6
                                 for block in range(2)])
            if runtime == 'v8':
                for row in processes:
                    assert len(row['context_ns']) == 21
                    assert all(math.isfinite(value) and value > 0 for value in row['context_ns'])
                runtimes[runtime]['v8_context_ms'] = statistics.median(
                    statistics.median(row['context_ns']) for row in processes) / 1e6
        rows.append(dict(file=name, runtimes=runtimes))
    return rows


def replace_marked_block(text, start_marker, end_marker, section, label):
    if text.count(start_marker) != 1 or text.count(end_marker) != 1:
        raise ValueError(f'{label} needs exactly one ordered {start_marker} / {end_marker} pair')
    start = text.index(start_marker)
    end = text.index(end_marker)
    if start >= end:
        raise ValueError(f'{label} markers are reversed')
    end += len(end_marker)
    return text[:start] + section.rstrip('\n') + text[end:]


def comparison_summaries(rows):
    summaries = []
    for runtime, label in (('scriptc', 'Scriptc'), ('v8', 'V8')):
        wins = sum(all(row['runtimes']['scriptw']['block_median_ms'][block]
                       < row['runtimes'][runtime]['block_median_ms'][block] for block in range(2))
                   for row in rows)
        ratios = [math.exp(statistics.mean(math.log(row['runtimes']['scriptw']['block_median_ms'][block]
                                                   / row['runtimes'][runtime]['block_median_ms'][block])
                                           for row in rows)) for block in range(2)]
        summaries.append(f'Scriptw is faster than {label} in **{wins}/{len(rows)} workloads** in both blocks; '
                         f'block execution geometric means are **{min(ratios):.3f}–{max(ratios):.3f}× {label}**.')
    return summaries


def readme_section(authority, report, node, rows):
    scriptc, v8 = comparison_summaries(rows)
    return f'''{README_START}
**Runtime kernels · {authority['date']}**, compiler `{report['source_fingerprint'][:12]}`; Node **{node['versions']['node']}**, V8 **{node['versions']['v8']}**.
{scriptc}
{v8}
Wasm is compiled before timing; Scriptw entry includes Realm bootstrap. V8 creates a fresh context before timing; entry checks outputs.
See [measurement details and charts](docs/rust-rewrite/benchmarks.md#runtime-comparison), [data and identities](docs/rust-rewrite/execution-current.json), and [workflow](CONTRIBUTING.md#benchmark-charts).
<!-- runtime-comparison:end -->
'''


def details_section(authority, report, node, identity, rows):
    return f'''{DETAILS_START}
## Runtime comparison

**Measured {authority['date']} locally**, compiler fingerprint `{report['source_fingerprint'][:12]}`.
V8 **{node['versions']['v8']}**, Node **{node['versions']['node']}**.
{' '.join(comparison_summaries(rows))}

This compares Wasm public entry with V8 `runInContext`. Scriptw and Scriptc compile
Wasm before the measured processes and share a Wasmtime host with compilation disabled.
Wasmtime {identity['shared_configuration']['wasmtime_version']} uses {identity['shared_configuration']['native_compilation_mode']}/{identity['shared_configuration']['register_allocation']}
native code and a {identity['shared_configuration']['target']} target.
Timed Wasm entry includes Scriptw Realm/intrinsic bootstrap. V8 parses one `vm.Script`
per process and creates a fresh `vm.Context` before each invocation outside entry timing;
lazy compilation, JIT tiering and natural GC may occur inside. Every entry checks output.

Each bar shows the median of eight process medians: four processes per balanced
block, each with 21 measured entries after ten checked warmups. Memory bars show
median GNU-time whole-child **peak RSS** for all three runtimes, including host,
code, runtime and warmup history. Warmups do not establish optimized steady state.
Each panel uses its own zero-based scale. Chart data also retains V8 context-creation
time as a supplemental measurement, excluded from bars and ratios.

{gallery(rows, 'benchmarks-current', prefix='assets')}

[Chart data and runtime identities](execution-current.json) ·
[Complete evidence](performance-gaps-20261101.json) ·
[Workflow](../../CONTRIBUTING.md#benchmark-charts).
<!-- runtime-comparison-details:end -->
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--preview-dir', type=Path)
    parser.add_argument('--check', action='store_true', help='verify every generated byte without writing')
    args = parser.parse_args()
    authority = json.loads(AUTHORITY.read_text())
    checkpoint = authority['execution_three_runtime']
    analysis, identity = load_analysis(checkpoint)
    node = analysis['node_identity']
    assert len(node['executable_sha256']) == 64 and node['versions']['node'] and node['versions']['v8']
    import matplotlib
    report = dict(schema='scriptw-three-runtime-charts-v1',
                  source_commit=checkpoint['publication_commit'],
                  source_fingerprint=checkpoint['source_fingerprint'],
                  finished_at=authority['date'],
                  source_analysis=checkpoint['reference17']['evidence'],
                  consumer_identity=identity,
                  node_identity=node,
                  shared_configuration=identity['shared_configuration'],
                  archive=checkpoint['evidence'],
                  renderer=dict(matplotlib=matplotlib.__version__, script_sha256=digest(Path(__file__)),
                                shared_renderer_sha256=digest(ROOT / 'scripts/benchmark_charts.py')),
                  chart_panels=[('ms', 'Entry time · ms', 3), ('rss_mib', 'Peak RSS · MiB', 1)],
                  policy=dict(execution='Public-entry comparison: Scriptw and Scriptc Wasm compile before the measured processes; the shared CPP host disables compilation. Timed Wasm entry includes Scriptw Realm/intrinsic bootstrap. V8 parses one vm.Script per process and creates a fresh vm.Context before each invocation outside entry timing, then times runInContext. Lazy compilation, JIT tiering and natural GC may occur inside V8 entry timing. Every warmup and measured entry checks output.',
                              aggregation='Median of eight process medians for time; median of eight GNU-time child peaks for RSS. All process means and both block medians retained.',
                              sampling='Two balanced blocks, four processes per actor and case per block; 21 measured entries after ten warmups per process. Warmups do not establish optimized steady state.',
                              v8_context='Supplemental v8_context_ms is the median of eight process medians for fresh context creation, outside V8 entry timing; excluded from bars and ratios.',
                              source_commit_role='Publication commit matching the measured Rust source; measured binaries are identified by source_fingerprint.'))
    rows = chart_rows(analysis, report['source_fingerprint'])
    report['rows'] = rows
    outputs = {}
    for row in rows:
        preview = None
        if args.preview_dir:
            preview = args.preview_dir / (Path(row['file']).stem + '.png')
        outputs[ASSETS / (Path(row['file']).stem + '.svg')] = render(report, row, RUNTIMES, preview=preview)
    outputs[DATA] = (json.dumps(report, indent=2, ensure_ascii=False) + '\n').encode()
    readme = ROOT / 'README.md'
    readme_text = readme.read_text()
    detail_path = ROOT / 'docs/rust-rewrite/benchmarks.md'
    detail_text = detail_path.read_text()
    outputs[readme] = replace_marked_block(
        readme_text, README_START, README_END,
        readme_section(authority, report, node, rows), 'README.md').encode()
    outputs[detail_path] = replace_marked_block(
        detail_text, DETAILS_START, DETAILS_END,
        details_section(authority, report, node, identity, rows), 'benchmarks.md').encode()
    for path, encoded in outputs.items():
        if args.check:
            assert path.read_bytes() == encoded, f'generated output differs: {path.relative_to(ROOT)}'
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(encoded)
    print(f'Verified {len(rows)} current cards and README from {report["source_fingerprint"]}')


if __name__ == '__main__':
    main()
