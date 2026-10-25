"""Mapping-accounting controls for Linux process memory attribution."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import process_memory_maps as maps


def mapping(start='1000', end='5000', *, path='', permissions='rw-p', rss=8,
            pss=6, shared_clean=4, shared_dirty=0, private_clean=0,
            private_dirty=4, anonymous=4, swap=4):
    size = (int(end, 16) - int(start, 16)) // 1024
    return (f'{start}-{end} {permissions} 00000000 00:00 0 {path}\n'
            f'Size: {size} kB\nKernelPageSize: 4 kB\nMMUPageSize: 4 kB\n'
            f'Rss: {rss} kB\nPss: {pss} kB\n'
            f'Shared_Clean: {shared_clean} kB\nShared_Dirty: {shared_dirty} kB\n'
            f'Private_Clean: {private_clean} kB\nPrivate_Dirty: {private_dirty} kB\n'
            f'Anonymous: {anonymous} kB\nSwap: {swap} kB\n'
            'THPeligible: 0\nVmFlags: rd wr mr mw me ac sd\n')


class ProcessMemoryMapsTests(unittest.TestCase):
    def test_named_and_anonymous_ranges_reconcile_without_becoming_a_js_heap(self):
        text = ''.join([
            mapping('1000', '5000', path='[heap]'),
            mapping('5000', '9000', path='[stack]'),
            mapping('9000', 'd000', permissions='r-xp'),
            mapping('d000', '11000', path='[anon:wasmtime]'),
            mapping('11000', '15000', path='/tmp/runtime image (deleted)', permissions='r-xp'),
        ])
        observed = maps.parse_smaps(text)
        self.assertEqual(observed['totals']['size_bytes'], 5 * 16 * 1024)
        self.assertEqual(observed['totals']['rss_bytes'], 5 * 8 * 1024)
        self.assertEqual(observed['totals']['pss_bytes'], 5 * 6 * 1024)
        self.assertEqual(observed['totals']['swap_bytes'], 5 * 4 * 1024)
        self.assertEqual([row['group'] for row in observed['mappings']], list(maps.GROUPS))
        self.assertEqual(observed['groups']['file-backed']['anonymous_bytes'], 4 * 1024)
        self.assertEqual(observed['mappings'][-1]['path'], '/tmp/runtime image (deleted)')
        for group in observed['groups'].values():
            self.assertEqual(group['mapping_count'], 1)
            self.assertEqual(group['rss_bytes'], 8 * 1024)

    def test_malformed_missing_duplicate_nonfinite_or_overflowed_counters_reject(self):
        original = mapping()
        invalid = [
            original.replace('Pss: 6 kB\n', ''),
            original.replace('Rss: 8 kB', 'Rss: 8 kB\nRss: 8 kB'),
            original.replace('Rss: 8 kB', 'Rss: -1 kB'),
            original.replace('Rss: 8 kB', 'Rss: nan kB'),
            original.replace('Rss: 8 kB', 'Rss: 8 bytes'),
            original.replace('Rss: 8 kB', 'Rss: 1.5 kB'),
            original.replace('Rss: 8 kB', f'Rss: {2**64} kB'),
            original.replace('Rss: 8 kB', f'Rss: {2**64 - 1} kB'),
            original.replace('Rss: 8 kB', 'Rss: 8 kB garbage'),
        ]
        for text in invalid:
            with self.subTest(text=text[:80]), self.assertRaises(ValueError):
                maps.parse_smaps(text)

    def test_address_and_residency_inconsistency_reject(self):
        invalid = [
            mapping().replace('Size: 16 kB', 'Size: 32 kB'),
            mapping(rss=20),
            mapping(pss=9),
            mapping(anonymous=9),
            mapping(private_dirty=3),
            mapping(swap=12),
            mapping().replace('1000-5000', '5000-1000'),
            mapping().replace('1000-5000', '1000-10000000000000000'),
            mapping() + mapping('4000', '8000'),
            mapping() + mapping('0000', '4000'),
            mapping().replace('rw-p', 'rw-q'),
        ]
        for text in invalid:
            with self.subTest(text=text[:80]), self.assertRaises(ValueError):
                maps.parse_smaps(text)

    def test_empty_and_oversized_snapshots_reject(self):
        for text in ('', '\n', 'x' * (maps.MAX_SMAPS_BYTES + 1)):
            with self.assertRaises(ValueError):
                maps.parse_smaps(text)

    def test_grouping_does_not_confuse_executable_files_with_anonymous_code(self):
        observed = maps.parse_smaps(mapping(path='/memfd:wasmtime-code (deleted)', permissions='r-xp'))
        self.assertEqual(observed['mappings'][0]['group'], 'file-backed')
        self.assertEqual(observed['groups']['anon-exec']['mapping_count'], 0)
        observed = maps.parse_smaps(mapping(path='[vdso]', permissions='r-xp'))
        self.assertEqual(observed['mappings'][0]['group'], 'anon-exec')
        self.assertEqual(observed['totals']['anonymous_bytes'], 4 * 1024)

    def test_reader_rejects_pid_paths_before_access_and_bounds_file_reads(self):
        for pid in ('../self', '123', '/etc/passwd', 0, -1, True, 2**31):
            with self.subTest(pid=pid), patch.object(Path, 'open') as opened:
                with self.assertRaises(ValueError):
                    maps.read_smaps(pid)
                opened.assert_not_called()
        with patch.object(Path, 'open') as opened:
            opened.return_value.__enter__.return_value.read.return_value = b'x' * (maps.MAX_SMAPS_BYTES + 1)
            with self.assertRaisesRegex(ValueError, 'reading budget'):
                maps.read_smaps(123)
            opened.return_value.__enter__.return_value.read.assert_called_once_with(maps.MAX_SMAPS_BYTES + 1)


if __name__ == '__main__':
    unittest.main()
