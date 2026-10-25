#!/usr/bin/env python3
"""Archive checked startup comparisons and render their retained history."""
import argparse
from datetime import datetime, timezone
import gzip
import io
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import tarfile
import tempfile
import xml.etree.ElementTree as ET

from benchmark_metrics import SCHEMA, digest, load_report
from performance_charts import render_current

ROOT = Path(__file__).resolve().parents[1]
SCHEMA_HISTORY = 'scriptw-readme-performance-history-v1'
HISTORY = Path('docs/rust-rewrite/performance-history.json')
DOCS = Path('docs/rust-rewrite')
COHORT_FIELDS = ('schema', 'machine', 'configuration', 'policy', 'profile', 'node_versions',
                 'time_version', 'time_sha256', 'wasmtime_version', 'workloads', 'harness_sha256', 'samples')
CASES = (('integer', 'integer.js', '10,000 integer steps'),
         ('objects', 'objects.js', '500 objects'),
         ('retained', 'retained-objects.js', '8,192 retained objects'))
MAX_ARCHIVE_BYTES = 32 * 1024**2
MAX_MEMBER_BYTES = 16 * 1024**2
MAX_EXPANDED_BYTES = 64 * 1024**2


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(data):
    def closed(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, f'duplicate JSON key: {key}')
            result[key] = value
        return result

    def constant(value):
        raise ValueError(f'nonfinite JSON number: {value}')
    return json.loads(data, object_pairs_hook=closed, parse_constant=constant)


def sha(data):
    import hashlib
    return hashlib.sha256(data).hexdigest()


def hex_string(value, length):
    return isinstance(value, str) and re.fullmatch('[0-9a-f]{' + str(length) + '}', value) is not None


def instant(value):
    require(isinstance(value, str), 'measurement timestamp must be text')
    parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
    require(parsed.utcoffset() is not None, 'measurement timestamp needs a timezone')
    return parsed.astimezone(timezone.utc)


def positive(value, role):
    require(type(value) in (int, float) and math.isfinite(value) and value > 0,
            f'{role} must be finite and positive')
    return value


def validate_readme_report(report):
    require(report.get('schema') == SCHEMA and report.get('complete') is True and 'failure' not in report,
            'README history needs a complete ordinary v2 report')
    config = report['configuration']
    require(config.get('startup') is True and config.get('native_policy') is None,
            'README history requires ordinary startup and execution measurements')
    expected = {'startup.js': 'startup', **{name: 'execution' for _, name, _ in CASES}}
    workloads = report['workloads']
    require(len(workloads) == 4 and {w['file']: w['role'] for w in workloads} == expected,
            'README history needs exactly startup.js, integer.js, objects.js and retained-objects.js')
    require(all(w.get('execution_mode', 'fresh-realm') == 'fresh-realm' for w in workloads),
            'README history requires the fresh Store/context timing boundary')
    require(instant(report['started_at']) <= instant(report['finished_at']),
            'measurement finished before it started')
    require(hex_string(report['source_commit'], 40), 'measurement needs a full Git source commit')
    for key in ('cli_sha256', 'runtime_runner_sha256'):
        require(hex_string(report[key], 64), f'invalid {key}')
    for key in COHORT_FIELDS:
        require(key in report, f'missing comparison field: {key}')
    require(isinstance(report['machine'].get('cpu_model'), str), 'missing measured CPU model')
    require(all(isinstance(report['node_versions'].get(key), str) for key in ('node', 'v8')),
            'missing Node/V8 versions')
    require(isinstance(report['wasmtime_version'], str), 'missing Wasmtime version')


def expected_files(report):
    files = {'workload/manifest.json'}
    files.update('workload/' + w['file'] for w in report['workloads'])
    for command in report['commands']:
        files.update('logs/' + command['label'] + '.' + stream for stream in ('stdout', 'stderr'))
    files.update('wasm/' + name + '.wasm' for _, name, _ in CASES)
    declared = [item['path'] for item in report['retained_files']]
    require(len(declared) == len(set(declared)) and set(declared) == files,
            'retained file inventory differs from the complete README measurement plan')
    return files


def bind_identities(root, report):
    identity = read_json((root / 'logs/compiler-identity.stdout').read_bytes())
    require(identity == report['compiler_identity'], 'compiler identity log differs from report')
    require(identity['source_revision'] == {'kind': 'git-commit', 'commit': report['source_commit']},
            'measured source differs from compiler identity')
    require(identity['executable_sha256'] == report['cli_sha256'],
            'measured CLI differs from compiler identity')
    require(hex_string(identity['source_fingerprint'], 64), 'invalid compiler source fingerprint')
    for _, name, _ in CASES:
        raw = read_json((root / f'logs/execution-wasmtime-{name}.stdout').read_bytes())
        require(raw['source_commit'] == report['source_commit']
                and raw['source_fingerprint'] == identity['source_fingerprint']
                and raw['executable_sha256'] == report['runtime_runner_sha256'],
                'Wasmtime output source/runner identity differs from report')
        require(raw.get('requested_native_policy') is None,
                'ordinary README history cannot carry a native-policy experiment')


