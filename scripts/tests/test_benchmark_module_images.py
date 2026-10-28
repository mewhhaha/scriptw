"""Adversarial inert controls for excluded ownership diagnostics."""
import copy
import struct
import unittest
from unittest.mock import MagicMock, patch

from scripts import benchmark_module_images as m
from scripts import benchmark_module_sections as sections

PHASES = ['after_transport_drop', 'after_instantiate', 'after_execution', 'after_store_drop']
NONCE = 'a' * 32
IDENTITY = dict(source_commit='a'*40, source_fingerprint='b'*64, executable_sha256='c'*64,
                image_sha256='d'*64, program_native_sha256='e'*64, runtime_key=None,
                shared_configuration=dict(consumer_compiler_enabled=False))


def row(start, end, permissions='r-xp'):
    return dict(start=start, end=end, permissions=permissions, Rss=end-start,
                path='/owned/consumer', inode=17, file_offset=start-0x400000)


def snapshots(rows=None):
    rows = rows or [row(0x400000, 0x402000)]
    return [dict(phase=phase, sequence=index+1, nonce=NONCE, observation_index=16,
                 proc_exe=dict(inode=17), mappings=copy.deepcopy(rows),
                 mapped_presence=dict(status='present-bits-only-no-PFNs', page_bytes=4096,
                     mappings=[dict(start=r['start'], end=r['end'], pages=(r['end']-r['start'])//4096,
                                    present_bits_lsb_hex='01') for r in rows if r['permissions'].startswith('r')]))
            for index, phase in enumerate(PHASES)]


def section_metadata():
    return dict(schema='scriptw-module-sections-v1', serialized_sha256='e'*64, serialized_bytes=4096,
                elf_header=dict(offset=0, bytes=64), section_headers=dict(offset=0xe00, bytes=256),
                sections=[dict(name='.text', offset=0x100, bytes=32, flags=6, section_type=1),
                          dict(name='.wasmtime.info', offset=0x400, bytes=512, flags=2, section_type=1),
                          dict(name='.shstrtab', offset=0x800, bytes=128, flags=0, section_type=3)])


def owners(runtime=False, separate=False):
    value = dict(schema='scriptw-precompiled-module-images-v2', nonce=NONCE,
                 observation_index=16, owners=[dict(role='program',
                    image=dict(start=0x400000, end=0x401000),
                    text=dict(start=0x401100 if separate else 0x400100,
                              end=0x401120 if separate else 0x400120), sections=section_metadata())])
    if runtime:
        value['owners'].append(dict(role='runtime', image=dict(start=0x401000, end=0x402000),
                                   text=dict(start=0x401100, end=0x401120), sections=section_metadata()))
    return value


