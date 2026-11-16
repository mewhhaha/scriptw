#!/usr/bin/env python3
"""Freeze the Scriptw binaries an application benchmark campaign measures.

Builds (or confirms fresh) the product CLI and the shared precompiled host from
one clean committed tree, copies them and the host's Cargo runtime-image outputs
into a new directory, and writes the receipts that benchmark-applications.py
and benchmark-react.py require before every run:

  candidate/compiler_precompiled_baseline   shared precompiled host (frozen copy)
  candidate/identity.json                   its own `identity` report
  candidate/runtime-build/*.bin             the host's Cargo runtime-image outputs
  candidate-ordinary/scriptw                product CLI (frozen copy)
  candidate-ordinary/identity.json          its own `compiler-identity` report
  product-build.stdout, examples-build.stdout  the two Cargo JSON producer streams
  runtime-product-producer-candidate.json   runtime outputs bound to the producer stream
  publication-rust-witness.json             digests of every tracked compiler input
  native-qualified.json                     the freeze qualification (see `scope`)

Run it under limited_verification.py; it never measures anything.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from application_benchmarks import require
from benchmark_metrics import digest

ROOT = Path(__file__).resolve().parents[1]
SOURCE_PATHS = ('crates', 'Cargo.toml', 'Cargo.lock', '.cargo', 'rust-toolchain.toml')
PRODUCT = ('product-build', ['--bin', 'scriptw'])
HOST = ('examples-build', ['--example', 'compiler_precompiled_baseline'])
MANIFEST_MAGIC = b'SCRWNR01'


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True)


def source_witness():
    """Every tracked compiler input outside test directories, with its digest."""
    # Test directories build no product or host artifact; benchmark fixtures there are bound by the drivers.
    changed = [line for line in git('status', '--porcelain', '--untracked-files=all', '--', *SOURCE_PATHS).splitlines()
               if '/tests/' not in '/' + line[3:]]
    require(not changed, 'compiler sources must be the committed tree: commit or remove Rust/Cargo changes first')
    names = [name for name in git('ls-files', '-z', '--', *SOURCE_PATHS).split('\0')
             if name and '/tests/' not in '/' + name]
    return {name: digest(ROOT / name) for name in sorted(names)}


def cargo_rows(stream):
    rows = []
    for line in Path(stream).read_text().splitlines():
        if line.startswith('{'):
            rows.append(json.loads(line))
    return rows


def build(output, stem, selection, jobs):
    """One Cargo invocation; returns its compiler-artifact and build-script rows."""
    command = ['cargo', 'build', '-j', str(jobs), '--release', '--locked', '-p', 'scriptw-cli',
               *selection, '--message-format=json-render-diagnostics']
    with (output / (stem + '.stdout')).open('xb') as stdout, (output / (stem + '.stderr')).open('xb') as stderr:
        subprocess.run(command, cwd=ROOT, stdout=stdout, stderr=stderr, check=True)
    rows = cargo_rows(output / (stem + '.stdout'))
    finished = [row for row in rows if row.get('reason') == 'build-finished']
    require(len(finished) == 1 and finished[0].get('success') is True, stem + ': Cargo did not finish successfully')
    return rows


def executable(rows, kind, name):
    found = [row for row in rows if row.get('reason') == 'compiler-artifact'
             and row['target']['name'] == name and kind in row['target']['kind'] and row.get('executable')]
    require(len(found) == 1, f'one {kind} artifact required: {name}')
    return found[0]


def runtime_owner(rows):
    found = [row for row in rows if row.get('reason') == 'build-script-executed'
             and '/crates/scriptw-runtime-image#' in row['package_id']]
    require(len(found) == 1 and found[0].get('out_dir'), 'one scriptw-runtime-image build script required')
    return found[0]


def frozen_copy(source, destination):
    shutil.copyfile(source, destination)
    os.chmod(destination, 0o555)
    require(digest(source) == digest(destination), 'artifact changed while it was copied: ' + str(source))


def decode_manifest(data):
    """The Cargo producer's runtime manifest (embedded_runtime/manifest.rs, v5)."""
    require(data[:8] == MANIFEST_MAGIC and len(data) >= 23, 'runtime manifest magic differs')
    length = int.from_bytes(data[19:21], 'little')
    fields = data[21 + length:]
    require(len(fields) == 128, 'runtime manifest framing differs')
    return dict(version=int.from_bytes(data[8:12], 'little'), configuration=int.from_bytes(data[12:16], 'little'),
                target=data[21:21 + length].decode(), source_fingerprint=fields[:32].hex(),
                runtime_key=fields[32:64].hex(), package_sha256=fields[64:96].hex(), native_sha256=fields[96:].hex())


