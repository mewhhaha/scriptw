#!/usr/bin/env python3
"""Bind the Locale absent-nu query to its genuine NumberFormat data/source authority."""
import argparse
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

DEST = Path('crates/scriptw-intl/data/locale-numbering-systems/kernel-source-manifest.json')
IDENTITY = Path('crates/scriptw-intl/src/provider/locale_numbering_systems/kernel_identity.rs')
NATIVE = Path('crates/scriptw-intl/src/provider/locale_numbering_systems.rs')
NUMBER_ROOT = Path('crates/scriptw-intl/src/number_format')
NUMBER_PRODUCTION = ['crates/scriptw-intl/src/number_format/configuration.rs', 'crates/scriptw-intl/src/number_format/mod.rs', 'crates/scriptw-intl/src/number_format/numeric/digits.rs', 'crates/scriptw-intl/src/number_format/numeric/mod.rs', 'crates/scriptw-intl/src/number_format/numeric/notation.rs', 'crates/scriptw-intl/src/number_format/numeric/parse/thresholds.rs', 'crates/scriptw-intl/src/number_format/numeric/parse.rs', 'crates/scriptw-intl/src/number_format/numeric/plural.rs', 'crates/scriptw-intl/src/number_format/numeric/resource.rs', 'crates/scriptw-intl/src/number_format/numeric/round.rs', 'crates/scriptw-intl/src/number_format/options.rs', 'crates/scriptw-intl/src/number_format/partition/buffer.rs', 'crates/scriptw-intl/src/number_format/partition/mod.rs', 'crates/scriptw-intl/src/number_format/partition/range.rs', 'crates/scriptw-intl/src/number_format/partition/render.rs', 'crates/scriptw-intl/src/number_format/partition_resource.rs', 'crates/scriptw-intl/src/number_format/parts.rs', 'crates/scriptw-intl/src/number_format/plural_rules.rs', 'crates/scriptw-intl/src/number_format/profiles/fingerprint.rs', 'crates/scriptw-intl/src/number_format/profiles/mod.rs', 'crates/scriptw-intl/src/number_format/profiles/read.rs', 'crates/scriptw-intl/src/number_format/profiles/validate.rs']
HELPERS = [
    'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/provider/conformance.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs', 'crates/scriptw-intl/src/lib.rs',
    'crates/scriptw-intl/src/artifact_identity.rs',
    'crates/scriptw-intl/src/service_selection.rs',
    'crates/scriptw-intl/src/selection/manifest.rs',
    'crates/scriptw-intl/src/selection/export.rs',
    'crates/scriptw-intl/src/protocol.rs',
    'crates/scriptw-intl/src/provider/region_preference.rs',
    'crates/scriptw-intl/src/identifiers.rs',
    'Cargo.lock', 'Cargo.toml', 'crates/scriptw-intl/Cargo.toml',
]