def elf():
    """Small real-layout ELF64 with one file-backed executable function."""
    raw = bytearray(0x500)
    ident = b'\x7fELF\x02\x01\x01' + b'\0' * 9
    struct.pack_into('<16sHHIQQQIHHHHHH', raw, 0, ident, 3, 62, 1,
                     0x400100, 64, 0x300, 0, 64, 56, 1, 64, 5, 2)
    struct.pack_into('<IIQQQQQQ', raw, 64, 1, 5, 0, 0x400000, 0, len(raw), len(raw), 4096)
    names = b'\0.text\0.shstrtab\0.strtab\0.symtab\0'
    raw[0x180:0x180+len(names)] = names
    raw[0x200:0x205] = b'\0foo\0'
    # Null entry then one global STT_FUNC, plus a separately testable zero-size alias.
    struct.pack_into('<IBBHQQ', raw, 0x258, 1, 0x12, 0, 1, 0x400100, 16)
    struct.pack_into('<IBBHQQ', raw, 0x270, 1, 0x12, 0, 1, 0x400110, 0)
    sections = [
        (0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
        (1, 1, 6, 0x400100, 0x100, 32, 0, 0, 16, 0),
        (7, 3, 0, 0, 0x180, len(names), 0, 0, 1, 0),
        (17, 3, 0, 0, 0x200, 5, 0, 0, 1, 0),
        (25, 2, 0, 0, 0x240, 72, 3, 1, 8, 24),
    ]
    for i, fields in enumerate(sections):
        struct.pack_into('<IIQQQQIIQQ', raw, 0x300+i*64, *fields)
    return raw


def elf_with_function_name(name, terminated=True):
    raw = elf()
    offset = len(raw)
    strings = b'\0' + name + (b'\0' if terminated else b'')
    raw.extend(strings)
    # Relocate the linked .strtab without changing code, loads or symbols.
    struct.pack_into('<Q', raw, 0x300+3*64+24, offset)
    struct.pack_into('<Q', raw, 0x300+3*64+32, len(strings))
    return raw


class ModuleImageControls(unittest.TestCase):
    def validate(self, value, shots=None, runtime='scriptc', identity=None):
        return m.validate_module_images(value, runtime, NONCE, 16,
                                        snapshots() if shots is None else shots,
                                        copy.deepcopy(IDENTITY) if identity is None else dict(copy.deepcopy(IDENTITY), **identity))

    def test_actual_serialized_text_anchor_and_optional_runtime_roles(self):
        result = self.validate(owners(), snapshots([row(0x400000, 0x401000)]))
        self.assertEqual(result['owners'][0]['relation'], 'text-inside-image')
        self.assertEqual(result['owners'][0]['phases'][0]['union_present_pages'], 1)
        self.assertEqual(result['owners'][0]['phases'][0]['whole_vma_rss_bytes'], 4096)
        with self.assertRaisesRegex(ValueError, 'text inside'):
            self.validate(owners(separate=True))
        partial = self.validate(owners())['owners'][0]['phases'][0]
        self.assertEqual(partial['whole_vma_rss_bytes'], 0)
        self.assertEqual(partial['partial_vma_count'], 1)
        self.assertIsNone(partial['partial_vma_rss_bytes'])
        result = self.validate(owners(runtime=True), runtime='scriptw', identity={'runtime_key': 'f'*64})
        self.assertEqual([o['role'] for o in result['owners']], ['program', 'runtime'])
        # Without R in actual identity, a Scriptw P-only module remains admissible.
        self.assertEqual(len(self.validate(owners(), runtime='scriptw')['owners']), 1)

    def test_closed_owner_nonce_index_domain_and_overlap(self):
        cases = []
        for key, bad in [('nonce', 'b'*32), ('observation_index', True), ('schema', 'future')]:
            x = owners(); x[key] = bad; cases.append(x)
        x = owners(); x['extra'] = 1; cases.append(x)
        x = owners(); x['owners'][0]['role'] = 'host'; cases.append(x)
        x = owners(runtime=True); x['owners'][1]['role'] = 'program'; cases.append(x)
        x = owners(); x['owners'][0]['text'] = dict(start=0x400fff, end=0x401001); cases.append(x)
        for x in cases:
            with self.subTest(value=x), self.assertRaises(ValueError): self.validate(x)
        x = owners(runtime=True); x['owners'][1]['image'] = dict(start=0x400000, end=0x402000)
        with self.assertRaises(ValueError): self.validate(x, runtime='scriptw', identity={'runtime_key': 'f'*64})

    def test_ranges_require_complete_readable_executable_phase_coverage(self):
        for span in [dict(start=0, end=1), dict(start=True, end=3),
                     dict(start=0x400000, end=0x400000),
                     dict(start=0x400000, end=m.MAX_ADDRESS)]:
            value = owners(); value['owners'][0]['image'] = span
            with self.subTest(span=span), self.assertRaises(ValueError): self.validate(value)
        for change in ['missing-phase', 'phase-order', 'nonce', 'index', 'sequence', 'unreadable', 'not-executable', 'gap']:
            shots = snapshots()
            if change == 'missing-phase': shots.pop()
            elif change == 'phase-order': shots.reverse()
            elif change == 'nonce': shots[2]['nonce'] = 'b'*32
            elif change == 'index': shots[2]['observation_index'] = 15
            elif change == 'sequence': shots[2]['sequence'] = 4
            elif change == 'unreadable': shots[1]['mappings'][0]['permissions'] = '---p'
            elif change == 'not-executable': shots[1]['mappings'][0]['permissions'] = 'r--p'
            else: shots = snapshots([row(0x401000, 0x402000)])
            with self.subTest(change=change), self.assertRaises(ValueError): self.validate(owners(), shots)

    def test_source_and_consumer_identity_domains_are_closed(self):
        for key, value in [('source_commit', 'bad'), ('source_commit', 1), ('source_fingerprint', 'a'*63),
                           ('executable_sha256', 1), ('image_sha256', 'A'*64),
                           ('program_native_sha256', None), ('runtime_key', ''),
                           ('shared_configuration', {'consumer_compiler_enabled': True})]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.validate(owners(), identity={key: value})
        with self.assertRaises(ValueError):
            self.validate(owners(), identity={'extra': 'field'})

    def test_presence_exact_coverage_padding_and_no_pfn_leak(self):
        for change in ['missing', 'duplicate', 'foreign', 'pages', 'padding', 'extra']:
            shot = snapshots()[0]; entries = shot['mapped_presence']['mappings']
            if change == 'missing': entries.clear()
            elif change == 'duplicate': entries.append(copy.deepcopy(entries[0]))
            elif change == 'foreign': entries[0]['start'] += 4096
            elif change == 'pages': entries[0]['pages'] = True
            elif change == 'padding': entries[0]['present_bits_lsb_hex'] = '81'
            else: entries[0]['pfn'] = 123
            with self.subTest(change=change), self.assertRaises(ValueError): m.presence_pages(shot)
        # Bit63 alone determines presence, even if all lower PFN/flag bits are set.
        with patch.object(m.Path, 'open', return_value=MagicMock()), patch.object(m.os, 'pread', return_value=struct.pack('<QQ', 2**64-1, 2**63-1)):
            value = m.readable_presence(123, [row(0x400000, 0x402000)], 4096)
        self.assertEqual(value['mappings'][0]['present_bits_lsb_hex'], '01')
        self.assertEqual(set(value['mappings'][0]), {'start', 'end', 'pages', 'present_bits_lsb_hex'})


class ModuleSectionControls(unittest.TestCase):
    def prepare(self, value=None, image=(0x400000, 0x401000), text=(0x400100, 0x400120)):
        return sections.prepare(section_metadata() if value is None else value, image, text, 4096,
                                expected_native_sha256='e'*64)

    def test_shared_boundaries_are_nonadditive_and_not_whole_page_rss(self):
        result = sections.attribute(self.prepare(), {0x400000})
        self.assertEqual(result['image_present_pages'], 1)
        self.assertEqual(result['section_union_present_pages'], 1)
        self.assertEqual(result['mixed_section_present_pages'], 1)
        self.assertEqual(result['section_boundary_present_pages'], 1)
        self.assertEqual(result['elf_header_present_page_overlaps'], 1)
        self.assertEqual(result['section_headers_present_page_overlaps'], 1)
        self.assertEqual([r['present_page_overlaps'] for r in result['sections']], [1, 1, 1])
        self.assertEqual([r['full_present_pages'] for r in result['sections']], [0, 0, 0])
        self.assertNotIn('rss_bytes', result)

    def test_nontext_whole_pages_and_uncovered_headers_or_padding_remain_distinct(self):
        value = section_metadata()
        value.update(serialized_bytes=0x5000, section_headers=dict(offset=0x4e00, bytes=256))
        value['sections'][0].update(offset=0x1000, bytes=4096)
        value['sections'][1].update(offset=0x2000, bytes=4096)
        value['sections'][2].update(offset=0x3000, bytes=128)
        prepared = self.prepare(value, (0x400000, 0x405000), (0x401000, 0x402000))
        result = sections.attribute(prepared, {0x400000, 0x401000, 0x402000, 0x404000})
        self.assertEqual(result['image_present_pages'], 4)
        self.assertEqual(result['nontext_image_present_pages'], 3)
        self.assertEqual(result['uncovered_present_image_pages'], 2)
        self.assertEqual(result['mixed_section_present_pages'], 0)
        self.assertEqual([r['full_present_pages'] for r in result['sections']], [1, 1, 0])

    def test_serialized_identity_length_and_actual_text_anchor_cannot_be_substituted(self):
        for field, bad in [('serialized_sha256', 'f'*64), ('serialized_sha256', 'E'*64),
                           ('serialized_bytes', True), ('serialized_bytes', 4095),
                           ('serialized_bytes', sections.MAX_IMAGE_BYTES+1), ('schema', 'future')]:
            value = section_metadata(); value[field] = bad
            with self.subTest(field=field, value=bad), self.assertRaises(ValueError): self.prepare(value)
        for field, bad in [('offset', 0x101), ('bytes', 31), ('flags', 2), ('section_type', 3)]:
            value = section_metadata(); value['sections'][0][field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError): self.prepare(value)

    def test_section_extents_flags_inventory_and_structural_overlap_are_bounded(self):
        cases = []
        for field, bad in [('offset', 2**64-1), ('bytes', 2**64-1), ('bytes', 0),
                           ('flags', -1), ('flags', True), ('section_type', 8), ('section_type', 0)]:
            value = section_metadata(); value['sections'][1][field] = bad; cases.append(value)
        for offset in (0, 0x100, 0xe00):
            value = section_metadata(); value['sections'][1].update(offset=offset, bytes=16); cases.append(value)
        for table in (dict(offset=63, bytes=256), dict(offset=0xe01, bytes=256),
                      dict(offset=0xe00, bytes=65), dict(offset=0xe00, bytes=64),
                      dict(offset=0xff0, bytes=256)):
            value = section_metadata(); value['section_headers'] = table; cases.append(value)
        value = section_metadata(); value['sections'] *= 2000; cases.append(value)
        value = section_metadata(); value['sections'][0]['extra'] = True; cases.append(value)
        for value in cases:
            with self.subTest(value=value), self.assertRaises(ValueError): self.prepare(value)

    def test_section_names_keep_encoding_uniqueness_and_aggregate_limits(self):
        for name in ('', '.text', 'bad\0name', '\ud800', 'n'*(sections.MAX_SECTION_NAME_BYTES+1)):
            value = section_metadata(); value['sections'][1]['name'] = name
            with self.subTest(name=repr(name)), self.assertRaises(ValueError): self.prepare(value)
        value = section_metadata(); value['sections'][1]['name'] = 'n'*sections.MAX_SECTION_NAME_BYTES
        self.prepare(value)
        with patch.object(sections, 'MAX_TOTAL_NAME_BYTES', 8), self.assertRaisesRegex(ValueError, 'byte budget'):
            self.prepare()
        with patch.object(sections, 'MAX_PAGE_WORK', 2), self.assertRaisesRegex(ValueError, 'work exceeds'):
            self.prepare()

    def test_owner_attribution_rejects_missing_sections_and_wrong_program_digest(self):
        for change in ('missing', 'old-schema', 'digest', 'header'):
            value = owners()
            if change == 'missing': del value['owners'][0]['sections']
            elif change == 'old-schema': value['schema'] = 'scriptw-precompiled-module-images-v1'
            elif change == 'digest': value['owners'][0]['sections']['serialized_sha256'] = 'f'*64
            else: value['owners'][0]['sections']['elf_header']['bytes'] = 63
            with self.subTest(change=change), self.assertRaises(ValueError):
                m.validate_module_images(value, 'scriptc', NONCE, 16, snapshots(), copy.deepcopy(IDENTITY))


class ElfControls(unittest.TestCase):
    def test_long_defined_function_names_admit_measured_host_extent_and_exact_bound(self):
        # The actual Rust host has a 133,748-byte defined executable symbol.
        # Synthetic names preserve that extent without retaining host ELF bytes.
        for length in (133748, m.MAX_SYMBOL_NAME_BYTES):
            with self.subTest(length=length):
                name = b'R' * length
                parsed = m.parse_elf(elf_with_function_name(name))
                self.assertEqual(parsed['functions'][0]['name'], name.decode())

    def test_symbol_names_keep_size_termination_encoding_and_aggregate_limits(self):
        cases = [(b'R' * (m.MAX_SYMBOL_NAME_BYTES+1), True, 'oversized ELF symbol name'),
                 (b'R' * 133748, False, 'unterminated/oversized ELF symbol name'),
                 (b'R\xff', True, 'utf-8')]
        for name, terminated, error in cases:
            with self.subTest(length=len(name), terminated=terminated), self.assertRaisesRegex(ValueError, error):
                m.parse_elf(elf_with_function_name(name, terminated))
        with patch.object(m, 'MAX_NAME_BYTES', 64), self.assertRaisesRegex(ValueError, 'name budget exceeded'):
            m.parse_elf(elf_with_function_name(b'R'*65))

    def test_section_names_retain_their_separate_short_bound(self):
        for length in (m.MAX_SECTION_NAME_BYTES, m.MAX_SECTION_NAME_BYTES+1):
            raw = elf()
            offset = len(raw)
            raw.extend(b'\0'+b'S'*length+b'\0')
            struct.pack_into('<Q', raw, 0x300+2*64+24, offset)
            struct.pack_into('<Q', raw, 0x300+2*64+32, length+2)
            # All non-null sections can share this synthetic section name.
            for index in range(1, 5):
                struct.pack_into('<I', raw, 0x300+index*64, 1)
            if length == m.MAX_SECTION_NAME_BYTES:
                self.assertEqual(m.parse_elf(raw)['functions'][0]['section'], 'S'*length)
            else:
                with self.assertRaisesRegex(ValueError, 'oversized ELF section name'):
                    m.parse_elf(raw)

    def test_file_backed_function_bounds_and_zero_size_exclusion(self):
        parsed = m.parse_elf(elf())
        self.assertEqual(parsed['functions'], [dict(name='foo', vaddr=0x400100, bytes=16,
                                                   file_offset=0x100, section='.text')])
        self.assertEqual(parsed['zero_size_functions'], 1)

    def test_adversarial_elf_headers_sections_loads_and_symbols(self):
        changes = [('class',4,'B',1), ('endian',5,'B',2), ('machine',18,'H',0),
                   ('phoff',32,'Q',m.MAX_ADDRESS), ('shoff',40,'Q',m.MAX_ADDRESS),
                   ('filesz',64+32,'Q',0x501), ('memsz',64+40,'Q',1),
                   ('align',64+48,'Q',3), ('section-type',0x340+4,'I',8),
                   ('symbol-link',0x400+40,'I',9), ('symbol-ent',0x400+56,'Q',16),
                   ('symbol-section',0x258+6,'H',65535), ('symbol-start',0x258+8,'Q',0x4000ff),
                   ('symbol-size',0x258+16,'Q',33), ('symbol-name',0x258,'I',5)]
        for name, offset, fmt, bad in changes:
            raw = elf(); struct.pack_into('<'+fmt, raw, offset, bad)
            with self.subTest(name=name), self.assertRaises(ValueError): m.parse_elf(raw)
        raw = elf(); struct.pack_into('<H', raw, 56, 2)
        raw[120:176] = raw[64:120]  # Same function has two PT_LOAD translations.
        with self.assertRaises(ValueError): m.parse_elf(raw)

    def test_shared_function_page_overlap_is_nonexclusive_not_rss(self):
        inventory = dict(m.parse_elf(elf()), sha256='source-bound', inode=17)
        inventory['functions'].append(dict(inventory['functions'][0], name='alias'))
        shot = snapshots([row(0x400000, 0x401000)])[0]
        result = m.attribute_host(inventory, [shot], '/owned/consumer')
        self.assertEqual(result['phases'][0]['load_bias'], 0)
        self.assertEqual([s['present_page_overlaps'][PHASES[0]] for s in result['symbols']], [1, 1])
        self.assertNotIn('rss_bytes', result['symbols'][0])
        bad = copy.deepcopy(shot); bad['mappings'][0]['file_offset'] = 4096
        with self.assertRaises(ValueError): m.attribute_host(inventory, [bad], '/owned/consumer')


if __name__ == '__main__':
    unittest.main()
