#!/usr/bin/env python3
"""Bind absent-co Locale information to the genuine admitted Collator profile owner."""
import argparse
import gzip
import hashlib
import json
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NATIVE = Path('crates/scriptw-intl/src/provider/locale_collations.rs')
IDENTITY = NATIVE.with_suffix('') / 'kernel_identity.rs'
DATA = Path('crates/scriptw-intl/data/locale-collations/kernel-source-manifest.json')
COLLATOR = Path('crates/scriptw-intl/data/collator-icu-2')
NUMBER = Path('crates/scriptw-intl/data/number-cldr-47')
COLLATOR_PRODUCTION = [
    'crates/scriptw-intl/src/collator.rs', 'crates/scriptw-intl/src/collator/profiles.rs',
    'crates/scriptw-intl/src/collator/identity.rs', 'crates/scriptw-intl/src/collator_protocol.rs',
    'crates/scriptw-intl/src/collator/profiles/preferences.rs',
    'crates/scriptw-intl-collator-data/src/lib.rs', 'crates/scriptw-intl-collator-data/src/baked.rs',
]
HELPERS = [
    'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/provider/conformance.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs', 'crates/scriptw-intl/src/lib.rs', 'crates/scriptw-intl/src/artifact_identity.rs', 'crates/scriptw-intl/src/protocol.rs',
    'crates/scriptw-intl/src/service_selection.rs',
    'crates/scriptw-intl/src/selection/manifest.rs',
    'crates/scriptw-intl/src/selection/export.rs',
    'crates/scriptw-intl/src/locale_information_wire.rs', 'crates/scriptw-intl/src/provider/region_preference.rs',
    'crates/scriptw-intl/src/identifiers.rs', 'Cargo.lock', 'Cargo.toml', 'crates/scriptw-intl/Cargo.toml',
    'crates/scriptw-intl-collator-data/Cargo.toml',
]