def runtime_receipt(output, owner, identity):
    out_dir = Path(owner['out_dir'])
    destination = output / 'candidate/runtime-build'
    destination.mkdir(parents=True)
    files = {}
    for source in sorted(out_dir.glob('*.bin')):
        frozen_copy(source, destination / source.name)
        files[source.name] = dict(source_path=str(source), sha256=digest(destination / source.name),
                                  bytes=source.stat().st_size)
    for prefix in ('scriptw-runtime', 'scriptw-runtime-portable'):
        names = {suffix: f'{prefix}-{suffix}.bin' for suffix in ('manifest', 'package', 'native')}
        require(all(name in files for name in names.values()), 'runtime output group is incomplete: ' + prefix)
        manifest = decode_manifest((destination / names['manifest']).read_bytes())
        require(manifest['source_fingerprint'] == identity['source_fingerprint'],
                'runtime image source differs from the frozen host: ' + prefix)
        require(manifest['target'] == identity['shared_configuration']['target_triple'], 'runtime target differs')
        require(manifest['package_sha256'] == files[names['package']]['sha256']
                and manifest['native_sha256'] == files[names['native']]['sha256'],
                'runtime manifest does not bind its package/native outputs: ' + prefix)
        files[names['package']]['package'] = dict(
            sha256=manifest['package_sha256'], bytes=files[names['package']]['bytes'],
            package_source_fingerprint=manifest['source_fingerprint'], runtime_key=manifest['runtime_key'],
            manifest=names['manifest'], manifest_version=manifest['version'],
            fingerprint_matches_host=True)
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--output', type=Path, required=True, help='new cohort directory')
    parser.add_argument('--cargo-jobs', type=int, default=1)
    args = parser.parse_args()
    require(args.cargo_jobs > 0, 'positive Cargo jobs required')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    commit = git('rev-parse', 'HEAD').strip()
    before = source_witness()
    product_rows = build(output, *PRODUCT, args.cargo_jobs)
    host_rows = build(output, *HOST, args.cargo_jobs)
    product = Path(executable(product_rows, 'bin', 'scriptw')['executable'])
    host = Path(executable(host_rows, 'example', 'compiler_precompiled_baseline')['executable'])
    for folder in ('candidate', 'candidate-ordinary'):
        (output / folder).mkdir()
    frozen_host, frozen_product = output / 'candidate/compiler_precompiled_baseline', output / 'candidate-ordinary/scriptw'
    frozen_copy(host, frozen_host)
    frozen_copy(product, frozen_product)
    identity = json.loads(subprocess.check_output([frozen_host, 'identity'], cwd=ROOT, timeout=60))
    require(identity['executable_sha256'] == digest(frozen_host), 'host identity does not describe its frozen bytes')
    require(identity['source_commit'] == commit, 'host source commit differs from HEAD')
    require(identity['comparison_cohort'] == 'scriptw-precompiled-comparison-v1'
            and identity['shared_configuration']['consumer_compiler_enabled'] is False, 'host configuration differs')
    save(output / 'candidate/identity.json', identity)
    ordinary = json.loads(subprocess.check_output([frozen_product, 'compiler-identity'], cwd=ROOT, timeout=60))
    require(ordinary['executable_sha256'] == digest(frozen_product)
            and ordinary['source_fingerprint'] == identity['source_fingerprint']
            and ordinary['source_revision'] == dict(kind='git-commit', commit=commit),
            'product CLI and host do not share one compiler source')
    save(output / 'candidate-ordinary/identity.json', ordinary)
    owner = runtime_owner(host_rows)
    files = runtime_receipt(output, owner, identity)
    # A second, no-op Cargo pass proves the copies are the exact artifacts of these sources:
    # a rebuild by another process in between would report fresh=false or different bytes.
    for stem, selection in (PRODUCT, HOST):
        rows = build(output, stem + '-confirm', selection, args.cargo_jobs)
        require(all(row['fresh'] for row in rows if row.get('reason') == 'compiler-artifact'),
                stem + ': Cargo rebuilt something after the freeze; rerun on a quiet tree')
    require(digest(host) == digest(frozen_host) and digest(product) == digest(frozen_product)
            and runtime_owner(cargo_rows(output / 'examples-build-confirm.stdout'))['out_dir'] == owner['out_dir']
            and all(digest(Path(owner['out_dir']) / name) == row['sha256'] for name, row in files.items()),
            'Cargo outputs changed after the freeze')
    after = source_witness()
    require(before == after, 'compiler sources changed during the freeze')
    save(output / 'runtime-product-producer-candidate.json', dict(
        status='verified', build_stdout='examples-build.stdout',
        build_stdout_sha256=digest(output / 'examples-build.stdout'), owner=owner, files=files))
    save(output / 'publication-rust-witness.json', dict(
        status='all tracked compiler inputs unchanged through the freeze', source_commit=commit,
        source_fingerprint=identity['source_fingerprint'], files=after, deleted=[],
        purpose='Every Scriptw artifact of the campaign is built from these exact bytes; drivers recheck them.'))
    save(output / 'native-qualified.json', dict(
        status='complete', source_fingerprint=identity['source_fingerprint'], source_commit=commit,
        scope=('freeze qualification only: one clean committed tree; the product CLI, the precompiled host and '
               'both Cargo runtime-image groups report the same compiler source fingerprint; Cargo confirmed the '
               'frozen copies fresh. No native test selection was run for this cohort.'),
        cargo_producers={stem: dict(path=stem + '.stdout', sha256=digest(output / (stem + '.stdout')))
                         for stem in ('product-build', 'examples-build', 'product-build-confirm', 'examples-build-confirm')},
        selected_tests=[]))
    print(json.dumps(dict(cohort=str(output), source_commit=commit, source_fingerprint=identity['source_fingerprint'],
                          host_sha256=identity['executable_sha256'], scriptw_sha256=ordinary['executable_sha256']),
                     indent=2))


if __name__ == '__main__':
    sys.exit(main())
