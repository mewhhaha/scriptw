#!/usr/bin/env python3
"""Verify pinned ICU list sources and bind the consumed lossless list kernel."""
import argparse
import ast
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
DATA = Path('crates/scriptw-intl/data/list-icu-2')
OWNER = Path('crates/scriptw-intl/src/list_format')
PACKAGES = {'icu_list': '2.0.1', 'icu_list_data': '2.0.0', 'writeable': '0.6.3',
            'icu_locale': '2.0.0', 'icu_locale_data': '2.0.0', 'icu_locale_core': '2.0.1',
            'icu_provider': '2.0.0', 'icu_provider_blob': '2.0.0',
            'icu_provider_adapters': '2.0.0', 'zerovec': '0.11.6'}
BYTE_STRING = r'b"(?:\\.|[^"\\])*"'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def regular_members(raw):
    result = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:gz') as archive:
        for member in archive:
            if member.isdir():
                continue
            path = Path(member.name)
            if not member.isfile() or path.is_absolute() or '..' in path.parts or member.name in result:
                raise ValueError(f'invalid pinned archive member: {member.name}')
            result[member.name] = archive.extractfile(member).read()
    return result


def unpack(repository):
    inputs_raw = (repository / DATA / 'source-inputs.json').read_bytes()
    inputs = json.loads(inputs_raw)
    if inputs['schema_version'] != 1 or inputs['archive'] != 'sources.tar.gz':
        raise ValueError('unknown ListFormat source archive schema')
    raw = (repository / DATA / inputs['archive']).read_bytes()
    if len(raw) != inputs['archive_bytes'] or sha(raw) != inputs['archive_sha256']:
        raise ValueError('ListFormat source archive identity mismatch')
    members = regular_members(raw)
    expected = {row['path']: row for row in inputs['files']}
    if len(expected) != len(inputs['files']) or members.keys() != expected.keys():
        raise ValueError('ListFormat source archive census mismatch')
    for name, content in members.items():
        row = expected[name]
        if len(content) != row['bytes'] or sha(content) != row['sha256']:
            raise ValueError(f'ListFormat source member differs: {name}')
    sources = dict(members)
    for package in inputs['packages']:
        if package['runtime_dependency']:
            content = members[package['archive_member']]
            if sha(content) != package['crate_checksum']:
                raise ValueError('pinned runtime crate checksum differs')
            sources.update(regular_members(content))
    return inputs, sources


def census(sources):
    rows = []
    distinct = set()
    for marker in ('and', 'or', 'unit'):
        name = f'icu_list_data-2.0.0/data/list_{marker}_v1.rs.data'
        text = sources[name].decode()
        joiners = re.findall(r'ListJoinerPattern::from_parts\(unsafe \{ zerovec::VarZeroCow::from_bytes_unchecked\((' + BYTE_STRING + r')\) \}, (\d+)u8\)', text)
        starts = re.findall(r'start: icu::list::provider::ListJoinerPattern::from_parts\(unsafe \{ zerovec::VarZeroCow::from_bytes_unchecked\((' + BYTE_STRING + r')\) \}, (\d+)u8\)', text)
        pools = text.count('icu::list::provider::ListFormatterPatterns {')
        if not pools or len(starts) != pools or not joiners:
            raise ValueError('incomplete pinned list template census')
        for raw, offset in joiners:
            content = ast.literal_eval(raw)
            content.decode('utf-8')
            index = int(offset)
            if index > len(content):
                raise ValueError('pinned list literal boundary exceeds its string')
            content[:index].decode('utf-8')
            content[index:].decode('utf-8')
        if any(int(offset) != len(ast.literal_eval(raw)) for raw, offset in starts):
            raise ValueError('ICU start suffix would be omitted by the list renderer')
        pairs = re.findall(r'from_dfa_bytes_unchecked\(if cfg!\(target_endian = "little"\) \{ (' + BYTE_STRING + r') \} else \{ (' + BYTE_STRING + r') \}\)', text)
        conditions = []
        if len(pairs) != text.count('special_case: Some('):
            raise ValueError('conditional list pattern census is incomplete')
        for little, big in pairs:
            little, big = ast.literal_eval(little), ast.literal_eval(big)
            if len(little) != len(big):
                raise ValueError('conditional DFA endian lengths differ')
            pair = (sha(little), sha(big))
            distinct.add(pair)
            conditions.append({'little_endian_sha256': pair[0], 'big_endian_sha256': pair[1], 'bytes': len(little)})
        rows.append({'marker': marker, 'source_sha256': sha(sources[name]),
                     'pooled_patterns': pools, 'joiner_patterns': len(joiners),
                     'start_suffixes_empty': True, 'joiner_utf8_boundaries_valid': True,
                     'conditional_occurrences': conditions})
    return {'rows': rows, 'distinct_condition_dfa_pairs': sorted(distinct),
            'actual_available_locale_count': 'only the native constructor/runtime controls establish this count'}


