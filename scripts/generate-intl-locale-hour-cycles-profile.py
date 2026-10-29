#!/usr/bin/env python3
"""Generate complete Locale hour-cycle data from immutable existing CLDR47."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

RELEASE = '47.0.0'
COMMIT = '2ef784e3a4168bc2a43cd1b5b9839b6636f5899c'
ALGORITHM = 'locale-hour-cycles-cldr47-v1'
PRIMARY = Path('crates/scriptw-intl/data/datetime-cldr-47')
DEST = Path('crates/scriptw-intl/data/locale-hour-cycles-cldr-47')
NATIVE = Path('crates/scriptw-intl/src/provider/locale_hour_cycles')
IDENTITY = NATIVE / 'profile_identity.rs'
REGION_HELPER = Path('crates/scriptw-intl/src/provider/region_preference.rs')
PRODUCTION = [NATIVE.with_suffix('.rs'), NATIVE / 'record.rs', NATIVE / 'profile.rs']
HELPERS = [Path(p) for p in ['crates/scriptw-intl/src/provider/language_domain.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs',
    'crates/scriptw-intl/src/identifiers.rs', 'crates/scriptw-intl/src/datetime.rs',
    'Cargo.lock', 'Cargo.toml', 'crates/scriptw-intl/Cargo.toml']]

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
    'crates/scriptw-intl/src/provider/locale_week.rs',
    'crates/scriptw-intl/src/provider/locale_week/record.rs',
    'crates/scriptw-intl/src/provider/locale_week/profile.rs',
    'crates/scriptw-intl/data/locale-week-cldr-47/profile.json',
    'crates/scriptw-intl/src/provider/locale_text.rs',
    'crates/scriptw-intl/src/provider/locale_text/record.rs',
    'crates/scriptw-intl/src/provider/locale_text/profile.rs',
    'crates/scriptw-intl/data/locale-text-cldr-47/profile.json',
    'crates/scriptw-intl/src/provider/locale_text/script.rs',
    'scripts/generate-intl-locale-hour-cycles-profile.py',
])
SYMBOLS = {'H': 'h23', 'h': 'h12', 'K': 'h11', 'k': 'h24', 'hb': 'h12', 'hB': 'h12'}
REGION = re.compile(r'(?:[A-Z]{2}|[0-9]{3})')
SELECTOR = re.compile(r'(?:(?:[a-z]{2,3}|[a-z]{5,8})_)?(?:[A-Z]{2}|[0-9]{3})')
sha = lambda value: hashlib.sha256(value).hexdigest()
encode = lambda value: (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def primary_inputs(root):
    base = root / PRIMARY
    manifest_bytes = (base / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_bytes)
    if manifest['release'] != RELEASE or manifest['commit'] != COMMIT:
        raise ValueError('unreviewed CLDR primary revision')
    pinned = {row['path']: row for row in manifest['files']}
    inputs = []
    for name in ['common/supplemental/supplementalData.xml', 'docs/ldml/tr35-dates.md', 'LICENSE']:
        data = (base / name).read_bytes()
        row = pinned[name]
        if row['sha256'] != sha(data) or row['bytes'] != len(data):
            raise ValueError('primary input differs from pinned manifest: ' + name)
        inputs.append({'path': (PRIMARY / name).as_posix(), 'sha256': sha(data), 'bytes': len(data)})
    return base, sha(manifest_bytes), inputs


def project(allowed):
    result = []
    for symbol in allowed.split():
        if symbol not in SYMBOLS:
            raise ValueError('unknown timeData hour symbol')
        cycle = SYMBOLS[symbol]
        if cycle not in result:
            result.append(cycle)
    if not result:
        raise ValueError('empty allowed cycle list')
    return result


def extract(root):
    time_data = root.find('timeData')
    if time_data is None:
        raise ValueError('missing timeData')
    selectors, raw_rows = {}, []
    for item in time_data:
        if item.tag != 'hours' or set(item.attrib) != {'preferred', 'allowed', 'regions'}:
            raise ValueError('unreviewed timeData row schema')
        if item.attrib['preferred'] not in {'H', 'h', 'K', 'k'}:
            raise ValueError('unknown compatibility preferred symbol')
        cycles = project(item.attrib['allowed'])
        names = item.attrib['regions'].split()
        if not names:
            raise ValueError('empty timeData selector set')
        raw_rows.append({'preferred': item.attrib['preferred'], 'allowed': item.attrib['allowed'],
            'selectors': names, 'projected_cycles': cycles})
        for name in names:
            if not SELECTOR.fullmatch(name):
                raise ValueError('invalid selector spelling')
            key = name.replace('_', '-')
            if key in selectors:
                raise ValueError('duplicate timeData selector')
            selectors[key] = cycles
    if '001' not in selectors:
        raise ValueError('missing world timeData default')
    territory = {item.attrib['type'] for item in root.findall('territoryInfo/territory')}
    containment = set()
    for item in root.findall('territoryContainment/group'):
        if item.get('status') == 'deprecated':
            continue
        containment.add(item.attrib['type'])
        containment.update(item.attrib['contains'].split())
    regions = territory | containment | {key for key in selectors if '-' not in key}
    regions.discard('ZZ')
    if not territory or not containment or any(not REGION.fullmatch(key) for key in regions):
        raise ValueError('incomplete/invalid pinned region availability domain')
    if any(key.split('-')[-1] not in regions for key in selectors):
        raise ValueError('selector region is unavailable')
    effective = [{'region': region, 'cycles': selectors.get(region, selectors['001'])}
                 for region in sorted(regions)]
    inheritance = [{'region': region, 'source_selector': region if region in selectors else '001'}
                   for region in sorted(regions)]
    return selectors, effective, raw_rows, inheritance


def kernel_sources(primary_root, native_root):
    actual = {path.relative_to(native_root) for path in (native_root / NATIVE).rglob('*.rs')
              if path.name not in {'tests.rs', 'profile_identity.rs'} and 'tests' not in path.parts}
    actual.add(NATIVE.with_suffix('.rs'))
    if actual != set(PRODUCTION):
        raise ValueError('unknown/missing Locale hour-cycle production reachability')
    parent = (native_root / NATIVE.with_suffix('.rs')).read_text()
    modules = set(re.findall(r'^(?:pub(?:\([^)]*\))?\s+)?mod (\w+);$', parent, re.M)) - {'tests', 'profile_identity'}
    if modules != {'record', 'profile'}:
        raise ValueError('Locale hour-cycle production module closure changed')
    region_source = (native_root / REGION_HELPER).read_text()
    if re.search(r'^(?:pub(?:\([^)]*\))?\s+)?mod \w+;', region_source, re.M):
        raise ValueError('shared region helper production closure changed')
    result = []
    for path in sorted(PRODUCTION + HELPERS + [REGION_HELPER]):
        owner = native_root if path in PRODUCTION or path == REGION_HELPER else primary_root
        data = (owner / path).read_bytes()
        result.append({'path': path.as_posix(), 'sha256': sha(data), 'bytes': len(data)})
    return result


def outputs(primary_root, native_root=None):
    native_root = native_root or Path(__file__).resolve().parents[1]
    base, manifest_sha, inputs = primary_inputs(primary_root)
    selectors, regions, raw_rows, inheritance = extract(ET.parse(base / 'common/supplemental/supplementalData.xml').getroot())
    counts = (len(raw_rows), len(selectors), sum('-' not in key for key in selectors),
              sum('-' in key for key in selectors), len(regions))
    if counts != (25, 275, 252, 23, 292):
        raise ValueError('pinned complete timeData coverage changed')
    profile = {'schema': 1, 'algorithm': ALGORITHM, 'cldr_release': RELEASE, 'cldr_commit': COMMIT,
        'primary_manifest_sha256': manifest_sha, 'primary_source_sha256': inputs[0]['sha256'],
        'regions': regions, 'selectors': [{'selector': key, 'cycles': selectors[key]} for key in sorted(selectors)]}
    profile_bytes = encode(profile)
    kernel_input = {'domain': 'scriptw-locale-hour-cycles-native-kernel-v1',
        'profile_sha256': sha(profile_bytes), 'sources': kernel_sources(primary_root, native_root)}
    kernel_digest = sha(encode(kernel_input))
    def constant(name, value):
        return 'pub const ' + name + ': [u8; 32] = [\n' + ''.join(
            '    ' + ', '.join('0x' + value[j:j+2] for j in range(i, i+32, 2)) + ',\n'
            for i in range(0, 64, 32)) + '];\n'
    identity = ('// Generated by scripts/generate-intl-locale-hour-cycles-profile.py.\n'
        '// DATA admits exact profile bytes; KERNEL binds reachable native production.\n'
        + constant('LOCALE_HOUR_CYCLES_DATA_SHA256', sha(profile_bytes))
        + constant('LOCALE_HOUR_CYCLES_KERNEL_SHA256', kernel_digest)).encode()
    provenance = {'schema': 1, 'algorithm': ALGORITHM, 'cldr_release': RELEASE, 'cldr_commit': COMMIT,
        'primary_manifest_path': (PRIMARY / 'manifest.json').as_posix(),
        'primary_manifest_sha256': manifest_sha, 'inputs': inputs, 'profile_sha256': sha(profile_bytes),
        'raw_timeData_rows': raw_rows, 'raw_timeData_node_count': 25, 'raw_selector_count': 275,
        'region_selector_count': 252, 'language_region_selector_count': 23, 'effective_region_rows': 292,
        'region_inheritance': inheritance, 'symbol_projection': SYMBOLS,
        'ordering': 'allowed preference order; stable uniqueness after projection; never prepend preferred',
        'region_availability': 'territoryInfo plus active territoryContainment plus explicit timeData regions; exclude ZZ sentinel',
        'fallback_authority': 'same pinned CLDR47 docs/ldml/tr35-dates.md Time_Data: region001 defaults',
        'unavailable_override': 'no effective row; continue with original base region',
        'kernel_input': kernel_input, 'kernel_sha256': kernel_digest,
        'kernel_excludes': ['tests.rs', 'profile_identity.rs'],
        'shared_region_helper': 'Root-authored canonical RegionPreference leaf; no duplicate selector',
        'normative_source': 'https://tc39.es/ecma402/#sec-hourcyclesoflocale',
        'existing_primary_data_changed': False, 'generation_kind': 'source-only; no product execution'}
    return {DEST / 'profile.json': profile_bytes, DEST / 'provenance.json': encode(provenance), IDENTITY: identity}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--primary-root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--output-root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for path, data in outputs(args.primary_root, args.output_root).items():
        actual = args.output_root / path
        if args.check:
            if actual.read_bytes() != data:
                raise ValueError('generated output differs: ' + str(path))
        else:
            if actual.exists() and actual.read_bytes() == data:
                continue
            actual.parent.mkdir(parents=True, exist_ok=True)
            actual.write_bytes(data)
    print('PASS exact complete CLDR47 timeData profile/native identity' if args.check else 'GENERATED exact complete CLDR47 timeData profile/native identity')


if __name__ == '__main__':
    main()