NUMBER_PRODUCTION.extend([
    'crates/scriptw-intl/src/number_format/profiles/domains.rs',
    'crates/scriptw-intl/src/number_format/profiles/projection.rs',
    'crates/scriptw-intl/src/number_format/profiles/projection/closure.rs',
    'crates/scriptw-intl/src/number_format/profiles/projection/encode.rs',
])

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
    'crates/scriptw-intl/src/number_image.rs',
    'crates/scriptw-intl/src/number_image/projection.rs',
    'scripts/generate-intl-locale-numbering-systems-identity.py',
    'scripts/intl_numberformat_profile/generate_payload.py',
    'scripts/intl_numberformat_profile/profile_schema.py',
    'scripts/generate-intl-numberformat-profile.py',
    'scripts/intl_numberformat_profile/cldr_resolver.py',
    'scripts/intl_ldml_schema.py',
    'scripts/intl_positional_numbering.py',
])
DATA = Path('crates/scriptw-intl/data/number-cldr-47')
DATA_FILES = ['profiles.bin', 'profiles.json.gz', 'payload-manifest.json', 'source-manifest.json']
sha = lambda data: hashlib.sha256(data).hexdigest()
pretty = lambda value: (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def source(primary_root, native_root, path):
    path = Path(path)
    override = native_root / path
    return override if override.is_file() else primary_root / path


def kernel_sources(primary_root, native_root):
    expected = set(NUMBER_PRODUCTION)
    actual = set()
    for root in [primary_root, native_root]:
        for path in (root / NUMBER_ROOT).rglob('*.rs'):
            relative = path.relative_to(root)
            if 'tests' not in relative.parts and path.name != 'tests.rs':
                actual.add(relative.as_posix())
    if actual != expected:
        raise ValueError('missing or unreviewed NumberFormat production module')
    native_dir = native_root / NATIVE.with_suffix('')
    unexpected = [p for p in native_dir.rglob('*.rs') if p.name not in {'tests.rs', 'kernel_identity.rs'}]
    if unexpected:
        raise ValueError('unreviewed Locale numbering-system production module')
    rows = []
    for name in sorted(NUMBER_PRODUCTION + HELPERS + [NATIVE.as_posix()]):
        raw = source(primary_root, native_root, name).read_bytes()
        rows.append({'path':name, 'sha256':sha(raw), 'bytes':len(raw)})
    return rows


def admitted_data(primary_root, native_root):
    canonical = source(primary_root, native_root, DATA / 'profiles.json.gz').read_bytes()
    payload = source(primary_root, native_root, DATA / 'profiles.bin').read_bytes()
    manifest_raw = source(primary_root, native_root, DATA / 'payload-manifest.json').read_bytes()
    manifest = json.loads(manifest_raw)
    profile = json.loads(gzip.decompress(canonical))
    if profile['schema'] != 2 or profile['complete'] is not True or profile['cldr_commit'] != '2ef784e3a4168bc2a43cd1b5b9839b6636f5899c':
        raise ValueError('unreviewed NumberFormat canonical profile')
    if len(profile['locales']) != 1082 or len(profile['numbering_systems']) != 78:
        raise ValueError('incomplete NumberFormat data inventory')
    if manifest['canonical_sha256'] != sha(gzip.decompress(canonical)) or manifest['payload_sha256'] != sha(payload):
        raise ValueError('NumberFormat data digest differs')
    scripts = primary_root / 'scripts'
    sys.path.insert(0, str(scripts))
    sys.path.insert(0, str(scripts / 'intl_numberformat_profile'))
    producer_path = scripts / 'intl_numberformat_profile/generate_payload.py'
    spec = importlib.util.spec_from_file_location('locale_numbering_payload', producer_path)
    producer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(producer)
    if producer.encode(profile) != payload:
        raise ValueError('genuine NumberFormat payload differs')
    rows = []
    for name in DATA_FILES:
        raw = source(primary_root, native_root, DATA / name).read_bytes()
        rows.append({'path':(DATA / name).as_posix(), 'sha256':sha(raw), 'bytes':len(raw)})
    return rows, manifest['payload_sha256']


def outputs(primary_root, native_root=None):
    native_root = native_root or Path(__file__).resolve().parents[1]
    data, data_digest = admitted_data(primary_root, native_root)
    recipe = {'schema':1, 'domain':'scriptw-locale-numbering-systems-native-v1',
        'selection':'NoNu checked request; NumberFormat prefix/default-index; unmatched latn',
        'number_locales':1082, 'decimal_numbering_systems':78,
        'NumberFormat_payload_sha256':data_digest, 'data':data,
        'production_sources':kernel_sources(primary_root, native_root)}
    encoded = pretty(recipe)
    kernel = sha(encoded)
    identity = ('// Generated by scripts/generate-intl-locale-numbering-systems-identity.py.\n'
        '// Binds the admitted NumberFormat payload and exact reachable native sources.\n'
        'pub const LOCALE_NUMBERING_SYSTEMS_KERNEL_SHA256: [u8; 32] = [\n'
        + ''.join('    ' + ', '.join('0x' + kernel[j:j+2] for j in range(i,i+32,2)) + ',\n' for i in range(0,64,32))
        + '];\n').encode()
    return {DEST:pretty({**recipe,'kernel_sha256':kernel}), IDENTITY:identity}


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--primary-root',type=Path,default=Path(__file__).resolve().parents[1])
    parser.add_argument('--output-root',type=Path,default=Path(__file__).resolve().parents[1])
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    for name, raw in outputs(args.primary_root,args.output_root).items():
        path=args.output_root/name
        if args.check:
            if path.read_bytes()!=raw:raise ValueError('generated source identity differs: '+str(name))
        else:
            if path.exists() and path.read_bytes() == raw:
                continue
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_bytes(raw)
    print(json.dumps({'status':'PASS SOURCE ONLY','outputs':2,'NumberFormat_data_changed':False,'Cargo_commands':0,'product_commands':0}))


if __name__=='__main__':main()