def generate(repository):
    inputs, sources = unpack(repository)
    lock = tomllib.loads((repository / 'Cargo.lock').read_text())
    packages = []
    for name, version in sorted(PACKAGES.items()):
        candidates = [row for row in lock['package'] if row['name'] == name]
        if len(candidates) != 1 or candidates[0]['version'] != version:
            raise ValueError(f'unexpected pinned list dependency: {name}')
        row = candidates[0]
        if row.get('source') != 'registry+https://github.com/rust-lang/crates.io-index' or len(row.get('checksum', '')) != 64:
            raise ValueError('pinned list dependency lacks registry identity')
        packages.append({key: row[key] for key in ('name', 'version', 'source', 'checksum')})
    for row in inputs['packages']:
        if row['runtime_dependency']:
            matched = next(p for p in packages if p['name'] == row['name'])
            if matched['version'] != row['version'] or matched['checksum'] != row['crate_checksum']:
                raise ValueError('source archive is not the selected runtime crate')
    paths = {DATA / 'source-inputs.json', Path('scripts/generate-intl-list-identity.py'),
             Path('crates/scriptw-intl/Cargo.toml'), Path('crates/scriptw-intl/src/list_format.rs'),
             Path('crates/scriptw-intl/src/list_protocol.rs'), Path('crates/scriptw-intl/src/lib.rs'),
             Path('crates/scriptw-intl/src/artifact_identity.rs'),
             Path('crates/scriptw-intl/src/protocol.rs'), Path('crates/scriptw-intl/src/provider.rs'), Path('crates/scriptw-intl/src/provider/conformance.rs'), Path('crates/scriptw-intl/src/selection.rs'), Path('crates/scriptw-intl/src/selection/minimal_admission.rs'),
             Path('crates/scriptw-intl/src/service_selection.rs'),
             Path('crates/scriptw-intl/src/selection/manifest.rs'),
             Path('crates/scriptw-intl/src/selection/export.rs'),
             Path('crates/scriptw-intl/src/number_protocol.rs'), Path('crates/scriptw-intl/src/number_protocol/primitives.rs'),
             Path('crates/scriptw-engine/src/intl_list_host.rs')}
    paths.update(Path(name) for name in ['crates/scriptw-intl/build.rs', 'crates/scriptw-intl/src/image.rs', 'crates/scriptw-intl/src/locale_image.rs', 'crates/scriptw-intl/src/image_build/locale.rs', 'crates/scriptw-engine/src/intl_data_images.rs', 'crates/scriptw-intl/src/list_image.rs', 'crates/scriptw-intl/src/image_build/list.rs'])
    paths.update(Path(name) for name in ['crates/scriptw-intl/src/image_build/keyword.rs', 'crates/scriptw-intl/src/provider/keyword_aliases.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs', 'crates/scriptw-intl/data/cldr-47-bcp47/manifest.json', 'crates/scriptw-intl/src/provider/language_domain.rs'])
    paths.update(Path(name) for name in ['crates/scriptw-aot-wasm/src/emit.rs', 'crates/scriptw-aot-wasm/src/emit/module_assembly.rs'])
    paths.update(Path(name) for name in [
        'crates/scriptw-intl/src/list_image/projection.rs',
        'crates/scriptw-intl/src/list_image/projection/export.rs',
        'crates/scriptw-intl/src/duration_format/profiles.rs',
        'crates/scriptw-intl/src/duration_format/raw.rs',
        'crates/scriptw-intl/src/duration_format/generated/profile.json'])
    paths.update(p.relative_to(repository) for p in (repository / OWNER).glob('*.rs') if p.name not in ('identity.rs', 'tests.rs'))
    records = []
    for path in sorted(paths):
        raw = (repository / path).read_bytes()
        records.append({'path': path.as_posix(), 'sha256': sha(raw), 'bytes': len(raw)})
    recipe = {'schema_version': 1, 'purpose': 'pinned ICU list templates, conditional DFA and exact UTF16/indexed parts kernel',
              'packages': packages, 'source_archive_sha256': inputs['archive_sha256'],
              'files': records, 'pattern_census': census(sources),
              'conditional_projection': 'valid UTF16 pairs decode normally; isolated surrogates become U+FFFD only for ICU template selection; output uses indexed original strings'}
    digest = hashlib.sha256(json.dumps(recipe, sort_keys=True, separators=(',', ':')).encode()).digest()
    rust = '// Generated by scripts/generate-intl-list-identity.py; do not edit.\n'
    rust += 'pub(crate) const LIST_FORMAT_DATA_SHA256: [u8; 32] = [\n'
    for start in range(0, 32, 16):
        rust += '    ' + ', '.join(f'0x{byte:02x}' for byte in digest[start:start+16]) + ',\n'
    rust += '];\n'
    return rust, json.dumps({**recipe, 'provider_data_sha256': digest.hex()}, indent=2, sort_keys=True) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository', type=Path, default=ROOT)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    rust, manifest = generate(args.repository)
    for path, content in ((OWNER / 'identity.rs', rust), (DATA / 'manifest.json', manifest)):
        target = args.repository / path
        if args.check:
            if target.read_text() != content:
                raise ValueError(f'stale ListFormat identity: {path}')
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content)
    receipt = json.loads(manifest)
    print(json.dumps({'provider_data_sha256': receipt['provider_data_sha256'], 'source_files': len(receipt['files']), 'pattern_census': receipt['pattern_census']}))


if __name__ == '__main__':
    main()
