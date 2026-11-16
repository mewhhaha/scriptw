#!/usr/bin/env python3
"""Reduce one complete application campaign to its inert evidence archive and authority.

Reads the portable report (benchmark-applications.py), the React report
(benchmark-react.py run), the prepared fixtures and the frozen cohort's text
receipts. Writes a bounded tar.xz of UTF-8 evidence only (no Wasm, native
images, screenshots or executables) and the authority JSON that
render-application-benchmarks.py verifies and draws. The archive is written
beside the authority and is never overwritten. It executes no measured program.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import statistics
import sys
import tarfile
import tempfile

import importlib.util

from application_benchmarks import ACTORS, SCHEMA, load_report, require
from benchmark_metrics import digest

ROOT = Path(__file__).resolve().parents[1]
FIXTURE_SOURCES = ROOT / 'crates/scriptw-cli/tests/fixtures/edge_rendering'
HARNESS = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_baseline/node-runtime.cjs'
TOOLING = ('benchmark-applications.py', 'application_benchmarks.py', 'benchmark-react.py', 'benchmark_metrics.py',
           'compilation_comparison.py', 'compiler_comparison.py', 'limited_verification.py', 'resource_numbers.py',
           'freeze-application-cohort.py', 'publish-application-benchmarks.py', 'render-application-benchmarks.py',
           'benchmark_charts.py')
COHORT_TEXT = ('candidate/identity.json', 'candidate-ordinary/identity.json', 'runtime-product-producer-candidate.json',
               'publication-rust-witness.json', 'native-qualified.json')
SKIP_DIRECTORIES = {'tmp', 'cache', 'work', 'node_modules', '__pycache__'}
BINARY_SUFFIXES = {'.wasm', '.swpc', '.png', '.bin', '.so', '.a', '.o', '.exe'}
MAX_FILE, MAX_EXPANDED, MAX_ARCHIVE, MAX_MEMBERS = 64 * 1024**2, 512 * 1024**2, 128 * 1024**2, 20_000


def renderer():
    spec = importlib.util.spec_from_file_location('render_applications', ROOT / 'scripts/render-application-benchmarks.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load(path):
    return json.loads(Path(path).read_text())


def inert_text(path):
    """Evidence must be UTF-8 text without NUL; anything else is refused, never silently dropped."""
    value = Path(path).read_bytes()
    require(len(value) <= MAX_FILE, 'evidence file exceeds the per-file bound: ' + str(path))
    require(not value.startswith((b'\0asm', b'\x7fELF', b'SWPC', b'SCRWNR01')) and b'\0' not in value,
            'binary bytes in text evidence: ' + str(path))
    value.decode('utf-8', errors='strict')
    return value


def member(path):
    relative = Path(path).resolve().relative_to(ROOT)
    name = 'evidence/' + relative.as_posix()
    require(not PurePosixPath(name).is_absolute() and '..' not in PurePosixPath(name).parts, 'unsafe member')
    return name


def walk(directory):
    """Every regular file of a campaign directory except scratch folders and known binary images."""
    for parent, children, names in os.walk(directory, followlinks=False):
        children[:] = sorted(name for name in children if name not in SKIP_DIRECTORIES)
        for name in sorted(names):
            path = Path(parent) / name
            if path.is_symlink() or not path.is_file() or path.suffix in BINARY_SUFFIXES:
                continue
            yield path


def collect(portable_dir, react_dir, fixtures, frozen, support):
    selected = {}

    def add(path):
        name = member(path)
        value = inert_text(path)
        require(selected.get(name, value) == value, 'evidence path changed: ' + str(path))
        selected[name] = value

    for directory in (portable_dir, react_dir, fixtures):
        for path in walk(directory):
            add(path)
    for name in COHORT_TEXT:
        add(frozen / name)
    for path in sorted(frozen.glob('*.stdout')) + sorted(frozen.glob('*.stderr')):
        add(path)
    for name in TOOLING:
        add(ROOT / 'scripts' / name)
    for path in walk(FIXTURE_SOURCES):
        add(path)
    add(HARNESS)
    # Upstream inputs the bundles were built from, bound by support.json digests.
    for name, sha in support['inputs'].items():
        path = Path(name)
        require(path.is_absolute() and path.resolve().is_relative_to(ROOT) and digest(path) == sha,
                'bundled upstream input changed or escapes the workspace: ' + name)
        if path.suffix in ('.js', '.cjs', '.mjs', '.json'):
            add(path)
    require(len(selected) + 2 <= MAX_MEMBERS and sum(map(len, selected.values())) <= MAX_EXPANDED,
            'evidence exceeds archive bounds')
    return selected


def write_archive(path, selected, index):
    index_bytes = (json.dumps(index, indent=2, sort_keys=True) + '\n').encode()
    members = dict(selected)
    members['INDEX.json'] = index_bytes
    members['INDEX.json.sha256'] = f'{hashlib.sha256(index_bytes).hexdigest()}  INDEX.json\n'.encode()
    descriptor, temporary = tempfile.mkstemp(prefix='.application-evidence-', suffix='.tar.xz', dir=path.parent)
    os.close(descriptor)
    temporary = Path(temporary)
    try:
        with tarfile.open(temporary, 'w:xz', preset=6) as archive:
            for name in sorted(members):
                info = tarfile.TarInfo(name)
                info.size, info.mode, info.mtime = len(members[name]), 0o644, 0
                archive.addfile(info, io.BytesIO(members[name]))
        require(temporary.stat().st_size <= MAX_ARCHIVE, 'compressed archive exceeds its bound')
        os.link(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
    return dict(file=path.name, sha256=digest(path), bytes=path.stat().st_size,
                regular_utf8_members=len(members), expanded_utf8_bytes=sum(map(len, members.values())))


def ready(portable_path, react_path, fixtures, frozen):
    """The cross-report identities the archive and authority claim, checked before anything is written."""
    portable = load_report(portable_path)
    react = load(react_path)
    identity = load(frozen / 'candidate/identity.json')
    ordinary = load(frozen / 'candidate-ordinary/identity.json')
    require(react.get('schema') == 'scriptw-react-benchmark-v1' and react.get('mode') == 'run'
            and react.get('status') == 'complete' and react['inputs_before'] == react['inputs_after'],
            'React report is incomplete or its inputs changed')
    fingerprint = identity['source_fingerprint']
    require(portable['source_fingerprint'] == portable['consumer_identity']['source_fingerprint'] == fingerprint
            and portable['consumer_identity'] == identity, 'portable campaign used another frozen host')
    require(react['ordinary_identity'] == ordinary and ordinary['source_fingerprint'] == fingerprint,
            'React campaign used another ordinary compiler')
    require(load(frozen / 'native-qualified.json')['source_fingerprint'] == fingerprint, 'cohort receipts differ')
    prepared = load(fixtures / 'report.json')
    require(prepared.get('mode') == 'prepare' and prepared.get('status') == 'complete'
            and prepared['inputs_before'] == prepared['inputs_after'], 'fixture preparation is incomplete')
    require((portable_path.parent / 'sources/manifest.json').read_bytes() == (fixtures / 'manifest.json').read_bytes(),
            'portable campaign measured other fixtures')
    servers = [command for command in react['commands'] if command['stage'] in ('rsc-sync', 'rsc-async', 'web-url', 'web-json')]
    require(servers and all(Path(command['argv'][3]) == fixtures for command in servers),
            'React campaign measured other fixtures')
    return portable, react, identity


def authority_for(portable, react, receipts, archive, identity):
    browsers = [entry['raw'] for entry in react['references'] if entry['kind'] == 'browser']
    versions = {raw['browser_version'] for raw in browsers}
    require(len(versions) == 1, 'one Chromium version required')
    machines = [portable['machine'], react['machine']]
    require(machines[0]['cpu_model'] == machines[1]['cpu_model'], 'reports come from different machines')
    resources = portable['machine']['resources']
    scope = 'one pinned CPU under a 4 GiB cgroup'
    if 'inherited_memory_cap_bytes' in resources:
        scope = f"one pinned CPU, inherited {resources['inherited_memory_cap_bytes'] / 1024**3:.1f} GiB cloud memory cap"
    summary = f"{machines[0]['cpu_model']} ({machines[0]['logical_cpus']} vCPUs), {scope}"
    rows = portable['rows']
    attempts = []
    for attempt in react['capability']:
        commands = [attempt[field] for field in ('build', 'native_preparation', 'execution') if field in attempt]
        attempts.append(dict(case=attempt['case'], actor=attempt['actor'], status=attempt['status'],
                             stage=attempt['stage'], timed_out=any(c['resources']['timed_out'] for c in commands)))
    failures = []
    for item in portable['summary']:
        for actor in ACTORS:
            row = item['actors'][actor]
            if row['status'] != 'sampled':
                failures.append(dict(scope='portable', case=item['file'], actor=actor, stage=row['stage'],
                                     owner={'scriptw': 'Scriptw compiler/runtime', 'scriptc': 'Scriptc 0.4.5 (external)',
                                            'v8': 'benchmark harness'}[actor]))
    for attempt in attempts:
        if attempt['status'] != 'checked':
            failures.append(dict(scope='standalone', case=attempt['case'], actor=attempt['actor'], stage=attempt['stage'],
                                 owner={'scriptw': 'Scriptw compiler/runtime host APIs',
                                        'scriptc': 'Scriptc 0.4.5 (external)'}[attempt['actor']]))
    finished = max(portable['finished_at'], react['finished_at'])
    return dict(
        schema='scriptw-application-charts-v1', status='complete_descriptive_benchmarks_with_explicit_failures',
        repository_root=str(ROOT), source_fingerprint=identity['source_fingerprint'],
        source_commit=identity['source_commit'], measurement_commit=portable['measurement_commit'],
        started_at=min(portable['started_at'], react['started_at']), finished_at=finished,
        archive=archive, reports=receipts,
        machine=dict(summary=summary, portable=portable['machine'], react=react['machine']),
        versions=dict(react=react['versions'], node=portable['node_identity']['versions'], chromium=versions.pop()),
        portable=portable['summary'],
        react_reference_medians_ms={row['case']: row['process_medians_ns']['median'] / 1e6 for row in react['summaries']},
        standalone=attempts, failures=failures,
        coverage=dict(portable_attempted_processes=len(rows),
                      portable_successful_processes=sum(row['status'] == 'sampled' for row in rows),
                      portable_checked_entries=sum(len(row.get('entry_wall_ns', [])) + portable['warmups']
                                                   for row in rows if row['status'] == 'sampled'),
                      react_server_processes=sum(entry['kind'] == 'server' for entry in react['references']),
                      browser_processes=len(browsers), standalone_attempts=len(attempts)),
        workloads=portable['workloads'],
        limits=('Fresh-entry portable corpus and retained-Realm Web/DOM references have different timing boundaries. '
                'No pooled ratio across them, significance claim, full React support, deployed edge platform '
                'throughput or conformance acceptance. Timed-out child RSS/CPU samples are unavailable, not zero.'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--portable', type=Path, required=True, help='benchmark-applications.py output directory')
    parser.add_argument('--react', type=Path, required=True, help='benchmark-react.py run output directory')
    parser.add_argument('--fixtures', type=Path, required=True, help='benchmark-react.py prepare output directory')
    parser.add_argument('--frozen', type=Path, required=True, help='freeze-application-cohort.py output directory')
    parser.add_argument('--authority', type=Path, default=ROOT / 'docs/rust-rewrite/applications-current.json')
    args = parser.parse_args()
    portable_dir, react_dir, fixtures, frozen = (path.resolve(strict=True) for path in
                                                 (args.portable, args.react, args.fixtures, args.frozen))
    authority_path = args.authority.resolve()
    portable, react, identity = ready(portable_dir / 'report.json', react_dir / 'report.json', fixtures, frozen)
    support = load(fixtures / 'support.json')
    selected = collect(portable_dir, react_dir, fixtures, frozen, support)
    receipts = {name: dict(path=str((directory / 'report.json').relative_to(ROOT)),
                           sha256=digest(directory / 'report.json'))
                for name, directory in (('portable', portable_dir), ('react', react_dir))}
    date = max(portable['finished_at'], react['finished_at'])[:10].replace('-', '')
    archive_path = authority_path.parent / f'application-benchmarks-{date}-inert.tar.xz'
    require(not archive_path.exists(), 'refusing to overwrite ' + str(archive_path))
    index = dict(schema='scriptw-inert-evidence-index-v1', cohort=f'application-benchmarks-{date}',
                 members=[dict(path=name, bytes=len(value), sha256=hashlib.sha256(value).hexdigest())
                          for name, value in sorted(selected.items())],
                 excluded=['Wasm, native images, executables and screenshots', 'tmp, cache and work directories',
                           'node_modules except the upstream inputs bound by support.json'],
                 limits=dict(file_bytes=MAX_FILE, expanded_bytes=MAX_EXPANDED, archive_bytes=MAX_ARCHIVE,
                             members=MAX_MEMBERS))
    archive = write_archive(archive_path, selected, index)
    authority = authority_for(portable, react, receipts, archive, identity)
    try:
        # The renderer's own verification must accept the archive before the authority names it.
        renderer().verified_campaign(authority, authority_path.parent)
    except BaseException:
        archive_path.unlink()
        raise
    authority_path.parent.mkdir(parents=True, exist_ok=True)
    authority_path.write_text(json.dumps(authority, indent=2, sort_keys=True, allow_nan=False) + '\n')
    print(json.dumps(dict(authority=str(authority_path), archive=archive), indent=2))


if __name__ == '__main__':
    sys.exit(main())