# Capture the actual selected image, keyword authority and artifact boundary.
# This service's own generated kernel outputs are excluded.
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
    'crates/scriptw-intl/src/collator_image.rs',
    'crates/scriptw-intl/src/collator_image/projection.rs',
    'crates/scriptw-intl/src/collator_image/projection/rows.rs',
    'crates/scriptw-intl/src/image_build/collator.rs',
    'crates/scriptw-intl/data/collator-icu-2/locale-inventory.json',
    'crates/scriptw-intl/src/number_image.rs',
    'scripts/generate-intl-locale-collations-identity.py',
    'scripts/generated_output_transaction.py',
    'crates/scriptw-intl/src/collator_markers.rs',
    'crates/scriptw-intl/src/image_build/export_contract.rs',
])


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def pretty(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def row(name, raw):
    return {'path': str(name), 'sha256': sha(raw), 'bytes': len(raw)}


def number_production(root):
    paths = []
    for path in (root / 'crates/scriptw-intl/src/number_format').rglob('*.rs'):
        if 'tests' not in path.parts and path.name != 'tests.rs':
            paths.append(path.relative_to(root).as_posix())
    # Reuse the existing closed production authority, not a fresh permissive census.
    import importlib.util
    spec = importlib.util.spec_from_file_location('collations_number_source_authority', root / 'scripts/generate-intl-locale-numbering-systems-identity.py')
    authority = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(authority)
    if set(paths) != set(authority.NUMBER_PRODUCTION):
        raise ValueError('missing or unreviewed NumberFormat production module')
    return sorted(paths)


def kernel_sources(root):
    native = root / NATIVE.with_suffix('')
    if any(path.name not in {'tests.rs', 'kernel_identity.rs'} for path in native.rglob('*.rs')):
        raise ValueError('unreviewed Locale collation production module')
    collator_dir = root / 'crates/scriptw-intl/src/collator'
    actual = {path.relative_to(root).as_posix() for path in collator_dir.rglob('*.rs')
              if 'tests' not in path.parts and path.name != 'tests.rs'}
    if actual != {'crates/scriptw-intl/src/collator/profiles.rs', 'crates/scriptw-intl/src/collator/identity.rs',
                  'crates/scriptw-intl/src/collator/profiles/preferences.rs'}:
        raise ValueError('missing or unreviewed Collator production module')
    projection = root / 'crates/scriptw-intl/src/collator_image'
    actual_projection = {path.relative_to(root).as_posix() for path in projection.rglob('*.rs')
                         if 'tests' not in path.parts and path.name != 'tests.rs'}
    if actual_projection != {'crates/scriptw-intl/src/collator_image/projection.rs',
                             'crates/scriptw-intl/src/collator_image/projection/rows.rs'}:
        raise ValueError('missing or unreviewed Collator projection production module')
    names = sorted(COLLATOR_PRODUCTION + HELPERS + number_production(root) + [str(NATIVE)])
    return [row(name, (root / name).read_bytes()) for name in names]


def locale_independent_profile_evidence(admission, receipt_sha256):
    """The prescribed pair has exact, historically consumed root sort profiles."""
    result = []
    for name in ['emoji', 'eor']:
        profiles = [p for p in admission['profiles']
                    if p['locale'] == 'und' and p['attributes'] == name]
        if len(profiles) != 1:
            raise ValueError('missing or duplicate locale-independent root profile: ' + name)
        profile = profiles[0]
        if profile['option_control_count'] != 48 or len(profile['option_controls']) != 48:
            raise ValueError('incomplete locale-independent root constructor controls')
        for control in [profile['default'], *profile['option_controls']]:
            loads = control['loads']
            metadata = [p for p in loads if p['marker'] == 'CollationMetadataV1']
            if len(metadata) != 1:
                raise ValueError('missing root metadata consumption proof')
            for load in loads:
                expected_attributes = name if load['marker'] in {
                    'CollationMetadataV1', 'CollationTailoringV1', 'CollationReorderingV1'} else ''
                if (load['requested_locale'] != 'und' or load['returned_locale'] != 'und'
                        or load['requested_attributes'] != expected_attributes
                        or load['internal_locale_fallback'] is not False):
                    raise ValueError('locale-independent root profile consumed a foreign association')
        result.append({'locale': 'und', 'collation': name,
                       'actual_profile_sha256': sha(pretty(profile)),
                       'metadata_bits': profile['metadata_bits'],
                       'constructor_configurations': 49,
                       'default_consumed_loads': profile['default']['loads']})
    return {'historical_admission_receipt_sha256': receipt_sha256,
            'profiles': result,
            'current_native_admission': 'Setup loads and retains both profiles through existing checked ICU constructors; source-only recipe does not claim current execution.'}


def admitted_data(root, with_root_evidence=False):
    # Reuse the original producer's read-only archive/export/constructor receipt
    # checks. This authenticates historical data ancestry without reexporting or
    # claiming that native tests for this new source have run.
    import importlib.util
    spec = importlib.util.spec_from_file_location('collations_genuine_source_authority', root / 'scripts/generate-intl-collator-data.py')
    producer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(producer)
    inputs, members, _ = producer.sources(root=root)
    actual_admission = members['evidence/admission-receipt.json']
    root_evidence = locale_independent_profile_evidence(json.loads(actual_admission), sha(actual_admission))
    raw_manifest = (root / COLLATOR / 'manifest.json').read_bytes()
    manifest = json.loads(raw_manifest)
    if manifest['schema_version'] != 1 or len(manifest['generated_outputs']) != 10:
        raise ValueError('unreviewed Collator data inventory')
    if manifest['source_inputs_sha256'] != sha((root / COLLATOR / 'source-inputs.json').read_bytes()) or manifest['generated_outputs'] != inputs['generated_outputs']:
        raise ValueError('Collator manifest differs from actual source/export ancestry')
    rows = [row(COLLATOR / 'manifest.json', raw_manifest)]
    for name in ['source-inputs.json', 'sources.tar.gz', 'icuexportdata_icu4x-2025-05-01-77.x.zip']:
        rows.append(row(COLLATOR / name, (root / COLLATOR / name).read_bytes()))
    generated = Path(manifest['generated_owner_path'])
    expected = set()
    for item in manifest['generated_outputs']:
        path = generated / item['path']
        raw = (root / path).read_bytes()
        if sha(raw) != item['sha256'] or len(raw) != item['bytes']:
            raise ValueError('genuine Collator generated payload differs')
        rows.append(row(path, raw))
        expected.add(str(path))
    actual = {str(path.relative_to(root)) for path in (root / generated).iterdir() if path.is_file()}
    if actual != expected:
        raise ValueError('missing or unreviewed generated Collator payload')
    canonical = (root / NUMBER / 'profiles.json.gz').read_bytes()
    number_manifest = json.loads((root / NUMBER / 'payload-manifest.json').read_bytes())
    profile = json.loads(gzip.decompress(canonical))
    if profile['complete'] is not True or len(profile['locales']) != 1082:
        raise ValueError('incomplete candidate locale inventory')
    payload = (root / NUMBER / 'profiles.bin').read_bytes()
    if number_manifest['payload_sha256'] != sha(payload) or number_manifest['canonical_sha256'] != sha(gzip.decompress(canonical)):
        raise ValueError('NumberFormat candidate inventory payload differs')
    for name in ['profiles.bin', 'profiles.json.gz', 'payload-manifest.json', 'source-manifest.json']:
        rows.append(row(NUMBER / name, (root / NUMBER / name).read_bytes()))
    spec = importlib.util.spec_from_file_location('collations_genuine_number_authority', root / 'scripts/generate-intl-locale-numbering-systems-identity.py')
    number_authority = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(number_authority)
    number_authority.admitted_data(root, root)
    rows = sorted(rows, key=lambda item: item['path'])
    return (rows, root_evidence) if with_root_evidence else rows


def admit_build_selection(root):
    """Bind current feature and package choices separately from source ancestry."""
    manifest = tomllib.loads((root / 'crates/scriptw-intl/Cargo.toml').read_text())
    features = {'runtime': ['serde'], 'build': ['datagen']}
    for section, label in [('dependencies', 'runtime'), ('build-dependencies', 'build')]:
        for package, version in [('icu_collator', '=2.0.0'), ('icu_normalizer', '=2.0.1')]:
            choice = manifest[section][package]
            if (choice['version'] != version or choice['default-features'] is not False
                    or sorted(choice['features']) != features[label]):
                raise ValueError('unreviewed actual Collator ' + package + ' ' + label + ' feature selection')
    lock = tomllib.loads((root / 'Cargo.lock').read_text())['package']
    required = {'icu_collator': '2.0.0', 'icu_normalizer': '2.0.1',
                'icu_provider': '2.0.0', 'icu_provider_blob': '2.0.0',
                'icu_provider_adapters': '2.0.0', 'icu_locale': '2.0.0',
                'icu_locale_data': '2.0.0'}
    def select(name, version=None):
        rows = [row for row in lock if row['name'] == name
                and (version is None or row['version'] == version)]
        if (len(rows) != 1
                or rows[0].get('source') != 'registry+https://github.com/rust-lang/crates.io-index'
                or len(rows[0].get('checksum', '')) != 64):
            raise ValueError('unreviewed actual Collator package selection: ' + name)
        return rows[0]
    queue = [select(name, version) for name, version in sorted(required.items())]
    packages = {}
    while queue:
        row = queue.pop()
        key = (row['name'], row['version'])
        if key in packages:
            continue
        packages[key] = {field: row[field] for field in ('name', 'version', 'source', 'checksum')}
        for dependency in row.get('dependencies', []):
            words = dependency.split()
            queue.append(select(words[0], words[1] if len(words) > 1 else None))
    return {'features': features, 'normalizer_features': features,
            'packages': [packages[key] for key in sorted(packages)]}


def outputs(root):
    data, root_evidence = admitted_data(root, with_root_evidence=True)
    recipe = {'schema': 1, 'domain': 'scriptw-locale-collations-native-v1',
              'selection': 'NoCo checked request; actual CollatorProfiles matching(Lookup) per-locale admitted sort map; unmatched checked locale-independent und emoji/eor profile owner',
              'global_union_selection': False, 'formatting_DefaultLocale_selection': False,
              'producer_checks': 'read-only historical genuine export/constructor ancestry; no new Cargo reexport or runtime admission',
              'excluded': ['standard', 'search', 'searchjl'],
              'locale_independent_profile_admission': root_evidence,
              'data': data, 'actual_build_selection': admit_build_selection(root),
              'production_sources': kernel_sources(root)}
    digest = sha(pretty(recipe))
    identity = ('// Generated by scripts/generate-intl-locale-collations-identity.py.\n'
                '// Binds genuine Collator payloads and the actual per-locale admitted query.\n'
                'pub const LOCALE_COLLATIONS_KERNEL_SHA256: [u8; 32] = [\n'
                + ''.join('    ' + ', '.join('0x' + digest[j:j+2] for j in range(i, i+32, 2)) + ',\n' for i in range(0, 64, 32))
                + '];\n').encode()
    return {DATA: pretty({**recipe, 'kernel_sha256': digest}), IDENTITY: identity}


def main():
    from generated_output_transaction import recover_outputs, publish_outputs

    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    root = args.root.resolve()
    recover_outputs(root, 'intl-locale-collations', check=args.check)
    results = outputs(root)
    if args.check:
        for name, raw in results.items():
            if (root / name).read_bytes() != raw:
                raise ValueError('generated Locale collation output differs: ' + str(name))
    else:
        publish_outputs(root, 'intl-locale-collations', results)
    print(json.dumps({'status': 'PASS SOURCE ONLY', 'outputs': len(results), 'global_union_used': False,
                      'Cargo_commands': 0, 'product_commands': 0}))


if __name__ == '__main__':
    main()
