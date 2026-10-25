"""Bounded Linux smaps snapshots for an explicitly selected process.

Residency is process-wide. Mapping labels and permissions classify address
ranges; neither anonymous mappings nor the Anonymous counter identify a JS heap.
No process discovery, memory mutation, collection or benchmark runs occur here.
"""
import os
from pathlib import Path
import re


MAX_SMAPS_BYTES = 8 * 1024**2
MAX_U64 = 2**64 - 1
COUNTERS = {
    'Size': 'size_bytes',
    'Rss': 'rss_bytes',
    'Pss': 'pss_bytes',
    'Shared_Clean': 'shared_clean_bytes',
    'Shared_Dirty': 'shared_dirty_bytes',
    'Private_Clean': 'private_clean_bytes',
    'Private_Dirty': 'private_dirty_bytes',
    'Anonymous': 'anonymous_bytes',
    'Swap': 'swap_bytes',
}
GROUPS = ('[heap]', '[stack]', 'anon-exec', 'anon', 'file-backed')
HEADER = re.compile(
    r'([0-9a-fA-F]+)-([0-9a-fA-F]+)\s+([r-][w-][x-][ps])\s+'
    r'([0-9a-fA-F]+)\s+([0-9a-fA-F]+:[0-9a-fA-F]+)\s+([0-9]+)'
    r'(?:[ \t]+(.*))?'
)
PROPERTY = re.compile(r'([A-Za-z_][A-Za-z0-9_]*):\s*(.*)')
KILOBYTES = re.compile(r'([0-9]+)\s+kB')


def _require(condition, message):
    if not condition:
        raise ValueError('smaps: ' + message)


def _number(value, base, role):
    number = int(value, base)
    _require(0 <= number <= MAX_U64, role + ' exceeds unsigned 64-bit bounds')
    return number


def _group(path, permissions):
    if path == '[heap]':
        return '[heap]'
    if path == '[stack]' or re.fullmatch(r'\[stack:[0-9]+\]', path):
        return '[stack]'
    if path and not path.startswith('['):
        return 'file-backed'
    return 'anon-exec' if permissions[2] == 'x' else 'anon'


def _validate(mapping):
    missing = set(COUNTERS.values()) - mapping.keys()
    _require(not missing, 'mapping misses counters: ' + ', '.join(sorted(missing)))
    size = mapping['size_bytes']
    rss = mapping['rss_bytes']
    _require(size == mapping['end'] - mapping['start'], 'Size differs from the address range')
    _require(rss <= size and mapping['swap_bytes'] <= size - rss,
             'resident/swapped bytes exceed the mapping size')
    _require(mapping['pss_bytes'] <= rss and mapping['anonymous_bytes'] <= rss,
             'Pss or Anonymous exceeds Rss')
    parts = sum(mapping[key] for key in ('shared_clean_bytes', 'shared_dirty_bytes',
                                         'private_clean_bytes', 'private_dirty_bytes'))
    _require(parts == rss, 'shared/private residency does not reconcile to Rss')


def _empty_totals():
    return {field: 0 for field in COUNTERS.values()}


def parse_smaps(text):
    """Parse required counters, rejecting partial/inconsistent mapping records.

    Unknown kernel metadata such as VmFlags is left uninterpreted. Every
    accepted metric is a bounded nonnegative integer number of bytes.
    """
    _require(isinstance(text, str), 'input must be text')
    _require(len(text) <= MAX_SMAPS_BYTES
             and len(text.encode('utf-8', errors='surrogateescape')) <= MAX_SMAPS_BYTES,
             'input exceeds the 8-MiB reading budget')
    mappings = []
    current = None
    for line_number, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        header = HEADER.fullmatch(line)
        if header:
            if current is not None:
                _validate(current)
                mappings.append(current)
            start, end, permissions, offset, device, inode, path = header.groups()
            start = _number(start, 16, 'mapping start')
            end = _number(end, 16, 'mapping end')
            _require(start < end, 'mapping address range is empty or reversed')
            _require(not mappings or start >= mappings[-1]['end'],
                     'mapping address ranges overlap or are out of order')
            path = path or ''
            current = dict(start=start, end=end, permissions=permissions,
                           offset=_number(offset, 16, 'mapping offset'), device=device,
                           inode=_number(inode, 10, 'mapping inode'), path=path,
                           group=_group(path, permissions))
            continue
        property_match = PROPERTY.fullmatch(line)
        _require(current is not None and property_match is not None,
                 f'invalid mapping/header property at line {line_number}')
        name, value = property_match.groups()
        if name not in COUNTERS:
            continue
        field = COUNTERS[name]
        _require(field not in current, 'duplicate counter: ' + name)
        kilobytes = KILOBYTES.fullmatch(value)
        _require(kilobytes is not None, 'counter must be unsigned integer kB: ' + name)
        amount = _number(kilobytes[1], 10, name)
        _require(amount <= MAX_U64 // 1024, 'counter byte conversion overflows: ' + name)
        current[field] = amount * 1024
    _require(current is not None, 'snapshot has no mappings')
    _validate(current)
    mappings.append(current)
    totals = _empty_totals()
    groups = {name: dict(mapping_count=0, **_empty_totals()) for name in GROUPS}
    for mapping in mappings:
        group = groups[mapping['group']]
        group['mapping_count'] += 1
        for field in COUNTERS.values():
            totals[field] += mapping[field]
            group[field] += mapping[field]
            _require(totals[field] <= MAX_U64, 'aggregate byte accounting overflows: ' + field)
    return dict(scope='linux-process-smaps-residency', totals=totals,
                groups=groups, mappings=mappings)


def _pid(pid):
    if pid == 'self':
        return os.getpid()
    _require(type(pid) is int and 0 < pid <= 2**31 - 1,
             'PID must be a positive integer or self')
    return pid


def read_smaps(pid='self'):
    """Return at most 8 MiB of smaps text for only the requested process."""
    selected = _pid(pid)
    with (Path('/proc') / str(selected) / 'smaps').open('rb') as stream:
        data = stream.read(MAX_SMAPS_BYTES + 1)
    _require(len(data) <= MAX_SMAPS_BYTES, 'input exceeds the 8-MiB reading budget')
    return data.decode('utf-8', errors='surrogateescape')


def read_snapshot(pid='self'):
    """Read and parse one process; callers may retain read_smaps output instead."""
    selected = _pid(pid)
    return dict(pid=selected, **parse_smaps(read_smaps(selected)))
