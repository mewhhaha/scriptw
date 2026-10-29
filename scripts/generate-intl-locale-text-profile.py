#!/usr/bin/env python3
"""Generate complete CLDR47 script directions; never executes a product."""
import argparse
import hashlib
import json
from pathlib import Path
import re

RELEASE = '47.0.0'
COMMIT = '2ef784e3a4168bc2a43cd1b5b9839b6636f5899c'
SOURCE_SHA = 'f765a79559429de5cf1c3346df9eebb73be5e46cef4489df7052b878e230bb0b'
LICENSE_SHA = 'b4c0ae8ef04f7059f96ce5bbe0467f9fe6f6d81bbe13517701dfeb961fb4d0b6'
DEST = Path('crates/scriptw-intl/data/locale-text-cldr-47')
PRIMARY = Path('crates/scriptw-intl/data/datetime-cldr-47')
SOURCE = DEST / 'source/scriptMetadata.txt'
IDENTITY = Path('crates/scriptw-intl/src/provider/locale_text/profile_identity.rs')
NATIVE = Path('crates/scriptw-intl/src/provider/locale_text')
PRODUCTION = [NATIVE.with_suffix('.rs'), NATIVE / 'record.rs', NATIVE / 'profile.rs', NATIVE / 'script.rs']
HELPERS = [Path(p) for p in ['crates/scriptw-intl/src/provider/language_domain.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs',
    'crates/scriptw-intl/src/identifiers.rs', 'Cargo.lock', 'Cargo.toml', 'crates/scriptw-intl/Cargo.toml']]

