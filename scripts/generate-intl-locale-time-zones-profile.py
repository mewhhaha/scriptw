#!/usr/bin/env python3
"""Project the actual IANA2026a zone.tab through Scriptw's pinned primary catalogue."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tarfile

ROOT = Path(__file__).resolve().parents[1]
IANA = Path('crates/scriptw-intl/data/iana-tzdb-2026a')
DATA = Path('crates/scriptw-intl/data/locale-time-zones-iana2026a')
NATIVE = Path('crates/scriptw-intl/src/provider/locale_time_zones.rs')
IDENTITY = NATIVE.with_suffix('') / 'kernel_identity.rs'
PROFILE_IDENTITY = NATIVE.with_suffix('') / 'profile_identity.rs'
ARCHIVE_SHA256 = '77b541725937bb53bd92bd484c0b43bec8545e2d3431ee01f04ef8f2203ba2b7'
ZONE_TAB_SHA256 = '586b4207e6c76722de82adcda6bf49d761f668517f45a673f64da83b333eecc4'
CATALOGUE_SHA256 = 'bd10c0094e5542508260bb02480f202203cc106252b692e8260070c5cec8436a'
NAMED_PRODUCTION = [
    'crates/scriptw-intl/src/provider/named_time_zones.rs',
    'crates/scriptw-intl/src/provider/named_time_zones/catalogue.rs',
    'crates/scriptw-intl/src/provider/named_time_zones/exact_query.rs',
    'crates/scriptw-intl/src/provider/named_time_zones/gap_topology.rs',
    'crates/scriptw-intl/src/provider/named_time_zones/gap_topology/footer.rs',
    'crates/scriptw-intl/src/provider/named_time_zones/identity.rs',
    'crates/scriptw-intl/src/provider/named_time_zones/validation.rs',
]
HELPERS = [
    'crates/scriptw-intl/src/provider/time_zone_snapshot.rs',
    'crates/scriptw-intl/src/time_zone.rs',
    'crates/scriptw-intl/src/time_zone/exact.rs',
    'crates/scriptw-intl/src/time_zone/named_query.rs',
    'crates/scriptw-intl/src/identifiers.rs',
    'crates/scriptw-intl/src/provider/region_preference.rs',
    'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs',
    'crates/scriptw-intl/src/lib.rs',
    'crates/scriptw-intl/src/artifact_identity.rs',
    'crates/scriptw-intl/src/protocol.rs',
    'crates/scriptw-intl/src/locale_information_wire.rs',
    'Cargo.lock', 'Cargo.toml', 'crates/scriptw-intl/Cargo.toml',
]

# Selected image admission and real keyword/data owners; generated kernel outputs stay upstream.
HELPERS.extend([
    'crates/scriptw-intl/build.rs',
    'crates/scriptw-intl/src/image.rs',
    'crates/scriptw-intl/src/locale_image.rs',
    'crates/scriptw-intl/src/image_build/locale.rs',
    'crates/scriptw-intl/src/image_build/keyword.rs',
    'crates/scriptw-intl/src/provider/language_domain.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases.rs',
    'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs',
    'scripts/generate-intl-keyword-aliases.py',
    'crates/scriptw-engine/src/intl_data_images.rs',
    'crates/scriptw-engine/src/wasm_gc_intl_host.rs',
    'crates/scriptw-aot-wasm/src/emit.rs',
    'crates/scriptw-aot-wasm/src/emit/module_assembly.rs',
    'crates/scriptw-intl/src/named_time_zone_image.rs',
    'crates/scriptw-intl/src/image_build/named_time_zones.rs',
    'crates/scriptw-intl/src/time_zone_names_image.rs',
    'crates/scriptw-intl/src/provider/time_zone_names.rs',
    'crates/scriptw-intl/src/provider/time_zone_names/raw.rs',
    'crates/scriptw-intl/data/zone-names-cldr-47/native-profile.json',
    'crates/scriptw-intl/data/zone-names-cldr-47/native-profile-manifest.json',
    'scripts/generate-intl-locale-time-zones-profile.py',
])


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def pretty(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def row(path, raw):
    return {'path': str(path), 'bytes': len(raw), 'sha256': sha(raw)}


def rust_digest(name, digest, public=True):
    return (('pub ' if public else 'pub(super) ') + f'const {name}: [u8; 32] = [\n'
            + ''.join('    ' + ', '.join('0x' + digest[j:j+2] for j in range(i, i+32, 2)) + ',\n'
                      for i in range(0, 64, 32)) + '];\n').encode()


def primary_catalogue(raw):
    names = {}
    previous = None
    for line in raw.decode('ascii').splitlines():
        fields = line.split('\t')
        if len(fields) != 3 or not re.fullmatch('[0-9a-f]{64}', fields[2]):
            raise ValueError('malformed named catalogue row')
        name, primary, _ = fields
        if not all(re.fullmatch(r'[A-Za-z0-9._+-]+(?:/[A-Za-z0-9._+-]+)*', item) for item in [name, primary]):
            raise ValueError('invalid catalogue name')
        if previous is not None and previous >= name:
            raise ValueError('unordered or duplicate named catalogue')
        previous = name
        names[name] = primary
    if len(names) != 598:
        raise ValueError('incomplete named catalogue')
    if any(names.get(primary) != primary for primary in names.values()):
        raise ValueError('catalogue primary is missing or non-terminal')
    return names


def memberships(raw, catalogue):
    seen = set()
    projected = set()
    for line in raw.decode('ascii').splitlines():
        if not line or line.startswith('#'):
            continue
        fields = line.split('\t')
        if len(fields) not in [3, 4] or not re.fullmatch('[A-Z]{2}', fields[0]):
            raise ValueError('malformed zone.tab country row')
        if not re.fullmatch(r'[+-][0-9]{4}(?:[0-9]{2})?[+-][0-9]{5}(?:[0-9]{2})?', fields[1]):
            raise ValueError('malformed zone.tab coordinates')
        country, _, identifier = fields[:3]
        if (country, identifier) in seen:
            raise ValueError('duplicate zone.tab country membership')
        seen.add((country, identifier))
        if identifier not in catalogue:
            raise ValueError('zone.tab identifier absent from primary catalogue')
        primary = catalogue[identifier]
        if catalogue.get(primary) != primary:
            raise ValueError('projected primary is non-terminal')
        projected.add((country, primary))
    if len(seen) != 418 or len({country for country, _ in seen}) != 247:
        raise ValueError('incomplete IANA2026a country membership')
    return sorted(projected)


def admitted_inputs(root):
    archive = (root / IANA / 'tzdata2026a.tar.gz').read_bytes()
    catalogue_raw = (root / IANA / 'catalogue.tsv').read_bytes()
    if sha(archive) != ARCHIVE_SHA256 or sha(catalogue_raw) != CATALOGUE_SHA256:
        raise ValueError('unreviewed IANA archive or primary catalogue digest')
    with tarfile.open(root / IANA / 'tzdata2026a.tar.gz', 'r:gz') as handle:
        if handle.extractfile('version').read().strip() != b'2026a':
            raise ValueError('wrong IANA source release')
        zone = handle.extractfile('zone.tab').read()
    if sha(zone) != ZONE_TAB_SHA256 or (root / IANA / 'source/zone.tab').read_bytes() != zone:
        raise ValueError('zone.tab differs from the actual IANA archive')
    catalogue = primary_catalogue(catalogue_raw)
    return zone, catalogue_raw, memberships(zone, catalogue)


def kernel_sources(root, generated_profile):
    native_dir = root / NATIVE.with_suffix('')
    if any(path.name not in {'tests.rs', 'profile_identity.rs', 'kernel_identity.rs'} for path in native_dir.rglob('*.rs')):
        raise ValueError('unreviewed Locale time-zone production module')
    actual = set()
    directory = root / 'crates/scriptw-intl/src/provider/named_time_zones'
    for path in directory.rglob('*.rs'):
        relative = path.relative_to(root)
        if 'tests' not in relative.parts and not path.name.endswith('tests.rs'):
            actual.add(relative.as_posix())
    if actual != set(NAMED_PRODUCTION[1:]):
        raise ValueError('missing or unreviewed named-zone production module')
    rows = [row(name, (root / name).read_bytes()) for name in sorted(NAMED_PRODUCTION + HELPERS + [str(NATIVE)])]
    rows.append(row(PROFILE_IDENTITY, generated_profile))
    return sorted(rows, key=lambda item: item['path'])


def outputs(root):
    zone, catalogue, pairs = admitted_inputs(root)
    regions = ''.join(country + '\t' + name + '\n' for country, name in pairs).encode('ascii')
    profile_identity = ('// Generated by scripts/generate-intl-locale-time-zones-profile.py.\n'
                        '// Binds actual IANA2026a country membership and the canonical-primary projection.\n').encode()
    profile_identity += rust_digest('ZONE_TAB_SHA256', sha(zone), False)
    profile_identity += rust_digest('LOCALE_TIME_ZONES_PROFILE_SHA256', sha(regions))
    manifest = pretty({
        'schema': 1, 'domain': 'scriptw-locale-time-zones-country-membership-v1', 'iana_version': '2026a',
        'selection': 'explicit base region only; zone.tab country rows; admitted terminal primary identity; sorted unique',
        'source_rows': 418, 'regions': 247, 'projected_memberships': len(pairs), 'named_catalogue_rows': 598,
        'source_archive': row(IANA / 'tzdata2026a.tar.gz', (root / IANA / 'tzdata2026a.tar.gz').read_bytes()),
        'source_zone_tab': row(IANA / 'source/zone.tab', zone),
        'named_catalogue': row(IANA / 'catalogue.tsv', catalogue),
        'projection': row(DATA / 'regions.tsv', regions),
        'region_inference': False, 'rg_sd_overrides': False, 'zone1970_multi_country_projection': False,
    })
    data_rows = [row(DATA / name, raw) for name, raw in [('zone.tab', zone), ('regions.tsv', regions), ('manifest.json', manifest)]]
    recipe = {'schema': 1, 'domain': 'scriptw-locale-time-zones-native-v1',
              'data': data_rows, 'production_sources': kernel_sources(root, profile_identity)}
    digest = sha(pretty(recipe))
    identity = b'// Generated by scripts/generate-intl-locale-time-zones-profile.py.\n' + rust_digest('LOCALE_TIME_ZONES_KERNEL_SHA256', digest)
    return {DATA / 'zone.tab': zone, DATA / 'regions.tsv': regions, DATA / 'manifest.json': manifest,
            PROFILE_IDENTITY: profile_identity, DATA / 'kernel-source-manifest.json': pretty({**recipe, 'kernel_sha256': digest}),
            IDENTITY: identity}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    results = outputs(args.root)
    for name, raw in results.items():
        path = args.root / name
        if args.check:
            if path.read_bytes() != raw:
                raise ValueError('generated Locale time-zone output differs: ' + str(name))
        else:
            if path.exists() and path.read_bytes() == raw:
                continue
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
    print(json.dumps({'status': 'PASS SOURCE ONLY', 'outputs': len(results), 'actual_iana_rows': 418,
                      'actual_regions': 247, 'Cargo_commands': 0, 'product_commands': 0}))


if __name__ == '__main__':
    main()
