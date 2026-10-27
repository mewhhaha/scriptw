"""Adversarial inert controls for excluded ownership diagnostics."""
import copy
import struct
import unittest
from unittest.mock import MagicMock, patch

from scripts import benchmark_module_images as m

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


def owners(runtime=False, separate=False):
    value = dict(schema='scriptw-precompiled-module-images-v1', nonce=NONCE,
                 observation_index=16, owners=[dict(role='program',
                    image=dict(start=0x400000, end=0x401000),
                    text=dict(start=0x401020 if separate else 0x400020,
                              end=0x401040 if separate else 0x400040))])
    if runtime:
        value['owners'].append(dict(role='runtime', image=dict(start=0x401000, end=0x402000),
                                   text=dict(start=0x401020, end=0x401040)))
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


class ModuleImageControls(unittest.TestCase):
    def validate(self, value, shots=None, runtime='scriptc', identity=None):
        return m.validate_module_images(value, runtime, NONCE, 16,
                                        snapshots() if shots is None else shots,
                                        copy.deepcopy(IDENTITY) if identity is None else dict(copy.deepcopy(IDENTITY), **identity))

    def test_actual_independent_image_text_and_optional_runtime_roles(self):
        result = self.validate(owners(separate=True))
        self.assertEqual(result['owners'][0]['relation'], 'separate-text')
        self.assertEqual(result['owners'][0]['phases'][0]['union_present_pages'], 1)
        self.assertEqual(result['owners'][0]['phases'][0]['whole_vma_rss_bytes'], 8192)
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


class ElfControls(unittest.TestCase):
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
