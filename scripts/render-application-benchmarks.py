#!/usr/bin/env python3
"""Render application cards from verified inert evidence, without executing programs."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import statistics
import tarfile

from application_benchmarks import (ACTORS, SCHEMA, checked_observations, require,
                                    checked_product_stdout, successful_process, summarize, validate_manifest)
from benchmark_charts import gallery, render
from benchmark_metrics import digest, summary

ROOT = Path(__file__).resolve().parents[1]
ARCHIVED_ROOT = ROOT
AUTHORITY = ROOT / 'docs/rust-rewrite/applications-current.json'
ASSETS = AUTHORITY.parent / 'assets/applications-current'
SERVER = ('rsc-sync', 'rsc-async', 'web-url', 'web-json')
BROWSER = ('mount', 'update', 'hydrate')
LABELS = {'rsc-sync': 'RSC Flight · sync', 'rsc-async': 'RSC Flight · async',
          'web-url': 'URL / Request', 'web-json': 'JSON / Request',
          'mount': 'React mount', 'update': 'React update', 'hydrate': 'React hydration'}


def sha(value):
    return hashlib.sha256(value).hexdigest()


def safe_path(name):
    path = PurePosixPath(name)
    require(bool(path.parts) and not path.is_absolute() and '..' not in path.parts
            and str(path) == name and '\\' not in name and ':' not in name, 'unsafe evidence member')
    return name


def read_archive(authority):
    receipt = authority['archive']
    archive_path = AUTHORITY.parent / safe_path(receipt['file'])
    require(archive_path.stat().st_size == receipt['bytes'] <= 128 * 1024**2
            and digest(archive_path) == receipt['sha256'], 'archive identity differs')
    files, expanded = {}, 0
    with tarfile.open(archive_path, 'r|xz') as archive:
        for member in archive:
            name = safe_path(member.name)
            expanded += member.size
            require(member.isfile() and name not in files and len(files) < 20_000
                    and 0 <= member.size <= 64 * 1024**2 and expanded <= 512 * 1024**2,
                    'archive type, duplicate member or reading budget differs')
            value = archive.extractfile(member).read()
            value.decode('utf-8', errors='strict')
            require(b'\0' not in value, 'evidence must be inert UTF-8 text')
            files[name] = value
    require(len(files) == receipt['regular_utf8_members'] and expanded == receipt['expanded_utf8_bytes'],
            'archive member inventory differs')
    index = json.loads(files['INDEX.json'])
    require(index['schema'] == 'scriptw-inert-evidence-index-v1', 'foreign evidence index')
    require(files['INDEX.json.sha256'] == (sha(files['INDEX.json']) + '  INDEX.json\n').encode(),
            'index SHA sidecar differs')
    indexed = {safe_path(row['path']): row for row in index['members']}
    require(len(indexed) == len(index['members']) and set(indexed) == set(files) - {'INDEX.json', 'INDEX.json.sha256'},
            'index member inventory differs')
    for name, row in indexed.items():
        require(name.startswith('evidence/') and row['bytes'] == len(files[name])
                and row['sha256'] == sha(files[name]), 'indexed evidence digest differs: ' + name)
    return files


def encoded(files, path):
    path = Path(path)
    if path.is_absolute():
        path = path.relative_to(ARCHIVED_ROOT)
    return files['evidence/' + safe_path(path.as_posix())]


def decoded(files, path):
    return json.loads(encoded(files, path))


def checked_report(files, receipt):
    value = encoded(files, receipt['path'])
    require(sha(value) == receipt['sha256'], 'report digest differs')
    report = json.loads(value)
    require(report['status'] == 'complete' and report['inputs_before'] == report['inputs_after'],
            'incomplete report or source/tool mutation')
    return report, Path(receipt['path']).parent


def check_command(files, base, command):
    for field in ('stdout', 'stderr'):
        require(sha(encoded(files, base / command[field])) == command[field + '_sha256'],
                'command raw log differs: ' + command[field])
    if command['status'] in ('succeeded', 'sampled'):
        require(successful_process(command['resources']), 'failed process labeled successful')


def portable_rows(files, receipt, fingerprint):
    report, base = checked_report(files, receipt)
    require(report['schema'] == SCHEMA and report['source_fingerprint'] == fingerprint
            and report['consumer_identity']['source_fingerprint'] == fingerprint, 'portable compiler identity differs')
    cases = validate_manifest(dict(execution=report['workloads']))
    require(len(cases) == 7 and report['summary'] == summarize(report), 'portable summary or inventory differs')
    require(decoded(files, base / 'sources/manifest.json')['execution'] == cases, 'portable fixture manifest differs')
    by_name = {case['file']: case for case in cases}
    for item in report['retained_files']:
        value = encoded(files, base / safe_path(item['path']))
        require(len(value) == item['bytes'] and sha(value) == item['sha256'], 'retained portable evidence differs')
    for case in cases:
        for preparation in report['preparations'][case['file']].values():
            for command in preparation.get('commands', []):
                check_command(files, base, command)
    for row in report['rows']:
        check_command(files, base, row)
        if row['status'] != 'sampled':
            continue
        case = by_name[row['case']]
        image = report['preparations'][row['case']][row['actor']].get('image')
        values = checked_observations(decoded(files, base / row['stdout']), row['actor'], case,
            sha(encoded(files, base / 'sources' / case['file'])), report['consumer_identity'], image, report['node_identity'])
        require(all(row.get(key) == value for key, value in values.items())
                and not encoded(files, base / row['stderr']), 'portable observation differs from raw output')
    rows = []
    for item in report['summary']:
        runtimes = {}
        for actor in ACTORS:
            actor_row = item['actors'][actor]
            if actor_row['status'] == 'sampled':
                runtimes[actor] = dict(status='ok', ms=actor_row['entry_wall_ns'] / 1e6,
                    rss_mib=actor_row['process_peak_rss_bytes'] / 1024**2)
            else:
                status = actor_row['stage']
                commands = report['preparations'][item['file']][actor].get('commands', [])
                if any(command['resources']['timed_out'] for command in commands):
                    status = 'timed out'
                elif status == 'source-to-wasm':
                    status = 'build'
                runtimes[actor] = dict(status=status, stage=actor_row['stage'], reason=actor_row['reason'])
        rows.append(dict(file=item['file'], operations=by_name[item['file']]['operations'], runtimes=runtimes))
    return report, rows


def react_rows(files, receipt, fingerprint):
    report, base = checked_report(files, receipt)
    require(report['schema'] == 'scriptw-react-benchmark-v1' and report['mode'] == 'run'
            and report['ordinary_identity']['source_fingerprint'] == fingerprint
            and report['samples'] == 7 and report['warmups'] == 3, 'React report cohort differs')
    for command in report['commands']:
        check_command(files, base, command)
    commands = [command for command in report['commands'] if command['stage'] in (*SERVER, 'browser')]
    require(len(commands) == len(report['references']) == 40, 'React reference command inventory differs')
    support = decoded(files, Path(next(command for command in commands if command['stage'] in SERVER)['argv'][3]) / 'support.json')
    require(support['versions'] == report['versions'], 'React upstream versions differ')
    expected_order = []
    for block in range(2):
        names = list(SERVER)
        if block:
            names.reverse()
        for process in range(4):
            expected_order.extend((block, process, name) for name in names)
            expected_order.append((block, process, 'browser'))
    observed_order = []
    for entry, command in zip(report['references'], commands, strict=True):
        raw = entry['raw']
        require(command['status'] == 'succeeded' and not encoded(files, base / command['stderr'])
                and raw == decoded(files, base / command['stdout']), 'React reference differs from raw command output')
        require(raw['samples'] == 7 and raw['warmups'] == 3, 'React sample counts differ')
        name = 'browser'
        if entry['kind'] == 'server':
            name = raw['case']
            require(raw['schema'] == 'scriptw-react-server-v1' and raw['node'] == report['node_identity']
                    and len(raw['observations']) == 10, 'server identity or inventory differs')
            require(type(raw['parse_ns']) is int and raw['parse_ns'] >= 0
                    and type(raw['initialization_ns']) is int and raw['initialization_ns'] >= 0,
                    'server source/setup spans missing')
            groups = [(name, raw['observations'])]
        else:
            require(entry['kind'] == 'browser' and raw['schema'] == 'scriptw-react-browser-v1'
                    and raw['cross_origin_isolated'] is True and raw['errors'] == []
                    and raw['process_index'] == entry['block'] * 4 + entry['process']
                    and raw['block'] == entry['block'] and len(raw['observations']) == 30,
                    'browser identity, isolation or inventory differs')
            require(any(report['inputs_before'].get(value) == raw['chromium_sha256'] for value in command['argv']),
                    'Chromium identity differs')
            order = list(BROWSER)
            if entry['block']:
                order.reverse()
            offset = raw['process_index'] % 3
            order = order[offset:] + order[:offset]
            require(raw['order'] == order and [row['mode'] for row in raw['observations']]
                    == [mode for mode in order for _ in range(10)], 'browser mode order differs')
            groups = [(mode, [row for row in raw['observations'] if row['mode'] == mode]) for mode in BROWSER]
        observed_order.append((entry['block'], entry['process'], name))
        for mode, observations in groups:
            require(len(observations) == 10, 'per-case observations missing')
            for index, row in enumerate(observations):
                require(row['index'] == index and row['warmup'] == (index < 3)
                        and type(row['elapsed_ns']) is int and row['elapsed_ns'] > 0, 'React observation invalid')
                if entry['kind'] == 'server':
                    require(row['stdout'] == support['expected'][mode], 'server checked output differs')
                else:
                    count = 0
                    if mode == 'update':
                        count = 1
                    require(row['result'] == dict(react=report['versions']['react'], mode=mode, rows=64,
                                                 count=count, checked=True), 'browser checked result differs')
    require(observed_order == expected_order, 'React reference process order differs')
    summaries = []
    for name in (*SERVER, *BROWSER):
        groups = []
        for entry in report['references']:
            rows = entry['raw']['observations']
            if entry['kind'] == 'server' and entry['raw']['case'] != name:
                continue
            if entry['kind'] == 'browser':
                rows = [row for row in rows if row['mode'] == name]
            if rows:
                groups.append(dict(block=entry['block'], process=entry['process'],
                    entry_wall_ns=summary([row['elapsed_ns'] for row in rows if not row['warmup']])))
        require(len(groups) == 8, 'React case must have eight processes')
        summaries.append(dict(case=name, processes=groups,
            process_medians_ns=summary([round(group['entry_wall_ns']['median']) for group in groups])))
    require(summaries == report['summaries'], 'React summaries differ from raw samples')
    attempts = {(row['case'], row['actor']): row for row in report['capability']}
    require(set(attempts) == {(name, actor) for name in (*SERVER, 'client-mount') for actor in ('scriptw', 'scriptc')}
            and len(attempts) == len(report['capability']), 'full standalone capability matrix required')
    for (name, actor), attempt in attempts.items():
        require(attempt['status'] in ('checked', 'failed')
                and attempt['stage'] in ('source-to-wasm', 'wasm-to-native', 'execution'), 'unknown capability outcome')
        for field in ('build', 'native_preparation', 'execution'):
            if field in attempt:
                require(attempt[field] in report['commands'], 'capability command not retained in command inventory')
        if attempt['status'] == 'checked':
            execution = attempt['execution']
            require(attempt['build']['status'] == execution['status'] == 'succeeded'
                    and not encoded(files, base / execution['stderr']), 'failed capability labeled checked')
            output = encoded(files, base / execution['stdout']).decode()
            if actor == 'scriptw':
                checked_product_stdout(output, support['expected'][name])
            else:
                require(attempt['native_preparation']['status'] == 'succeeded', 'failed native preparation labeled checked')
                observations = json.loads(output)['observations']
                require(len(observations) == 10 and all(row['stdout'] == support['expected'][name]
                    and row['matched'] is True and row['completion'] == 'normal' for row in observations),
                    'standalone Scriptc capability output differs')
    return report, summaries


def reference_chart(report, rows, title, boundary, preview):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    with plt.rc_context({'font.family': 'DejaVu Sans', 'font.size': 11, 'svg.fonttype': 'none',
                         'svg.hashsalt': 'scriptw-application-reference-v1', 'text.usetex': False}):
        fig, ax = plt.subplots(figsize=(8, 3.7))
        try:
            values = [row['process_medians_ns']['median'] / 1e6 for row in rows]
            ax.barh(range(len(rows)), values, color='#de8b36', height=.6)
            ax.set_yticks(range(len(rows)), [LABELS[row['case']] for row in rows])
            ax.invert_yaxis()
            ax.set_xlim(0, max(values) * 1.3)
            ax.set_xlabel(boundary + ' · ms')
            ax.set_title(title, loc='left', fontweight='bold', pad=15)
            ax.set_axisbelow(True)
            ax.grid(axis='x', color='#eaeef2')
            for side in ('top', 'right', 'left'):
                ax.spines[side].set_visible(False)
            ax.tick_params(length=0, pad=8)
            for index, value in enumerate(values):
                ax.annotate(f'{value:.3f}', (value, index), xytext=(5, 0), textcoords='offset points', va='center')
            fig.subplots_adjust(left=.24, right=.96, top=.82, bottom=.18)
            output = io.BytesIO()
            fig.savefig(output, format='svg', facecolor='white', metadata={'Date': None, 'Title': title,
                'Creator': 'Scriptw application charts', 'Description': json.dumps(dict(
                    source_fingerprint=report['source_fingerprint'], source_commit=report['source_commit'],
                    boundary=boundary, rows=rows, renderer=report['renderer']), sort_keys=True)})
            if preview:
                preview.parent.mkdir(parents=True, exist_ok=True)
                fig.savefig(preview, dpi=100, facecolor='white')
            return b'\n'.join(line.rstrip() for line in output.getvalue().splitlines()) + b'\n'
        finally:
            plt.close(fig)


def readme_section(authority, rows, react, references):
    ratios = []
    for row in rows:
        actors = row['runtimes']
        if actors['scriptw']['status'] == actors['scriptc']['status'] == 'ok':
            ratios.append(actors['scriptw']['ms'] / actors['scriptc']['ms'])
    comparison = ''
    if ratios:
        comparison = (f'Across the **{len(ratios)} completed Scriptc comparisons**, Scriptw batch medians\n'
                      f'are **{min(ratios):.1f}–{max(ratios):.1f}× Scriptc**. Failed cases receive no speed score.')
    matrix = ['| Capability attempt | Scriptw | Scriptc | V8 host reference |', '| --- | --- | --- | --- |']
    attempts = {(row['case'], row['actor']): row for row in react['capability']}
    for name in (*SERVER, 'client-mount'):
        values = []
        for actor in ('scriptw', 'scriptc'):
            attempt = attempts[name, actor]
            text = 'Checked'
            if attempt['status'] != 'checked':
                text = {'source-to-wasm': 'Build failed', 'wasm-to-native': 'Native preparation failed',
                        'execution': 'Execution failed'}[attempt['stage']]
                commands = [attempt[field] for field in ('build', 'native_preparation', 'execution') if field in attempt]
                if any(command['resources']['timed_out'] for command in commands):
                    text = 'Timed out (60 s)'
            values.append(text)
        label, host = 'React client mount', 'Chromium checked'
        if name != 'client-mount':
            label, host = LABELS[name], 'Node checked'
        matrix.append('| ' + ' | '.join([label, *values, host]) + ' |')
    times = ['| Reference | Entry median · ms |', '| --- | ---: |']
    for row in references:
        times.append(f"| {LABELS[row['case']]} | {row['process_medians_ns']['median'] / 1e6:.3f} |")
    return f'''<!-- application-benchmarks:start -->
## Application workloads

Measured **{authority['finished_at'][:10]}**, compiler `{authority['source_fingerprint'][:12]}`.
Six synchronous request/response kernels use plain records, without networking or
Web APIs. The seventh case bundles real React {react['versions']['react']}
legacy `renderToString` with hooks/context and 32 escaped rows.
Times cover complete batches: 64 requests for JSON/routing/query/middleware,
32 HTML requests, 48 nested-result requests, or one 32-row SSR render.

Two balanced blocks, eight fresh processes per available actor/case; each process
has seven measured entries after three checked warmups. Cards show the median of
process entry medians and median whole-child peak RSS. Wasm compilation/loading
and V8 source/context setup stay outside entry timing; Scriptw Realm bootstrap
stays inside. Failed actors have no timing or memory bar.

{comparison}

{gallery(rows, 'applications-current', prefix='docs/rust-rewrite/assets')}

Real Server Components separately serialize and consume Flight streams in Node,
including an async component. Web references use Node's Request/Response/URL APIs.
The browser cases use real React DOM in Chromium: mount, state update and hydration,
with commit plus forced layout timed in the page. RSC renders 32 rows, Web API
cases process 32 requests, and client cases render 64 rows. Setup and golden-output comparisons are
outside these timers. These host references have different timing boundaries;
they establish no engine parity. Browser RSS covers all three cases in one process
and is not assigned to individual cases.

![Node Flight and Web API references](docs/rust-rewrite/assets/applications-current/react-server.svg)
![Chromium React DOM references](docs/rust-rewrite/assets/applications-current/react-browser.svg)

{chr(10).join(times)}

Actual standalone build/execution attempts remain visible, including failures:

{chr(10).join(matrix)}

Standalone run timeouts include compilation and startup; they do not isolate
rendering time. Scriptw's portable build and ordinary NativeHost/Fast run are
separate probes; the run recompiles source instead of loading the dumped pair.
The current Scriptw host also lacks DOM, encoding, Web Streams,
Fetch and URL APIs, as recorded by the separate capability probe.

[Data, identities and diagnostics](docs/rust-rewrite/applications-current.json) ·
[Inert raw evidence](docs/rust-rewrite/{authority['archive']['file']}) ·
[Workflow](CONTRIBUTING.md#react-and-edge-benchmarks).
<!-- application-benchmarks:end -->
'''


def main():
    global ARCHIVED_ROOT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--preview-dir', type=Path)
    args = parser.parse_args()
    authority = json.loads(AUTHORITY.read_text())
    require(authority['schema'] == 'scriptw-application-charts-v1', 'foreign application chart authority')
    ARCHIVED_ROOT = Path(authority['repository_root'])
    require(ARCHIVED_ROOT.is_absolute(), 'absolute recorded repository root required')
    files = read_archive(authority)
    portable, rows = portable_rows(files, authority['reports']['portable'], authority['source_fingerprint'])
    react, references = react_rows(files, authority['reports']['react'], authority['source_fingerprint'])
    import matplotlib
    report = dict(authority, consumer_identity=portable['consumer_identity'], node_identity=portable['node_identity'],
        policy=portable['policy'], chart_panels=[('ms', 'Entry time · ms', 3), ('rss_mib', 'Peak RSS · MiB', 1)],
        renderer=dict(matplotlib=matplotlib.__version__, script_sha256=digest(Path(__file__)),
                      shared_renderer_sha256=digest(ROOT / 'scripts/benchmark_charts.py')))
    outputs = {}
    for row in rows:
        preview = None
        if args.preview_dir:
            preview = args.preview_dir / (Path(row['file']).stem + '.png')
        outputs[ASSETS / (Path(row['file']).stem + '.svg')] = render(report, row, ACTORS, preview=preview)
    for names, stem, title, boundary in ((SERVER, 'react-server', 'Node · Flight and Web APIs', 'Complete async operation'),
                                       (BROWSER, 'react-browser', 'Chromium · React DOM', 'Commit + forced layout')):
        preview = None
        if args.preview_dir:
            preview = args.preview_dir / (stem + '.png')
        selected = [row for row in references if row['case'] in names]
        outputs[ASSETS / (stem + '.svg')] = reference_chart(report, selected, title, boundary, preview)
    readme = ROOT / 'README.md'
    text = readme.read_text()
    start_marker, end_marker = '<!-- application-benchmarks:start -->', '<!-- application-benchmarks:end -->'
    section = readme_section(authority, rows, react, references)
    if start_marker in text:
        require(text.count(start_marker) == text.count(end_marker) == 1, 'README application markers differ')
        start, end = text.index(start_marker), text.index(end_marker) + len(end_marker)
        text = text[:start] + section.rstrip('\n') + text[end:]
    else:
        require(end_marker not in text, 'orphan README application marker')
        location = text.index('## Compilation time\n')
        text = text[:location] + section + '\n' + text[location:]
    outputs[readme] = text.encode()
    for path, value in outputs.items():
        if args.check:
            require(path.read_bytes() == value, 'generated output differs: ' + str(path.relative_to(ROOT)))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(value)
    print(f'Verified {len(rows)} application cards, two host reference charts and README')


if __name__ == '__main__':
    main()