def admit(path):
    path = Path(path).resolve()
    require(path.stat().st_size <= MAX_MEMBER_BYTES, 'report exceeds publication budget')
    data = path.read_bytes()
    report = read_json(data)
    validate_readme_report(report)
    expected_files(report)
    size = len(data)
    for item in report['retained_files']:
        member = (path.parent / item['path']).resolve()
        require(member.is_relative_to(path.parent), 'retained file outside report directory')
        length = member.stat().st_size
        size += length
        require(length <= MAX_MEMBER_BYTES and size <= MAX_EXPANDED_BYTES,
                'retained files exceed publication budget')
    require(load_report(path) == report, 'report changed during admission')
    bind_identities(path.parent, report)
    point(report, sha(data))  # Reject unusable chart timings before recording anything.
    return report, data


def point(report, report_sha):
    metrics = []
    for mode, label in (('cold', 'Cold start'), ('hot', 'Cached start')):
        row = report['summaries']['startup'][mode]
        metrics.append(dict(id=mode, label=label, group='Startup',
            scriptw_ns=positive(row['scriptw']['elapsed_ns']['median'], 'Scriptw startup time'),
            v8_ns=positive(row['v8']['elapsed_ns']['median'], 'V8 startup time'),
            scriptw_rss_bytes=row['scriptw']['peak_rss_bytes']['median'],
            v8_rss_bytes=row['v8']['peak_rss_bytes']['median']))
    commands = {row['label']: row for row in report['commands']}
    for key, name, label in CASES:
        row = report['summaries']['execution']
        metrics.append(dict(id=key, label=label, group='Execution',
            scriptw_ns=positive(row['wasmtime'][name]['execution_ns']['median'], 'Scriptw execution time'),
            v8_ns=positive(row['v8'][name]['execution_ns']['median'], 'V8 execution time'),
            scriptw_rss_bytes=commands['execution-wasmtime-' + name]['peak_rss_bytes'],
            v8_rss_bytes=commands['execution-v8-' + name]['peak_rss_bytes']))
    compatibility = {key: report[key] for key in COHORT_FIELDS}
    return dict(id=report_sha, source_commit=report['source_commit'],
        measured_at=instant(report['finished_at']).isoformat(),
        started_at=instant(report['started_at']).isoformat(), samples=report['samples'],
        warmups=report['configuration']['warmups'],
        cohort_id=sha(json.dumps(compatibility, sort_keys=True, separators=(',', ':')).encode()),
        machine=report['machine'], node_version=report['node_versions']['node'],
        v8_version=report['node_versions']['v8'], wasmtime_version=report['wasmtime_version'], metrics=metrics)


def unpack(data, destination):
    require(len(data) <= MAX_ARCHIVE_BYTES, 'performance evidence archive is too large')
    names = set()
    expanded = 0
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        for member in archive:
            name = PurePosixPath(member.name)
            require(member.isfile() and not name.is_absolute() and '..' not in name.parts
                    and '\\' not in member.name and not re.match(r'^[A-Za-z]:', member.name)
                    and str(name) == member.name and member.name not in names,
                    'unsafe or duplicate performance evidence member')
            names.add(member.name)
            expanded += member.size
            require(0 <= member.size <= MAX_MEMBER_BYTES and expanded <= MAX_EXPANDED_BYTES
                    and len(names) <= 4096, 'performance evidence exceeds extraction budget')
            target = destination.joinpath(*name.parts)
            require(target.resolve().is_relative_to(destination.resolve()),
                    'performance evidence member escapes its destination')
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.extractfile(member) as source, target.open('xb') as output:
                remaining = member.size
                while remaining:
                    chunk = source.read(min(1024 * 1024, remaining))
                    require(chunk, 'truncated performance evidence member')
                    output.write(chunk)
                    remaining -= len(chunk)
    require('report.json' in names, 'performance evidence has no report')
    return names


