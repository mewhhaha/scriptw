#!/usr/bin/env python3
"""Generate Locale week data from the existing immutable CLDR47 authority."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

RELEASE = '47.0.0'
COMMIT = '2ef784e3a4168bc2a43cd1b5b9839b6636f5899c'
SCHEMA = 1
ALGORITHM = 'locale-week-cldr47-v1'
DAYS = dict(zip(['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'], range(1, 8)))
PRIMARY = Path('crates/scriptw-intl/data/datetime-cldr-47')
DEST = Path('crates/scriptw-intl/data/locale-week-cldr-47')
IDENTITY = Path('crates/scriptw-intl/src/provider/locale_week/profile_identity.rs')
NATIVE = Path('crates/scriptw-intl/src/provider/locale_week')
NATIVE_PRODUCTION = [NATIVE.with_suffix('.rs'), NATIVE / 'record.rs',
                     NATIVE / 'profile.rs']
LOCALE_HELPERS = [Path(p) for p in [
    'crates/scriptw-intl/src/provider/language_domain.rs',
    'crates/scriptw-intl/src/provider/region_preference.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs',
    'crates/scriptw-intl/src/identifiers.rs', 'Cargo.lock', 'Cargo.toml',
    'crates/scriptw-intl/Cargo.toml',
]]

# Selected image admission and real keyword/data owners. Only upstream
# DateTime metadata enters; this service's generated kernel outputs are excluded.
LOCALE_HELPERS.extend(Path(path) for path in [
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
    'crates/scriptw-intl/src/provider/locale_text.rs',
    'crates/scriptw-intl/src/provider/locale_text/record.rs',
    'crates/scriptw-intl/src/provider/locale_text/profile.rs',
    'crates/scriptw-intl/data/locale-text-cldr-47/profile.json',
    'crates/scriptw-intl/src/provider/locale_text/script.rs',
    'scripts/generate-intl-locale-week-profile.py',
])
sha = lambda b: hashlib.sha256(b).hexdigest()
encode = lambda x: (json.dumps(x, indent=2, sort_keys=True) + '\n').encode()


def weekend_mask(start, end):
    if start not in range(1, 8) or end not in range(1, 8):
        raise ValueError('weekday outside ISO domain')
    day, mask = start, 0
    while True:
        mask |= 1 << (day - 1)
        if day == end:
            return mask
        day = day % 7 + 1


def primary_inputs(root):
    base = root / PRIMARY
    manifest_bytes = (base / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_bytes)
    if manifest['release'] != RELEASE or manifest['commit'] != COMMIT:
        raise ValueError('unreviewed CLDR primary revision')
    records = {x['path']: x for x in manifest['files']}
    names = ['common/supplemental/supplementalData.xml', 'docs/ldml/tr35.md',
             'docs/ldml/tr35-dates.md']
    inputs = []
    for name in names:
        data = (base / name).read_bytes()
        row = records[name]
        if row['sha256'] != sha(data) or row['bytes'] != len(data):
            raise ValueError('primary source differs from existing pinned manifest: ' + name)
        inputs.append({'path': (PRIMARY / name).as_posix(), 'sha256': sha(data), 'bytes': len(data)})
    return base, sha(manifest_bytes), inputs


def extract(root):
    week = root.find('weekData')
    if week is None:
        raise ValueError('missing weekData')
    maps, alternatives = {}, []
    for field in ['firstDay', 'weekendStart', 'weekendEnd']:
        rows = {}
        for item in week.findall(field):
            if item.get('alt') is not None:
                alternatives.append({'field': field, 'attributes': dict(item.attrib)})
                continue
            if item.get('day') not in DAYS:
                raise ValueError('unknown weekday')
            # The current authority has whole-day boundaries. A future partial
            # day boundary requires explicit admission instead of truncation.
            if item.get('time') not in (None, '00:00'):
                raise ValueError('partial-day weekend requires explicit policy')
            value = DAYS[item.attrib['day']]
            for region in item.attrib['territories'].split():
                if region in rows:
                    raise ValueError('duplicate region field: ' + field + '/' + region)
                if re.fullmatch(r'(?:[A-Z]{2}|[0-9]{3})', region) is None:
                    raise ValueError('invalid region spelling')
                rows[region] = value
        if '001' not in rows:
            raise ValueError('missing 001 default: ' + field)
        maps[field] = rows
    # Cover all primary territories, including territories whose week fields
    # inherit 001. ZZ is CLDR's unknown-region sentinel, not available data.
    territory = {x.attrib['type'] for x in root.findall('territoryInfo/territory')}
    containment = set()
    for item in root.findall('territoryContainment/group'):
        if item.get('status') == 'deprecated':
            continue
        containment.add(item.attrib['type'])
        containment.update(item.attrib['contains'].split())
    regions = set().union(territory, containment, *(set(x) for x in maps.values()))
    regions.discard('ZZ')
    if '001' not in regions or not territory or not containment:
        raise ValueError('incomplete pinned territory domain')
    if any(re.fullmatch(r'(?:[A-Z]{2}|[0-9]{3})', x) is None for x in regions):
        raise ValueError('invalid territory domain')
    result = []
    for region in sorted(regions):
        first = maps['firstDay'].get(region, maps['firstDay']['001'])
        start = maps['weekendStart'].get(region, maps['weekendStart']['001'])
        end = maps['weekendEnd'].get(region, maps['weekendEnd']['001'])
        result.append({'region': region, 'first_day': first,
                       'weekend_mask': weekend_mask(start, end)})
    return result, maps, alternatives, territory, containment


def kernel_sources(primary_root, native_root):
    actual = {p.relative_to(native_root) for p in (native_root / NATIVE).rglob('*.rs')
              if p.name not in {'profile_identity.rs', 'tests.rs'} and 'tests' not in p.parts}
    actual.add(NATIVE.with_suffix('.rs'))
    if actual != set(NATIVE_PRODUCTION):
        raise ValueError('unreviewed/missing Locale week production source reachability')
    parent = (native_root / NATIVE.with_suffix('.rs')).read_text()
    modules = set(re.findall(r'^(?:pub(?:\([^)]*\))?\s+)?mod (\w+);$', parent, re.M)) - {'tests', 'profile_identity'}
    if modules != {'profile', 'record'}:
        raise ValueError('Locale week production module closure changed')
    rows = []
    for path in sorted(NATIVE_PRODUCTION + LOCALE_HELPERS):
        owner = native_root if path in NATIVE_PRODUCTION else primary_root
        data = (owner / path).read_bytes()
        rows.append({'path': path.as_posix(), 'bytes': len(data), 'sha256': sha(data)})
    return rows


def outputs(primary_root, native_root=None):
    native_root = native_root or Path(__file__).resolve().parents[1]
    base, manifest_sha, inputs = primary_inputs(primary_root)
    rows, maps, alternatives, territory, containment = extract(
        ET.parse(base / 'common/supplemental/supplementalData.xml').getroot())
    profile = {'schema': SCHEMA, 'algorithm': ALGORITHM, 'cldr_release': RELEASE,
               'cldr_commit': COMMIT, 'primary_manifest_sha256': manifest_sha,
               'primary_source_sha256': inputs[0]['sha256'], 'regions': rows}
    profile_bytes = encode(profile)
    digest = sha(profile_bytes)
    sources = kernel_sources(primary_root, native_root)
    kernel_input = {'domain': 'scriptw-locale-week-native-kernel-v1',
                    'profile_sha256': digest, 'sources': sources}
    kernel_digest = sha(encode(kernel_input))
    def constant(name, value):
        return 'pub const ' + name + ': [u8; 32] = [\n' + ''.join(
            '    ' + ', '.join('0x' + value[j:j+2] for j in range(i, i+32, 2)) + ',\n'
            for i in range(0, 64, 32)) + '];\n'
    identity = ('// Generated by scripts/generate-intl-locale-week-profile.py.\n'
                '// DATA admits exact profile bytes; KERNEL binds reachable native production.\n'
                + constant('LOCALE_WEEK_DATA_SHA256', digest)
                + constant('LOCALE_WEEK_KERNEL_SHA256', kernel_digest)).encode()
    provenance = {
        'schema': SCHEMA, 'algorithm': ALGORITHM, 'cldr_release': RELEASE, 'cldr_commit': COMMIT,
        'primary_manifest_path': (PRIMARY / 'manifest.json').as_posix(),
        'primary_manifest_sha256': manifest_sha, 'inputs': inputs,
        'profile_sha256': digest, 'effective_region_rows': len(rows),
        'kernel_sha256': kernel_digest, 'kernel_input': kernel_input,
        'kernel_source_closure': 'explicit production module inventory; missing/added production modules reject',
        'kernel_excludes': ['tests.rs', 'profile_identity.rs'],
        'explicit_field_rows': {k: len(v) for k,v in maps.items()},
        'territory_info_rows': len(territory), 'active_containment_domain': len(containment),
        'default_inheritance': 'per-field 001; all active pinned territories included',
        'unavailable_region': 'ZZ sentinel and non-domain regions are not profile rows',
        'alternative_rows_not_selected': alternatives,
        'weekend_order': 'ISO numeric ascending from an inclusive cyclic whole-day range',
        'minimalDays_exposed': False, 'existing_primary_data_changed': False,
        'generation_kind': 'source-only; no product execution',
    }
    return {DEST / 'profile.json': profile_bytes, DEST / 'provenance.json': encode(provenance),
            IDENTITY: identity}


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--primary-root', type=Path, default=Path(__file__).resolve().parents[1])
    p.add_argument('--output-root', type=Path, default=Path(__file__).resolve().parents[1])
    p.add_argument('--check', action='store_true')
    a = p.parse_args()
    for path, content in outputs(a.primary_root, a.output_root).items():
        actual = a.output_root / path
        if a.check:
            if actual.read_bytes() != content:
                raise ValueError('generated output differs: ' + str(path))
        else:
            actual.parent.mkdir(parents=True, exist_ok=True)
            actual.write_bytes(content)
    print(json.dumps({'status': 'PASS SOURCE ONLY', 'outputs': 3, 'Cargo': 0, 'product_execution': 0}))


if __name__ == '__main__':
    main()