# Selected image admission and real keyword/data owners. Only upstream
# DateTime metadata enters; this service's generated kernel outputs are excluded.
HELPERS.extend(Path(path) for path in [
    'crates/scriptw-intl/build.rs',
    'crates/scriptw-intl/src/lib.rs',
    'crates/scriptw-intl/src/artifact_identity.rs',
    'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs',
    'crates/scriptw-intl/src/protocol.rs',
    'crates/scriptw-intl/src/image.rs',
    'crates/scriptw-intl/src/locale_image.rs',
    'crates/scriptw-intl/src/image_build/locale.rs',
    'crates/scriptw-intl/src/image_build/keyword.rs',
    'scripts/generate-intl-keyword-aliases.py',
    'crates/scriptw-engine/src/intl_data_images.rs',
    'crates/scriptw-engine/src/wasm_gc_intl_host.rs',
    'crates/scriptw-aot-wasm/src/emit.rs',
    'crates/scriptw-aot-wasm/src/emit/module_assembly.rs',
    'crates/scriptw-intl/src/native_locale_information_image.rs',
    'crates/scriptw-intl/src/datetime_image.rs',
    'crates/scriptw-intl/src/image_build/calendar.rs',
    'crates/scriptw-intl/src/provider/datetime/identity.rs',
    'crates/scriptw-intl/data/datetime-cldr-47/kernel-identity.json',
    'crates/scriptw-intl/src/provider/locale_calendars.rs',
    'crates/scriptw-intl/src/provider/locale_calendars/record.rs',
    'crates/scriptw-intl/src/provider/locale_calendars/profile.rs',
    'crates/scriptw-intl/data/locale-calendars-cldr-47/profile.json',
    'crates/scriptw-intl/src/provider/locale_hour_cycles.rs',
    'crates/scriptw-intl/src/provider/locale_hour_cycles/record.rs',
    'crates/scriptw-intl/src/provider/locale_hour_cycles/profile.rs',
    'crates/scriptw-intl/data/locale-hour-cycles-cldr-47/profile.json',
    'crates/scriptw-intl/src/provider/locale_week.rs',
    'crates/scriptw-intl/src/provider/locale_week/record.rs',
    'crates/scriptw-intl/src/provider/locale_week/profile.rs',
    'crates/scriptw-intl/data/locale-week-cldr-47/profile.json',
    'scripts/generate-intl-locale-text-profile.py',
])
sha = lambda b: hashlib.sha256(b).hexdigest()
encode = lambda value: (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def extract(data):
    rows, lines = {}, []
    for number, line in enumerate(data.decode('utf-8').splitlines(), 1):
        body = line.split('#', 1)[0].strip()
        if not body:
            continue
        fields = [x.strip() for x in body.split(';')]
        if len(fields) != 11:
            raise ValueError('unreviewed script metadata field schema')
        script, rtl = fields[0], fields[6]
        if re.fullmatch(r'[A-Z][a-z]{3}', script) is None:
            raise ValueError('invalid script spelling')
        if script in rows:
            raise ValueError('duplicate script')
        if rtl not in {'YES', 'NO', 'UNKNOWN'}:
            raise ValueError('unreviewed script direction value')
        rows[script] = {'YES': 'rtl', 'NO': 'ltr', 'UNKNOWN': None}[rtl]
        lines.append({'script': script, 'source_line': number, 'RTL_field': rtl})
    if not rows:
        raise ValueError('missing script metadata records')
    return [{'script': script, 'direction': rows[script]} for script in sorted(rows)], lines


def primary_inputs(primary_root, native_root):
    manifest_bytes = (primary_root / PRIMARY / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_bytes)
    if manifest['release'] != RELEASE or manifest['commit'] != COMMIT:
        raise ValueError('unreviewed CLDR primary revision')
    metadata = (native_root / SOURCE).read_bytes()
    license_bytes = (native_root / DEST / 'LICENSE').read_bytes()
    pinned_license = (primary_root / PRIMARY / 'LICENSE').read_bytes()
    license_row = next(row for row in manifest['files'] if row['path'] == 'LICENSE')
    if sha(metadata) != SOURCE_SHA:
        raise ValueError('script metadata differs from exact pinned primary source')
    if sha(license_bytes) != LICENSE_SHA or license_bytes != pinned_license:
        raise ValueError('CLDR license differs from same-commit primary authority')
    if license_row['sha256'] != LICENSE_SHA or license_row['bytes'] != len(license_bytes):
        raise ValueError('pinned license manifest is inconsistent')
    inputs = [{'path': SOURCE.as_posix(), 'sha256': SOURCE_SHA, 'bytes': len(metadata)},
              {'path': (DEST / 'LICENSE').as_posix(), 'sha256': LICENSE_SHA, 'bytes': len(license_bytes)}]
    return sha(manifest_bytes), metadata, inputs


def kernel_sources(primary_root, native_root):
    actual = {p.relative_to(native_root) for p in (native_root / NATIVE).rglob('*.rs')
              if p.name not in {'tests.rs', 'profile_identity.rs'} and 'tests' not in p.parts}
    actual.add(NATIVE.with_suffix('.rs'))
    if actual != set(PRODUCTION):
        raise ValueError('unreviewed/missing Locale text production source reachability')
    parent = (native_root / NATIVE.with_suffix('.rs')).read_text()
    modules = set(re.findall(r'^(?:pub(?:\([^)]*\))?\s+)?mod (\w+);$', parent, re.M)) - {'tests', 'profile_identity'}
    if modules != {'record', 'profile', 'script'}:
        raise ValueError('Locale text production module closure changed')
    result = []
    for path in sorted(PRODUCTION + HELPERS):
        owner = native_root if path in PRODUCTION else primary_root
        data = (owner / path).read_bytes()
        result.append({'path': path.as_posix(), 'sha256': sha(data), 'bytes': len(data)})
    return result


def outputs(primary_root, native_root=None):
    native_root = native_root or Path(__file__).resolve().parents[1]
    manifest_sha, metadata, inputs = primary_inputs(primary_root, native_root)
    rows, source_lines = extract(metadata)
    counts = {name: sum(row['direction'] == direction for row in rows)
              for name, direction in [('rtl', 'rtl'), ('ltr', 'ltr'), ('unknown', None)]}
    if len(rows) != 177 or counts != {'rtl': 36, 'ltr': 137, 'unknown': 4}:
        raise ValueError('pinned script domain coverage differs')
    profile = {'schema': 1, 'algorithm': 'locale-text-cldr47-v1', 'cldr_release': RELEASE,
        'cldr_commit': COMMIT, 'primary_manifest_sha256': manifest_sha,
        'primary_source_sha256': SOURCE_SHA, 'license_sha256': LICENSE_SHA, 'scripts': rows}
    profile_bytes = encode(profile)
    kernel_input = {'domain': 'scriptw-locale-text-native-kernel-v1',
        'profile_sha256': sha(profile_bytes), 'sources': kernel_sources(primary_root, native_root)}
    kernel_digest = sha(encode(kernel_input))
    def constant(name, value):
        return 'pub const ' + name + ': [u8; 32] = [\n' + ''.join(
            '    ' + ', '.join('0x' + value[j:j+2] for j in range(i, i+32, 2)) + ',\n'
            for i in range(0, 64, 32)) + '];\n'
    identity = ('// Generated by scripts/generate-intl-locale-text-profile.py.\n'
        '// DATA admits exact profile bytes; KERNEL binds reachable production.\n'
        + constant('LOCALE_TEXT_DATA_SHA256', sha(profile_bytes))
        + constant('LOCALE_TEXT_KERNEL_SHA256', kernel_digest)).encode()
    provenance = {'schema': 1, 'algorithm': 'locale-text-cldr47-v1', 'cldr_release': RELEASE,
        'cldr_commit': COMMIT, 'primary_manifest_path': (PRIMARY / 'manifest.json').as_posix(),
        'primary_manifest_sha256': manifest_sha, 'primary_url': 'https://raw.githubusercontent.com/unicode-org/cldr/' + COMMIT + '/common/properties/scriptMetadata.txt',
        'license': 'UNICODE LICENSE V3; exact same-commit pinned LICENSE copied', 'inputs': inputs,
        'script_metadata_schema': {'semicolon_fields': 11, 'script_column': 0, 'RTL_column': 6},
        'direction_mapping': {'YES': 'rtl', 'NO': 'ltr', 'UNKNOWN': None},
        'script_rows': len(rows), 'direction_counts': counts, 'primary_source_lines': source_lines,
        'unknown_scripts': [row['script'] for row in rows if row['direction'] is None],
        'profile_sha256': sha(profile_bytes), 'kernel_sha256': kernel_digest, 'kernel_input': kernel_input,
        'kernel_source_closure': 'explicit production module inventory; unknown/missing production source rejects',
        'kernel_excludes': ['tests.rs', 'profile_identity.rs'],
        'selection': 'explicit canonical base script; genuine likely-subtags only if absent; no regional preference keywords',
        'unknown_direction': 'None, never an ltr fallback',
        'normative_source': 'https://tc39.es/ecma402/#sec-textdirectionoflocale',
        'existing_primary_data_changed': False, 'generation_kind': 'source-only; no product execution'}
    return {DEST / 'profile.json': profile_bytes, DEST / 'provenance.json': encode(provenance), IDENTITY: identity}


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--primary-root', type=Path, default=Path(__file__).resolve().parents[1])
    p.add_argument('--output-root', type=Path, default=Path(__file__).resolve().parents[1])
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    for path, content in outputs(args.primary_root, args.output_root).items():
        target = args.output_root / path
        if args.check:
            if target.read_bytes() != content:
                raise ValueError('generated output differs: ' + str(path))
        else:
            if target.exists() and target.read_bytes() == content:
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
    print(json.dumps({'status': 'PASS SOURCE ONLY', 'outputs': 3, 'Cargo': 0, 'product_execution': 0}))


if __name__ == '__main__':
    main()