def pack(path, report, data):
    members = {'report.json': data}
    for item in report['retained_files']:
        member = path.parent / item['path']
        require(member.stat().st_size <= MAX_MEMBER_BYTES, 'retained file exceeds publication budget')
        value = member.read_bytes()
        require(sha(value) == item['sha256'], 'retained file changed during publication')
        members[item['path']] = value
    require(sum(map(len, members.values())) <= MAX_EXPANDED_BYTES
            and all(len(data) <= MAX_MEMBER_BYTES for data in members.values()),
            'performance evidence exceeds publication budget')
    output = io.BytesIO()
    with gzip.GzipFile(fileobj=output, mode='wb', filename='', mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as archive:
            for name, value in sorted(members.items()):
                member = tarfile.TarInfo(name)
                member.size = len(value)
                member.mode = 0o644
                archive.addfile(member, io.BytesIO(value))
    result = output.getvalue()
    require(len(result) <= MAX_ARCHIVE_BYTES, 'performance evidence archive is too large')
    return result


def load_entry(root, entry, data=None):
    require(isinstance(entry, dict) and set(entry) == {'archive', 'archive_sha256', 'report_sha256'},
            'invalid performance history entry')
    identity = entry['report_sha256']
    require(hex_string(identity, 64) and hex_string(entry['archive_sha256'], 64), 'invalid evidence identity')
    require(entry['archive'] == f'performance-history/{identity}.tar.gz', 'invalid evidence archive path')
    if data is None:
        archive = root / DOCS / entry['archive']
        require(archive.stat().st_size <= MAX_ARCHIVE_BYTES, 'performance evidence archive is too large')
        data = archive.read_bytes()
    require(sha(data) == entry['archive_sha256'], 'performance archive checksum differs')
    with tempfile.TemporaryDirectory(prefix='scriptw-performance-') as directory:
        destination = Path(directory)
        names = unpack(data, destination)
        report, raw = admit(destination / 'report.json')
        require(sha(raw) == identity, 'source report checksum differs')
        require(names == expected_files(report) | {'report.json'}, 'unexpected performance evidence members')
        return point(report, identity)


def load_history(root):
    path = root / HISTORY
    manifest = read_json(path.read_bytes())
    require(isinstance(manifest, dict) and set(manifest) == {'schema', 'entries'}
            and manifest['schema'] == SCHEMA_HISTORY and isinstance(manifest['entries'], list)
            and manifest['entries'], 'invalid or empty performance history')
    ids = [row['report_sha256'] for row in manifest['entries']]
    require(len(ids) == len(set(ids)), 'duplicate measured report in history')
    points = [load_entry(root, entry) for entry in manifest['entries']]
    points.sort(key=lambda p: (instant(p['measured_at']), p['id']))
    return manifest, points


def outputs(root, points):
    current = points[-1]
    files = {
        DOCS / 'assets/performance-memory.svg': render_current(current).encode(),
    }
    for path, data in files.items():
        if path.suffix == '.svg':
            try:
                ET.fromstring(data)
            except ET.ParseError as error:
                raise ValueError(f'invalid generated SVG: {path}: {error}') from error
    return files


def write_files(root, files):
    for relative, data in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_name(path.name + f'.tmp-{os.getpid()}')
        try:
            temporary.write_bytes(data)
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def record(root, path):
    path = Path(path).resolve()
    report, raw = admit(path)
    identity = sha(raw)
    manifest, points = load_history(root) if (root / HISTORY).exists() else (
        dict(schema=SCHEMA_HISTORY, entries=[]), [])
    if identity in {p['id'] for p in points}:
        write_files(root, outputs(root, points))
        return dict(recorded=False, reports=len(points), latest_source=points[-1]['source_commit'])
    archive = pack(path, report, raw)
    entry = dict(archive=f'performance-history/{identity}.tar.gz', archive_sha256=sha(archive), report_sha256=identity)
    admitted = load_entry(root, entry, archive)
    existing = root / DOCS / entry['archive']
    require(not existing.exists() or (existing.is_file()
            and existing.stat().st_size == len(archive) and existing.read_bytes() == archive),
            'refusing to overwrite existing performance evidence')
    points.append(admitted)
    points.sort(key=lambda p: (instant(p['measured_at']), p['id']))
    entries = {row['report_sha256']: row for row in [*manifest['entries'], entry]}
    manifest['entries'] = [entries[p['id']] for p in points]
    files = outputs(root, points)  # Validate all output plans before writing history.
    files = {DOCS / entry['archive']: archive,
             HISTORY: (json.dumps(manifest, indent=2) + '\n').encode(), **files}
    write_files(root, files)
    return dict(recorded=True, reports=len(points), latest_source=points[-1]['source_commit'])


def render(root, check=False):
    _, points = load_history(root)
    files = outputs(root, points)
    if check:
        stale = [str(path) for path, data in files.items()
                 if not (root / path).is_file() or (root / path).read_bytes() != data]
        require(not stale, 'stale performance output: ' + ', '.join(stale))
    else:
        write_files(root, files)
    return dict(reports=len(points), latest_source=points[-1]['source_commit'], checked=check)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    add = sub.add_parser('record', help='admit a full retained report, append history and refresh its SVG')
    add.add_argument('report', type=Path)
    add.add_argument('--root', type=Path, default=ROOT)
    draw = sub.add_parser('render', help='regenerate from committed evidence without running benchmarks')
    draw.add_argument('--check', action='store_true')
    draw.add_argument('--root', type=Path, default=ROOT)
    args = parser.parse_args()
    try:
        result = record(args.root.resolve(), args.report) if args.command == 'record' else render(args.root.resolve(), args.check)
    except (ValueError, KeyError, TypeError, OSError, tarfile.TarError) as error:
        parser.exit(1, f'performance publication: {error}\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
