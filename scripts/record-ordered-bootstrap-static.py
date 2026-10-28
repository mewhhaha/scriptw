#!/usr/bin/env python3
"""Probe-only inert inspection of the actual owned Cargo R outputs."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import struct
import subprocess

spec = importlib.util.spec_from_file_location('ordered_bootstrap_ci_tools', Path(__file__).with_name('verify-ordered-bootstrap-ci.py'))
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)
ROOT, MIB = v.ROOT, v.MIB
HELPERS = ('helper::realm_initialize_intrinsics', 'helper::intrinsic_constructor_allocate',
           'helper::deferred_intrinsic_install', 'helper::intrinsic_accessor_allocate')


def owned(path, maximum):
    path = Path(path)
    before = path.lstat()
    v.require(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid() and before.st_nlink == 1
              and before.st_size <= maximum and path.resolve() == path, 'input must be an owned bounded nonsymlinked single-link regular file: ' + str(path))
    return before


def read(path, maximum):
    before = owned(path, maximum)
    with path.open('rb') as stream:
        raw = stream.read(maximum + 1)
    after = path.stat()
    v.require(len(raw) == before.st_size and len(raw) <= maximum
              and (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
              == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns), 'input changed during bounded read')
    return raw


def source_fingerprint():
    """Mirror the current Rust filesystem input contract, including ignored inputs.

    The scheme, roots and exact exclusions are parsed from the checked source;
    directory component ordering and target/.git pruning match the Rust owner.
    """
    implementation = ROOT / 'crates/scriptw-engine/compiler_fingerprint.rs'
    raw = read(implementation, MIB)
    text = raw.decode()
    scheme, = re.findall(r'pub\(crate\) const FINGERPRINT_SCHEME: &str = "([^"]+)";', text)
    v.require(scheme == 'scriptw-program-cache-compiler-v3', 'unsupported compiler fingerprint scheme')
    def rows(name):
        block, = re.findall(r'(?:pub\(crate\) )?const ' + name + r': &\[&str\] = &\[(.*?)\];', text, re.S)
        result = re.findall(r'^\s*"([^"]+)",', block, re.M)
        v.require(result and len(result) == len(set(result)), 'fingerprint input contract is empty/repeated')
        return result
    roots, excluded = rows('COMPILER_INPUTS'), rows('TEST_ONLY_INPUTS')
    files = set()
    def walk(path):
        relative = path.relative_to(ROOT).as_posix()
        if any(relative == item or relative.startswith(item + '/') for item in excluded):
            return
        info = path.lstat()
        if stat.S_ISREG(info.st_mode):
            v.require(info.st_size <= 64 * MIB, 'source fingerprint member exceeds bound')
            files.add(path)
            v.require(len(files) <= 32768, 'source fingerprint inventory exceeds bound')
        else:
            v.require(stat.S_ISDIR(info.st_mode), 'source fingerprint contains a nonregular member')
            for child in path.iterdir():
                child_info = child.lstat()
                if stat.S_ISDIR(child_info.st_mode) and child.name in ('target', '.git'):
                    continue
                walk(child)
    for root in roots:
        v.require(not Path(root).is_absolute() and '..' not in Path(root).parts, 'unsafe source fingerprint root')
        walk(ROOT / root)
    digest = hashlib.sha256(scheme.encode())
    total = 0
    for path in sorted(files, key=lambda item: item.relative_to(ROOT).parts):
        before = path.stat()
        label = path.relative_to(ROOT).as_posix().encode()
        total += before.st_size
        v.require(total <= 256 * MIB, 'source fingerprint total exceeds bound')
        digest.update(len(label).to_bytes(8, 'little'))
        digest.update(label)
        digest.update(before.st_size.to_bytes(8, 'little'))
        actual = 0
        with path.open('rb') as stream:
            while chunk := stream.read(MIB):
                actual += len(chunk)
                v.require(actual <= before.st_size, 'source changed while fingerprinting')
                digest.update(chunk)
        after = path.stat()
        v.require(actual == before.st_size and (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
                  == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns), 'source changed while fingerprinting')
    return dict(scheme=scheme, source_fingerprint=digest.hexdigest(), files=len(files), bytes=total,
                roots=roots, excluded=excluded, implementation_sha256=hashlib.sha256(raw).hexdigest(),
                scope='Current filesystem compiler inputs, including ignored inputs; not merely the Git-tracked file inventory.')


def inspect(profile, prefix, directory, fingerprint, output, report):
    package_path = directory / (prefix + '-package.bin')
    manifest_path = directory / (prefix + '-manifest.bin')
    native_path = directory / (prefix + '-native.bin')
    raw = read(package_path, 64 * MIB)
    v.require(len(raw) >= 176 and raw[:8] == b'SCRWRB01' and struct.unpack_from('<I', raw, 8)[0] == 29,
              'runtime package magic/layout differs')
    v.require(raw[12:44].hex() == fingerprint and hashlib.sha256(raw[:-32]).digest() == raw[-32:],
              'runtime package compiler fingerprint/checksum differs')
    static_bytes, utf16_bytes, strings, wasm_bytes = struct.unpack_from('<QQIQ', raw, 108)
    wasm = memoryview(raw)[136:-32]
    v.require(wasm_bytes == len(wasm) and wasm[:8] == b'\0asm\x01\0\0\0', 'runtime package Wasm extent/magic differs')
    key_hash = hashlib.sha256(struct.pack('<I', 29))
    key_hash.update(wasm)
    runtime_key = key_hash.hexdigest()
    v.require(raw[76:108].hex() == runtime_key, 'runtime package internal key differs from exact raw R Wasm')
    package_digest = hashlib.sha256(raw).hexdigest()
    wasm_digest = hashlib.sha256(wasm).hexdigest()
    package = dict(path=str(package_path), bytes=len(raw), sha256=package_digest, layout=29,
                   source_fingerprint=fingerprint, runtime_identity_sha256=raw[44:76].hex(), runtime_key=runtime_key,
                   static_bytes=static_bytes, pooled_utf16_bytes=utf16_bytes, strings=strings,
                   wasm_bytes=wasm_bytes, wasm_sha256=wasm_digest)
    if args.retain_raw_wasm:
        wasm_path = output / (profile + '-runtime.wasm')
        with wasm_path.open('xb') as stream:
            stream.write(wasm)
        package['retained_raw_wasm'] = dict(path=wasm_path.name, bytes=wasm_bytes, sha256=v.sha(wasm_path))
    del wasm, raw
    manifest = read(manifest_path, 4096)
    v.require(len(manifest) >= 149 and manifest[:8] == b'SCRWNR01' and struct.unpack_from('<I', manifest, 8)[0] == 5,
              'runtime native manifest magic/version differs')
    length = struct.unpack_from('<H', manifest, 19)[0]
    v.require(0 < length <= 256 and len(manifest) == 21 + length + 128, 'native manifest extent differs')
    target = manifest[21:21+length].decode('ascii')
    v.require(re.fullmatch('[A-Za-z0-9_.-]+', target) is not None, 'native manifest target differs')
    configuration = struct.unpack_from('<I', manifest, 12)[0]
    configuration_source = (ROOT / 'crates/scriptw-engine/src/wasmtime_config.rs').read_text()
    expected_configuration, = re.findall(r'pub\(crate\) const CONFIGURATION_SCHEMA: u32 = (\d+);', configuration_source)
    v.require(configuration == int(expected_configuration) and list(manifest[16:19]) == [1, 3, 1],
              'native manifest configuration/compilation/execution/register-allocation modes differ')
    offset = 21 + length
    source, key, pkg, native_digest = [manifest[offset+i*32:offset+(i+1)*32].hex() for i in range(4)]
    v.require((source, key, pkg) == (fingerprint, runtime_key, package_digest), 'manifest source/runtime/package digests differ')
    native_info = owned(native_path, 128 * MIB)
    with native_path.open('rb') as stream:
        v.require(stream.read(4) == b'\x7fELF', 'owned native image is not an ELF')
    v.require(v.sha(native_path) == native_digest, 'owned native ELF manifest digest differs')
    retained_manifest = output / (profile + '-manifest.bin')
    with retained_manifest.open('xb') as stream:
        stream.write(manifest)
    result = dict(package=package,
                  manifest=dict(path=str(manifest_path), retained_path=retained_manifest.name, bytes=len(manifest),
                                sha256=v.sha(retained_manifest), version=5, source_fingerprint=source,
                                runtime_key=key, package_digest=pkg, native_digest=native_digest,
                                target=target, configuration=configuration, modes=list(manifest[16:19])),
                  native=dict(path=str(native_path), bytes=native_info.st_size, sha256=native_digest))
    report['variants'][profile] = result
    row = dict(label=profile + '-r-symbols', command=['nm', '--print-size', '--size-sort', '--defined-only', str(native_path)],
               stdout=profile + '-r-symbols.stdout', stderr=profile + '-r-symbols.stderr')
    report['commands'].append(row)
    v.save(output, report)
    try:
        v.run_command(row, output, timeout=60, stdout_limit=32 * MIB, stderr_limit=MIB)
    finally:
        v.save(output, report)
    v.require(row.get('exit_code') == 0 and not row['timed_out'] and not row['output_limit_exceeded']
              and not row.get('pipe_cleanup_incomplete'), 'inert native symbol inspection failed; original streams retained')
    helpers = {}
    lines = (output / row['stdout']).read_bytes().splitlines()
    for helper in HELPERS:
        candidates = [line.split() for line in lines if line.endswith(('::' + helper).encode())]
        v.require(len(candidates) == 1 and len(candidates[0]) == 4, 'native helper symbol is missing/repeated: ' + helper)
        address, size, kind, name = candidates[0]
        v.require(re.fullmatch(b'[0-9a-fA-F]{1,16}', address) is not None
                  and re.fullmatch(b'[0-9a-fA-F]{1,16}', size) is not None and kind in (b't', b'T')
                  and 0 < int(size, 16) <= native_info.st_size, 'native helper symbol size/type differs')
        helpers[helper] = dict(native_bytes=int(size, 16), address=int(address, 16), symbol=name.decode('utf-8'))
    result['helper_symbols'] = helpers
    result['new_shared_helpers_native_bytes'] = sum(helpers[name]['native_bytes'] for name in HELPERS[1:3])
    result['initializer_plus_new_helpers_native_bytes'] = helpers[HELPERS[0]]['native_bytes'] + result['new_shared_helpers_native_bytes']
    # These reference facts are explicitly historical, not a fabricated local rerun.
    result['initializer_delta_from_reported_reference_bytes'] = helpers[HELPERS[0]]['native_bytes'] - 765194
    result['initializer_plus_new_helpers_delta_from_reported_reference_bytes'] = result['initializer_plus_new_helpers_native_bytes'] - 765194
    v.require(v.sha(package_path) == package_digest and v.sha(manifest_path) == v.sha(retained_manifest)
              and v.sha(native_path) == native_digest, 'owned runtime inputs changed during static inspection')
    after_native = native_path.stat()
    v.require((native_info.st_dev, native_info.st_ino, native_info.st_size, native_info.st_mtime_ns)
              == (after_native.st_dev, after_native.st_ino, after_native.st_size, after_native.st_mtime_ns), 'owned native file changed')
    v.save(output, report)


def main():
    global args
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--expected-source', required=True)
    parser.add_argument('--cargo-log', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--retain-raw-wasm', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = dict(schema='scriptw-ordered-bootstrap-static-ci-v1', source_commit=args.expected_source,
                  complete=False, scored=False, started_at=v.stamp(), commands=[], variants={},
                  limits=dict(package_bytes=64*MIB, native_bytes=128*MIB, nm_stdout_bytes=32*MIB, nm_stderr_bytes=MIB,
                              nm_timeout_seconds=60, cargo_log_bytes=64*MIB),
                  reported_reference=dict(source_commit='9001324dc52ce583e7b95a1292f317796d415c01', layout=28,
                                          realm_initializer_native_bytes=765194, accessor_allocate_native_bytes=783,
                                          remeasured_by_this_invocation=False),
                  interpretation='Static source-bound owned Cargo R package/native ELF metadata only. Historical reference sizes are not revalidated here. No native payload is executed, deserialized or retained. Native static size and overlapping function pages do not establish executed functions, process residency or causal RSS/timing savings.')
    v.save(output, report)
    try:
        v.scope()
        report.update(cpu_affinity=[0], before=v.budget())
        v.source(args.expected_source)
        report['compiler_inputs'] = source_fingerprint()
        log_path = args.cargo_log.resolve(strict=True)
        raw = read(log_path, 64 * MIB)
        cargo = [json.loads(line) for line in raw.splitlines()]
        v.require(cargo and cargo[-1] == dict(reason='build-finished', success=True)
                  and sum(row.get('reason') == 'build-finished' for row in cargo) == 1, 'Cargo build did not finish successfully exactly once')
        package_id = 'path+' + (ROOT / 'crates/scriptw-runtime-image').as_uri() + '#0.1.0'
        producers = [row for row in cargo if row.get('reason') == 'build-script-executed' and row.get('package_id') == package_id]
        v.require(len(producers) == 1, 'actual unique runtime-image Cargo build producer required')
        hosts = [row for row in cargo if row.get('reason') == 'compiler-artifact' and row.get('executable')
                 and row['target']['name'] == 'compiler_precompiled_baseline' and row['target']['kind'] == ['example']
                 and row['features'] == ['benchmark-module-images'] and row['profile']['opt_level'] == '3' and row['profile']['test'] is False]
        v.require(len(hosts) == 1, 'actual feature-enabled release shared-host producer required')
        directory = Path(producers[0]['out_dir'])
        target = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target'))).resolve()
        v.require(directory.is_absolute() and directory.resolve() == directory and directory.is_relative_to(target)
                  and directory.name == 'out' and directory.parent.name.startswith('scriptw-runtime-image-')
                  and directory.parent.parent.name == 'build', 'runtime-image out_dir is outside its actual Cargo target tree')
        report.update(cargo_log=dict(path=str(log_path), bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest()),
                      runtime_image_cargo_producer=producers[0], shared_host_cargo_producer=hosts[0], runtime_out_dir=str(directory))
        v.save(output, report)
        for profile, prefix in [('Portable', 'scriptw-runtime-portable'), ('NativeHost', 'scriptw-runtime')]:
            v.source(args.expected_source)
            inspect(profile, prefix, directory, report['compiler_inputs']['source_fingerprint'], output, report)
        first, second = (report['variants'][name]['manifest'] for name in ('Portable', 'NativeHost'))
        v.require(all(first[key] == second[key] for key in ('source_fingerprint', 'target', 'configuration', 'modes')),
                  'the two actual runtime variants differ in source/target/compilation policy')
        v.source(args.expected_source)
        v.require(v.sha(log_path) == report['cargo_log']['sha256'], 'Cargo producer log changed')
        report['after'] = v.budget()
        v.no_oom(report['before'], report['after'])
        report.update(complete=True, finished_at=v.stamp())
        return 0
    except Exception as error:
        report.update(error=str(error), error_type=type(error).__name__, finished_at=v.stamp())
        return 1
    finally:
        if 'before' in report and 'after' not in report:
            try:
                report['after'] = v.budget()
            except (OSError, ValueError) as error:
                report['resource_after_error'] = str(error)
        v.save(output, report)
        print(json.dumps(dict(complete=report['complete'], error=report.get('error'), output=str(output))), flush=True)


if __name__ == '__main__':
    raise SystemExit(main())
