#!/usr/bin/env python3
"""Bind the consumed DisplayNames and RelativeTimeFormat kernels and pinned data."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SHARED = {
    'Cargo.toml', 'crates/scriptw-intl/Cargo.toml',
    'crates/scriptw-intl/src/lib.rs', 'crates/scriptw-intl/src/artifact_identity.rs',
    'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/provider/conformance.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs',
    'crates/scriptw-intl/src/service_selection.rs',
    'crates/scriptw-intl/src/selection/manifest.rs',
    'crates/scriptw-intl/src/selection/export.rs',
    'crates/scriptw-intl/src/protocol.rs', 'crates/scriptw-intl/src/identifiers.rs',
    'crates/scriptw-intl/src/supported_values.rs',
    'crates/scriptw-intl/src/number_protocol.rs',
    'crates/scriptw-intl/src/image.rs', 'crates/scriptw-intl/src/locale_image.rs',
    'crates/scriptw-intl/src/image_build/locale.rs', 'crates/scriptw-intl/build.rs',
    'crates/scriptw-engine/src/intl_data_images.rs',
    'crates/scriptw-engine/src/wasm_gc_intl_host.rs',
    'scripts/generate-intl-display-relative-identity.py',
}
SHARED.update(['crates/scriptw-intl/src/image_build/keyword.rs', 'crates/scriptw-intl/src/provider/keyword_aliases.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs', 'crates/scriptw-intl/data/cldr-47-bcp47/manifest.json', 'crates/scriptw-intl/src/provider/language_domain.rs'])
SHARED.update(['crates/scriptw-aot-wasm/src/emit.rs', 'crates/scriptw-aot-wasm/src/emit/module_assembly.rs'])
PACKAGES = {
    'icu_locale': '2.0.0', 'icu_locale_core': '2.0.1',
    'icu_locale_data': '2.0.0', 'icu_provider': '2.0.0',
    'icu_provider_blob': '2.0.0', 'icu_provider_adapters': '2.0.0',
    'serde': '1.0.228', 'serde_json': '1.0.149', 'sha2': '0.10.9',
}
RELATIVE_PACKAGES = {**PACKAGES, 'ryu-js': '1.0.2'}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def production_sources(repository, owner, suffix='*.rs'):
    return {
        p.relative_to(repository).as_posix()
        for p in (repository / owner).rglob(suffix)
        if 'tests' not in p.relative_to(repository / owner).parts
        and p.name not in ('tests.rs', 'kernel_identity.rs')
        and not p.name.endswith('_tests.rs')
    }


def check_manifest(repository, relative, *, root_relative=False):
    document = json.loads((repository / relative).read_text())
    rows = document['files']
    if isinstance(rows, dict):
        rows = [dict(row, path=path) for path, row in rows.items()]
    paths = {relative}
    for row in rows:
        path = Path(row['path'])
        if path.is_absolute() or '..' in path.parts:
            raise ValueError('invalid captured source path')
        if not root_relative:
            path = Path(relative).parent / path
        if path.as_posix() in paths:
            raise ValueError('duplicate captured source path')
        raw = (repository / path).read_bytes()
        if sha(raw) != row['sha256'] or len(raw) != row['bytes']:
            raise ValueError(f'captured input changed: {path}')
        paths.add(path.as_posix())
    return paths


def packages(repository, required):
    lock = tomllib.loads((repository / 'Cargo.lock').read_text())
    result = []
    for name, version in sorted(required.items()):
        rows = [p for p in lock['package'] if p['name'] == name and p['version'] == version]
        if len(rows) != 1 or rows[0].get('source') != 'registry+https://github.com/rust-lang/crates.io-index':
            raise ValueError(f'wrong locked dependency: {name}')
        row = rows[0]
        if len(row.get('checksum', '')) != 64:
            raise ValueError(f'missing locked checksum: {name}')
        result.append({k: row[k] for k in ('name', 'version', 'source', 'checksum')})
    return result


def generate(repository):
    repository = repository.resolve()
    common = SHARED | production_sources(repository, 'crates/scriptw-intl/src/provider/language_domain')
    common |= production_sources(repository, 'crates/scriptw-intl/src/provider/keyword_aliases')
    common |= {'crates/scriptw-intl/src/provider/language_domain.rs',
               'crates/scriptw-intl/src/provider/keyword_aliases.rs'}
    common |= production_sources(repository, 'crates/scriptw-intl/src/number_protocol')
    common |= production_sources(repository, 'scripts/intl_cldr_profile', '*.py')
    common |= {'scripts/intl_cldr_profile.py', 'scripts/intl_ldml_schema.py',
               'scripts/intl_positional_numbering.py', 'scripts/intl_calendar_eras.py'}
    dn = common | production_sources(repository, 'crates/scriptw-intl/src/display_names')
    dn |= production_sources(repository, 'crates/scriptw-intl/src/display_names_image')
    dn |= {'crates/scriptw-intl/src/display_names.rs', 'crates/scriptw-intl/src/display_names_protocol.rs',
           'crates/scriptw-intl/src/display_names_image.rs',
           'crates/scriptw-intl/src/display_names/profile.json',
           'crates/scriptw-intl/src/display_names/provenance.json',
           'scripts/generate-intl-displaynames-profile.py',
           'crates/scriptw-engine/src/intl_display_names_host.rs'}
    dn |= check_manifest(repository, 'crates/scriptw-intl/data/datetime-cldr-47/manifest.json')
    dn |= check_manifest(repository, 'crates/scriptw-intl/data/cldr-47-bcp47/manifest.json')
    dn |= check_manifest(repository, 'crates/scriptw-intl/data/calendar-eras-cldr-48/manifest.json')
    dn |= check_manifest(repository, 'crates/scriptw-intl/data/numbering-tols-cldr-48/source-manifest.json')
    rtf = common | production_sources(repository, 'crates/scriptw-intl/src/relative_time_format')
    rtf |= production_sources(repository, 'crates/scriptw-intl/src/relative_time_protocol')
    rtf |= production_sources(repository, 'crates/scriptw-intl/src/relative_time_image')
    rtf |= production_sources(repository, 'crates/scriptw-intl/src/number_image')
    rtf |= production_sources(repository, 'crates/scriptw-intl/src/number_format')
    rtf |= production_sources(repository, 'crates/scriptw-intl/src/plural_rules')
    rtf |= {'crates/scriptw-intl/src/relative_time_format.rs', 'crates/scriptw-intl/src/relative_time_protocol.rs',
            'crates/scriptw-intl/src/number_operation.rs', 'crates/scriptw-intl/src/plural_protocol.rs',
            'crates/scriptw-intl/src/relative_time_image.rs', 'crates/scriptw-intl/src/number_image.rs',
            'crates/scriptw-intl/data/number-cldr-47/payload-manifest.json',
            'crates/scriptw-intl/src/number_format/profiles/fingerprint.rs',
            'crates/scriptw-intl/src/relative_time_format/generated/profile.json',
            'crates/scriptw-intl/data/number-cldr-47/profiles.bin',
            'crates/scriptw-intl/data/relative-time-cldr-47/report.json',
            'scripts/generate-intl-relative-time-profile.py',
            'crates/scriptw-engine/src/intl_relative_time_host.rs'}
    rtf |= production_sources(repository, 'crates/scriptw-intl/src/plural_protocol')
    rtf |= check_manifest(repository, 'crates/scriptw-intl/data/relative-time-cldr-47/source-inputs.json', root_relative=True)
    outputs = {}
    for service, paths, constant, data_dir, source_dir, required in (
        ('DisplayNames', dn, 'DISPLAY_NAMES_KERNEL_SHA256', 'displaynames-cldr-47', 'display_names', PACKAGES),
        ('RelativeTimeFormat', rtf, 'RELATIVE_TIME_KERNEL_SHA256', 'relative-time-cldr-47', 'relative_time_format', RELATIVE_PACKAGES),
    ):
        locked = packages(repository, required)
        records = []
        for path in sorted(paths):
            raw = (repository / path).read_bytes()
            records.append({'path': path, 'bytes': len(raw), 'sha256': sha(raw)})
        recipe = {'schema_version': 1, 'service': service, 'host_call_abi': int(re.fullmatch(r'.*?pub const INTL_HOST_CALL_ABI_VERSION: u16 = (\d+);.*', (repository / 'crates/scriptw-intl/src/lib.rs').read_text(), re.S).group(1)),
                  'wire_version': 1, 'files': records, 'packages': locked,
                  'scope': 'actual immutable image admission, retained selected data owners, checked native algorithm, primitive codecs, host consumer and captured CLDR inputs; tests and self identities excluded; runtime acceptance pending'}
        digest = hashlib.sha256(json.dumps(recipe, sort_keys=True, separators=(',', ':')).encode()).digest()
        rust = '// Generated by scripts/generate-intl-display-relative-identity.py; do not edit.\n'
        rust += f'pub(crate) const {constant}: [u8; 32] = [\n'
        for start in range(0, 32, 16):
            rust += '    ' + ', '.join(f'0x{b:02x}' for b in digest[start:start + 16]) + ',\n'
        rust += '];\n'
        outputs[f'crates/scriptw-intl/src/{source_dir}/kernel_identity.rs'] = rust
        outputs[f'crates/scriptw-intl/data/{data_dir}/kernel-manifest.json'] = json.dumps(
            dict(recipe, provider_data_sha256=digest.hex()), sort_keys=True, indent=2) + '\n'
    return outputs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository', type=Path, default=ROOT)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    outputs = generate(args.repository)
    for relative, content in outputs.items():
        path = args.repository / relative
        if args.check:
            if path.read_text() != content:
                raise ValueError(f'stale component identity: {relative}')
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
    print(json.dumps({json.loads(content)['service']: {'sha256': json.loads(content)['provider_data_sha256'],
                                                     'source_files': len(json.loads(content)['files'])}
                      for path, content in outputs.items() if path.endswith('.json')}, sort_keys=True))


if __name__ == '__main__':
    main()
