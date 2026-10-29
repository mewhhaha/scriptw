#!/usr/bin/env python3
"""Generate complete Locale calendar data from immutable existing CLDR47."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

RELEASE = '47.0.0'
COMMIT = '2ef784e3a4168bc2a43cd1b5b9839b6636f5899c'
ALGORITHM = 'locale-calendars-cldr47-v1'
PRIMARY = Path('crates/scriptw-intl/data/datetime-cldr-47')
DEST = Path('crates/scriptw-intl/data/locale-calendars-cldr-47')
NATIVE = Path('crates/scriptw-intl/src/provider/locale_calendars')
IDENTITY = NATIVE / 'profile_identity.rs'
REGION_HELPER = Path('crates/scriptw-intl/src/provider/region_preference.rs')
PRODUCTION = [NATIVE.with_suffix('.rs'), NATIVE / 'record.rs', NATIVE / 'profile.rs']
HELPERS = [Path(p) for p in ['crates/scriptw-intl/src/lib.rs', 'crates/scriptw-intl/src/artifact_identity.rs', 'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs',
    'crates/scriptw-intl/src/protocol.rs', 'crates/scriptw-intl/src/locale_information_wire.rs',
    'crates/scriptw-intl/src/provider/language_domain.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs',
    'crates/scriptw-intl/src/identifiers.rs', 'crates/scriptw-intl/src/datetime.rs',
    'crates/scriptw-intl/src/datetime_protocol.rs', 'crates/scriptw-intl/src/provider/datetime.rs',
    'crates/scriptw-intl/src/provider/datetime/locale.rs', 'crates/scriptw-intl/src/provider/datetime/profile.rs',
    'crates/scriptw-intl/src/provider/datetime/generated/profile.json',
    'Cargo.lock', 'Cargo.toml', 'crates/scriptw-intl/Cargo.toml']]

# Selected image admission and real keyword/data owners. Only upstream
# DateTime metadata enters; this service's generated kernel outputs are excluded.
HELPERS.extend(Path(path) for path in [
    'crates/scriptw-intl/build.rs',
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
    'crates/scriptw-intl/src/provider/locale_hour_cycles.rs',
    'crates/scriptw-intl/src/provider/locale_hour_cycles/record.rs',
    'crates/scriptw-intl/src/provider/locale_hour_cycles/profile.rs',
    'crates/scriptw-intl/data/locale-hour-cycles-cldr-47/profile.json',
    'crates/scriptw-intl/src/provider/locale_week.rs',
    'crates/scriptw-intl/src/provider/locale_week/record.rs',
    'crates/scriptw-intl/src/provider/locale_week/profile.rs',
    'crates/scriptw-intl/data/locale-week-cldr-47/profile.json',
    'crates/scriptw-intl/src/provider/locale_text.rs',
    'crates/scriptw-intl/src/provider/locale_text/record.rs',
    'crates/scriptw-intl/src/provider/locale_text/profile.rs',
    'crates/scriptw-intl/data/locale-text-cldr-47/profile.json',
    'crates/scriptw-intl/src/provider/locale_text/script.rs',
    'scripts/generate-intl-locale-calendars-profile.py',
])
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
    for name in ['common/supplemental/supplementalData.xml', 'common/bcp47/calendar.xml', 'docs/ldml/tr35-dates.md', 'LICENSE']:
        data = (base / name).read_bytes()
        row = pinned[name]
        if row['sha256'] != sha(data) or row['bytes'] != len(data):
            raise ValueError('primary input differs from pinned manifest: ' + name)
        inputs.append({'path': (PRIMARY / name).as_posix(), 'sha256': sha(data), 'bytes': len(data)})
    return base, sha(manifest_bytes), inputs


def extract(root, calendar_root):
    domain, aliases = set(), {}
    for row in calendar_root.findall("keyword/key[@name='ca']/type"):
        name = row.get('preferred', row.attrib['name'])
        if row.get('deprecated') != 'true':
            domain.add(name)
        aliases[row.attrib['name']] = name
        for alias in row.get('alias', '').split():
            aliases[alias] = name
    if len(domain) != 18:
        raise ValueError('unreviewed BCP47 calendar domain')
    selectors, raw_rows = {}, []
    rows = root.find('calendarPreferenceData')
    if rows is None:
        raise ValueError('missing calendarPreferenceData')
    for row in rows:
        if row.tag != 'calendarPreference' or set(row.attrib) != {'territories', 'ordering'}:
            raise ValueError('unreviewed calendar preference schema')
        projected = []
        for item in row.attrib['ordering'].split():
            name = aliases.get(item, item)
            if name not in domain:
                raise ValueError('unreviewed calendar preference identifier')
            if name not in projected:
                projected.append(name)
        if not projected:
            raise ValueError('empty calendar preference list')
        names = row.attrib['territories'].split()
        raw_rows.append({'ordering':row.attrib['ordering'], 'selectors':names, 'canonical_calendars':projected})
        for name in names:
            if not SELECTOR.fullmatch(name):
                raise ValueError('invalid selector')
            key = name.replace('_', '-')
            if key in selectors:
                raise ValueError('duplicate calendar selector')
            selectors[key] = projected
    if '001' not in selectors:
        raise ValueError('missing calendar world preference')
    territory = {row.attrib['type'] for row in root.findall('territoryInfo/territory')}
    containment = set()
    for row in root.findall('territoryContainment/group'):
        if row.get('status') == 'deprecated':
            continue
        containment.add(row.attrib['type'])
        containment.update(row.attrib['contains'].split())
    regions = territory | containment | {key for key in selectors if '-' not in key}
    regions.discard('ZZ')
    if not territory or not containment or any(not REGION.fullmatch(key) for key in regions):
        raise ValueError('invalid region availability')
    effective = [{'region':region, 'calendars':selectors.get(region, selectors['001'])} for region in sorted(regions)]
    inherited = [{'region':region, 'source_selector':region if region in selectors else '001'} for region in sorted(regions)]
    return selectors, effective, raw_rows, inherited, aliases, sorted(domain)


def kernel_sources(primary_root, native_root):
    actual = {path.relative_to(native_root) for path in (native_root / NATIVE).rglob('*.rs')
              if path.name not in {'tests.rs', 'profile_identity.rs'} and 'tests' not in path.parts}
    actual.add(NATIVE.with_suffix('.rs'))
    if actual != set(PRODUCTION):
        raise ValueError('unknown/missing Locale calendar production reachability')
    parent = (native_root / NATIVE.with_suffix('.rs')).read_text()
    modules = set(re.findall(r'^(?:pub(?:\([^)]*\))?\s+)?mod (\w+);$', parent, re.M)) - {'tests', 'profile_identity'}
    if modules != {'record', 'profile'}:
        raise ValueError('Locale calendar production module closure changed')
    region_source = (native_root / REGION_HELPER).read_text()
    if re.search(r'^(?:pub(?:\([^)]*\))?\s+)?mod \w+;', region_source, re.M):
        raise ValueError('shared region helper production closure changed')
    result = []
    for path in sorted(PRODUCTION + HELPERS + [REGION_HELPER]):
        owner = native_root if path in PRODUCTION or path == REGION_HELPER else primary_root
        data = (owner / path).read_bytes()
        result.append({'path': path.as_posix(), 'sha256': sha(data), 'bytes': len(data)})
    # Reuse the genuine complete DateTime authority for every reachable profile
    # helper. A hand-picked partial helper list cannot omit profile admission.
    import importlib.util
    producer_path = primary_root / 'scripts/generate-intl-datetime-identity.py'
    spec = importlib.util.spec_from_file_location('calendar_datetime_authority', producer_path)
    producer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(producer)
    rust, receipt = producer.generate(primary_root, primary_root, primary_root / 'Cargo.lock')
    identity_path = Path('crates/scriptw-intl/src/provider/datetime/identity.rs')
    receipt_path = Path('crates/scriptw-intl/data/datetime-cldr-47/kernel-identity.json')
    if (primary_root / identity_path).read_text() != rust or (primary_root / receipt_path).read_text() != receipt:
        raise ValueError('consumed DateTime source identity must be genuinely refreshed first')
    rows = {row['path']: row for row in result}
    for row in json.loads(receipt)['files']:
        rows[row['path']] = row
    for path in [identity_path, receipt_path]:
        data = (primary_root / path).read_bytes()
        rows[path.as_posix()] = {'path':path.as_posix(), 'sha256':sha(data), 'bytes':len(data)}
    return [rows[key] for key in sorted(rows)]


def outputs(primary_root, native_root=None):
    native_root = native_root or Path(__file__).resolve().parents[1]
    base, manifest_sha, inputs = primary_inputs(primary_root)
    selectors, regions, raw_rows, inheritance, aliases, calendar_domain = extract(ET.parse(base / 'common/supplemental/supplementalData.xml').getroot(), ET.parse(base / 'common/bcp47/calendar.xml').getroot())
    counts = (len(raw_rows), len(selectors), sum('-' not in key for key in selectors),
              sum('-' in key for key in selectors), len(regions))
    if counts != (15, 52, 52, 0, 292):
        raise ValueError('pinned complete calendarPreferenceData coverage changed')
    profile = {'schema': 1, 'algorithm': ALGORITHM, 'cldr_release': RELEASE, 'cldr_commit': COMMIT,
        'primary_manifest_sha256': manifest_sha, 'primary_source_sha256': inputs[0]['sha256'],
        'regions': regions, 'selectors': [{'selector': key, 'calendars': selectors[key]} for key in sorted(selectors)]}
    profile_bytes = encode(profile)
    kernel_input = {'domain': 'scriptw-locale-calendars-native-kernel-v1',
        'profile_sha256': sha(profile_bytes), 'sources': kernel_sources(primary_root, native_root)}
    kernel_digest = sha(encode(kernel_input))
    def constant(name, value):
        return 'pub const ' + name + ': [u8; 32] = [\n' + ''.join(
            '    ' + ', '.join('0x' + value[j:j+2] for j in range(i, i+32, 2)) + ',\n'
            for i in range(0, 64, 32)) + '];\n'
    identity = ('// Generated by scripts/generate-intl-locale-calendars-profile.py.\n'
        '// DATA admits exact profile bytes; KERNEL binds reachable native production.\n'
        + constant('LOCALE_CALENDARS_DATA_SHA256', sha(profile_bytes))
        + constant('LOCALE_CALENDARS_KERNEL_SHA256', kernel_digest)).encode()
    provenance = {'schema': 1, 'algorithm': ALGORITHM, 'cldr_release': RELEASE, 'cldr_commit': COMMIT,
        'primary_manifest_path': (PRIMARY / 'manifest.json').as_posix(),
        'primary_manifest_sha256': manifest_sha, 'inputs': inputs, 'profile_sha256': sha(profile_bytes),
        'raw_calendarPreferenceData_rows': raw_rows, 'raw_node_count': 15, 'raw_selector_count': 52,
        'region_selector_count':52, 'language_region_selector_count':0, 'effective_region_rows':292,
        'region_inheritance':inheritance, 'canonical_aliases':aliases, 'canonical_calendar_domain':calendar_domain,
        'ordering':'calendar preference order; canonical BCP47 aliases; stable uniqueness; runtime filter against actual checked DateTime AvailableCalendars',
        'region_availability':'territoryInfo plus active territoryContainment plus explicit preferences; exclude ZZ',
        'fallback_authority':'pinned CLDR47 tr35-dates Calendar_Preference_Data region001 defaults; ECMA402 CalendarsOfLocale empty gregory fallback',
        'unavailable_override':'continue original base region; recognized sparse regions inherit001',
        'kernel_input': kernel_input, 'kernel_sha256': kernel_digest,
        'kernel_excludes': ['tests.rs', 'profile_identity.rs'],
        'shared_region_helper': 'Root-authored canonical RegionPreference leaf; no duplicate selector',
        'normative_source': 'https://tc39.es/ecma402/#sec-calendarsoflocale',
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
    print('PASS exact complete CLDR47 timeData profile/native identity' if args.check else 'GENERATED exact complete CLDR47 calendarPreferenceData profile/native identity')


if __name__ == '__main__':
    main()
