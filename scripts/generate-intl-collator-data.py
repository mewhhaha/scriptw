#!/usr/bin/env python3
"""Verify genuine source/export ancestry and bind consumed Collator runtime data.

--check is entirely source/data-only. --reexport performs the exact isolated
pinned exporter recipe under target, compares all output bytes, and never writes
product inputs. The latter requires actual Cargo and is a batch executor action.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DATA = Path('crates/scriptw-intl/data/collator-icu-2')
OWNER = Path('crates/scriptw-intl/src/collator')
EXPORT_OWNER = Path('crates/scriptw-intl-collator-data/src')

# The pinned exporter emits a schematic, ignored example with a placeholder
# include path. Keep its original bytes and replace only that documentation;
# all macro bodies, include paths and exported ICU data remain byte-identical.
SCHEMATIC_EXAMPLE = '''# [doc = r" ```ignore"] # [doc = r" struct MyProvider;"] # [doc = r" const _: () = {"] # [doc = r#"     include!("path/to/generated/macros.rs");"#] # [doc = r"     make_provider!(MyProvider);"] # [doc = r"     impl_core_helloworld_v1!(MyProvider);"] # [doc = r" }"] # [doc = r" ```"]'''
PROVIDER_EXAMPLE = '''```rust
use icu_provider::{marker::NeverMarker, DataErrorKind, DataProvider, DataRequest};
use scriptw_intl_collator_data::__make_provider;
struct MyProvider;
__make_provider!(MyProvider);
let result = <MyProvider as DataProvider<NeverMarker<()>>>::load(
    &MyProvider, DataRequest::default(),
);
assert_eq!(result.unwrap_err().kind, DataErrorKind::MarkerNotFound);
```'''


def normalize_provider_documentation(raw):
    old = SCHEMATIC_EXAMPLE.encode()
    if raw.count(old) != 1:
        raise ValueError('pinned exporter documentation changed')
    new = ' '.join('# [doc = ' + json.dumps(line) + ']'
                   for line in PROVIDER_EXAMPLE.splitlines()).encode()
    return raw.replace(old, new, 1)


def consumed_export(inputs, export, *, refresh_documentation=False, root=None):
    root = ROOT if root is None else Path(root)
    projection = inputs['documentation_projection']
    if (set(projection) != {'schema_version', 'raw_mod', 'policy'}
            or projection['schema_version'] != 1
            or projection['policy'] != 'exact provider example documentation only'):
        raise ValueError('unreviewed Collator documentation projection')
    raw_row = projection['raw_mod']
    if set(raw_row) != {'path', 'sha256', 'bytes'} or raw_row['path'] != 'exported-mod.rs':
        raise ValueError('unreviewed raw exporter module owner')
    raw = (root / DATA / raw_row['path']).read_bytes()
    original = next(row for row in export['generated_files'] if row['path'] == 'mod.rs')
    if (sha(raw) != original['sha256'] or len(raw) != original['bytes']
            or raw_row['sha256'] != original['sha256'] or raw_row['bytes'] != original['bytes']):
        raise ValueError('raw exporter module identity mismatch')
    projected = normalize_provider_documentation(raw)
    expected = [dict(row) for row in export['generated_files']]
    for row in expected:
        if row['path'] == 'mod.rs':
            row.update(sha256=sha(projected), bytes=len(projected))
    if inputs['generated_outputs'] != expected and not refresh_documentation:
        raise ValueError('consumed Collator projection inventory mismatch')
    for row in expected:
        content = (root / EXPORT_OWNER / 'generated' / row['path']).read_bytes()
        if sha(content) != row['sha256'] or len(content) != row['bytes']:
            raise ValueError('consumed generated source identity mismatch: ' + row['path'])
    if refresh_documentation:
        # Only the documentation projected from the verified original export
        # may change. Every actual macro/data file above must still match.
        inputs['generated_outputs'] = expected
    return projected

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def sources(*, refresh_documentation=False, root=None):
    root = ROOT if root is None else Path(root)
    raw = (root / DATA / 'source-inputs.json').read_bytes()
    inputs = json.loads(raw)
    if inputs['schema_version'] != 1:
        raise ValueError('unsupported Collator input schema')
    archive = (root / DATA / inputs['source_archive']).read_bytes()
    if sha(archive) != inputs['source_archive_sha256'] or len(archive) != inputs['source_archive_bytes']:
        raise ValueError('source archive identity mismatch')
    members = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as tar:
        for row in tar:
            name = Path(row.name)
            if not row.isfile() or name.is_absolute() or '..' in name.parts or row.name in members:
                raise ValueError('nonregular or duplicate source archive member')
            members[row.name] = tar.extractfile(row).read()
    expected = {row['path']:row for row in inputs['source_files']}
    if expected.keys() != members.keys() or len(expected) != len(inputs['source_files']):
        raise ValueError('source archive census mismatch')
    for name, content in members.items():
        if sha(content) != expected[name]['sha256'] or len(content) != expected[name]['bytes']:
            raise ValueError('source member identity mismatch: '+name)
    # Full actual crate archives carry their registry checksums in the real locks.
    locks = [tomllib.loads(members['foundation/'+name].decode()) for name in ['Cargo.lock','admission/Cargo.lock']]
    locks.append(tomllib.loads(members['evidence/product-baseline-Cargo.lock'].decode()))
    for name, content in members.items():
        if name.startswith('crates/'):
            stem = Path(name).name.removesuffix('.crate')
            package, version = stem.rsplit('-',1)
            rows = [row for lock in locks for row in lock['package'] if row['name']==package and row['version']==version]
            if not rows or any(row['checksum']!=sha(content) for row in rows):
                raise ValueError('full crate is not a pinned actual-lock package: '+name)
    genuine = (root / DATA / inputs['icu_export']['archive']).read_bytes()
    if sha(genuine)!=inputs['icu_export']['sha256'] or len(genuine)!=inputs['icu_export']['bytes']:
        raise ValueError('genuine ICU ZIP identity mismatch')
    with zipfile.ZipFile(io.BytesIO(genuine)) as archive:
        if archive.testzip() is not None:
            raise ValueError('genuine ICU ZIP CRC mismatch')
    export = json.loads(members['evidence/export-receipt.json'])
    admission = json.loads(members['evidence/admission-receipt.json'])
    if sha(members['evidence/export-receipt.json'])!=inputs['actual_foundation_export_receipt_sha256'] or sha(members['evidence/admission-receipt.json'])!=inputs['actual_foundation_admission_receipt_sha256']:
        raise ValueError('actual proof receipt identity mismatch')
    if admission['export_receipt_sha256']!=inputs['actual_foundation_export_receipt_sha256'] or admission['actual_metadata_identifiers']!=144 or admission['constructors_per_identifier']!=49 or len(admission['profiles'])!=144:
        raise ValueError('actual constructor admission closure mismatch')
    if sha(members['foundation/Cargo.lock'])!=export['cargo_lock_sha256'] or sha(members['foundation/admission/Cargo.lock'])!=admission['cargo_lock_sha256']:
        raise ValueError('actual proof lock association mismatch')
    consumed_export(inputs, export, refresh_documentation=refresh_documentation, root=root)
    inventory=root/inputs['candidate_locale_inventory']['source']
    if sha(inventory.read_bytes())!=inputs['candidate_locale_inventory']['sha256']:
        raise ValueError('candidate locale inventory identity mismatch')
    return inputs, members, export

def result(inputs, *, root=None, source_inputs_bytes=None):
    root = ROOT if root is None else Path(root)
    lock = tomllib.loads((root / 'Cargo.lock').read_text())
    image_packages = []
    for name, version in sorted({'icu_provider': '2.0.0', 'icu_provider_blob': '2.0.0',
                                 'icu_provider_adapters': '2.0.0', 'icu_locale': '2.0.0',
                                 'icu_locale_data': '2.0.0'}.items()):
        selected = [row for row in lock['package'] if row['name'] == name]
        if len(selected) != 1 or selected[0]['version'] != version:
            raise ValueError('unexpected selected image dependency: ' + name)
        row = selected[0]
        if row.get('source') != 'registry+https://github.com/rust-lang/crates.io-index' or len(row.get('checksum', '')) != 64:
            raise ValueError('selected image dependency lacks registry identity: ' + name)
        image_packages.append({key: row[key] for key in ('name', 'version', 'source', 'checksum')})
    owners=[Path('Cargo.toml'),Path('crates/scriptw-intl/Cargo.toml'),
            Path('crates/scriptw-intl-collator-data/Cargo.toml'),EXPORT_OWNER/'lib.rs',EXPORT_OWNER/'baked.rs',
            Path('crates/scriptw-intl/src/collator.rs'),Path('crates/scriptw-intl/src/collator_protocol.rs'),
            OWNER/'profiles.rs',Path('scripts/generate-intl-collator-data.py'),Path('crates/scriptw-intl/src/provider.rs'), Path('crates/scriptw-intl/src/selection.rs')]
    owners.extend(Path(name) for name in [
        'crates/scriptw-intl/src/provider/conformance.rs',
        'crates/scriptw-intl/src/service_selection.rs',
        'crates/scriptw-intl/src/selection/manifest.rs',
        'crates/scriptw-intl/src/selection/export.rs'])
    owners.extend(Path(name) for name in ['crates/scriptw-intl/build.rs', 'crates/scriptw-intl/src/image.rs', 'crates/scriptw-intl/src/locale_image.rs', 'crates/scriptw-intl/src/image_build/locale.rs', 'crates/scriptw-engine/src/intl_data_images.rs', 'crates/scriptw-intl/src/collator_image.rs', 'crates/scriptw-intl/src/image_build/collator.rs', 'crates/scriptw-intl/src/lib.rs', 'crates/scriptw-intl/src/artifact_identity.rs', 'crates/scriptw-intl/src/protocol.rs', 'crates/scriptw-engine/src/intl_collator_host.rs'])
    owners.extend(Path(name) for name in ['crates/scriptw-intl/src/image_build/keyword.rs', 'crates/scriptw-intl/src/provider/keyword_aliases.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs', 'crates/scriptw-intl/data/cldr-47-bcp47/manifest.json', 'crates/scriptw-intl/src/provider/language_domain.rs', 'crates/scriptw-intl/data/collator-icu-2/locale-inventory.json'])
    owners.extend(Path(name) for name in ['crates/scriptw-aot-wasm/src/emit.rs', 'crates/scriptw-aot-wasm/src/emit/module_assembly.rs'])
    # Closed, consumed projection owners copy original Postcard rows rather than
    # introducing a second export or a full-data runtime fallback.
    projection = Path('crates/scriptw-intl/src/collator_image')
    expected_projection = {projection/'projection.rs', projection/'projection/rows.rs'}
    actual_projection = {path.relative_to(root) for path in (root/projection).rglob('*.rs')
                         if 'tests' not in path.parts and path.name != 'tests.rs'}
    if actual_projection != expected_projection:
        raise ValueError('missing or unreviewed Collator projection production owner')
    owners.extend(sorted(expected_projection))
    owners.append(OWNER/'profiles/preferences.rs')
    owners.extend(Path(name) for name in [
        'crates/scriptw-intl/src/collator_markers.rs',
        'crates/scriptw-intl/src/image_build/export_contract.rs',
        'scripts/generated_output_transaction.py'])
    rows=[{'path':str(path),'sha256':sha((root/path).read_bytes())} for path in owners]
    payload={'schema_version':1,'image_packages':image_packages,'source_inputs_sha256':sha(source_inputs_bytes if source_inputs_bytes is not None else (root/DATA/'source-inputs.json').read_bytes()),'owners':rows,'candidate_inventory_sha256':inputs['candidate_locale_inventory']['sha256'],'generated_outputs':inputs['generated_outputs'],
             'generated_owner_path':str(EXPORT_OWNER/'generated'),
             'public_catalogue_policy':'full canonical CLDR candidates; all sort-default and genuine-search profiles independently admitted; only explicit sort-purpose types with complete consumed data enter per-locale/public union',
             'utf16_policy':'direct pinned compare_utf16; isolated units have ICU replacement weights, valid pairs are scalar; raw units preserved; no lexical tie-breaker',
             'available_locales_count':None,'available_sort_collations_count':None}
    digest=sha(json.dumps(payload,sort_keys=True,separators=(',',':')).encode())
    payload['data_sha256']=digest
    data=bytes.fromhex(digest)
    rust='// Generated by scripts/generate-intl-collator-data.py; do not edit.\npub(crate) const COLLATOR_DATA_SHA256: [u8; 32] = [\n'
    for start in range(0,32,16):
        rust+='    '+', '.join(f'0x{byte:02x}' for byte in data[start:start+16])+',\n'
    rust+='];\n'
    return json.dumps(payload,indent=2)+'\n',rust

def reexport(inputs,members,export, *, root=None):
    root = ROOT if root is None else Path(root)
    (root/'target').mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='collator-reexport-',dir=root/'target') as name:
        scratch=Path(name)
        for path,raw in members.items():
            if path.startswith('foundation/'):
                relative=Path(path).relative_to('foundation')
                output=scratch/relative;output.parent.mkdir(parents=True,exist_ok=True);output.write_bytes(raw)
        subprocess.run(['cargo','run','--offline','--locked','--manifest-path',str(scratch/'Cargo.toml'),'--bin','collator-export','--',str(root/DATA/inputs['icu_export']['archive'])],check=True,cwd=root)
        actual={p.name:p.read_bytes() for p in (scratch/'generated').iterdir() if p.is_file()}
        expected={row['path'] for row in export['generated_files']}
        if actual.keys()!=expected:
            raise ValueError('reexport generated-file census changed')
        for row in export['generated_files']:
            if sha(actual[row['path']])!=row['sha256']:
                raise ValueError('real reexport bytes differ: '+row['path'])
        actual['mod.rs'] = normalize_provider_documentation(actual['mod.rs'])
        for row in inputs['generated_outputs']:
            if actual[row['path']] != (root/EXPORT_OWNER/'generated'/row['path']).read_bytes():
                raise ValueError('consumed normalized reexport differs: '+row['path'])

def main():
    from generated_output_transaction import recover_outputs, publish_outputs

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--reexport', action='store_true')
    parser.add_argument('--refresh-documentation-projection', action='store_true')
    args = parser.parse_args()
    if args.check and args.refresh_documentation_projection:
        parser.error('--check cannot refresh the documentation projection')
    root = args.root.resolve()
    recover_outputs(root, 'intl-collator', check=args.check)
    inputs, members, export = sources(root=root, refresh_documentation=args.refresh_documentation_projection)
    source_inputs_bytes = ((json.dumps(inputs, indent=2) + '\n').encode()
                           if args.refresh_documentation_projection else None)
    manifest, rust = result(inputs, root=root, source_inputs_bytes=source_inputs_bytes)
    outputs = {DATA/'manifest.json': manifest.encode(), OWNER/'identity.rs': rust.encode()}
    if source_inputs_bytes is not None:
        outputs[DATA/'source-inputs.json'] = source_inputs_bytes
    if args.reexport:
        reexport(inputs, members, export, root=root)
    if args.check:
        for path, content in outputs.items():
            if not (root/path).is_file() or (root/path).read_bytes() != content:
                raise ValueError('stale generated Collator identity: ' + str(path))
    else:
        publish_outputs(root, 'intl-collator', outputs)
    print(json.dumps({'status':'source/data check complete','source_metadata_ids':144,'actual_foundation_constructor_configurations':7056,'generated_files':10,'available_locale_count':None,'public_sort_collation_count':None,'data_sha256':json.loads(manifest)['data_sha256'],'real_reexport_executed':args.reexport}))
if __name__=='__main__':main()
