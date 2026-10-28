"""Bounded, nonadditive page attribution for actual-owned Wasmtime images.

The producer serializes only after the fourth capture. Pinned Wasmtime 47
serializes exactly the buffer exposed by image_range(); no arbitrary ELF file
or host load-address translation is accepted here.
"""
import re

MAX_IMAGE_BYTES = 128 * 1024**2
MAX_SECTIONS = 4096
MAX_SECTION_NAME_BYTES = 2048
MAX_TOTAL_NAME_BYTES = 128 * 1024
MAX_PAGE_WORK = 8_000_000


def require(value, message):
    if not value:
        raise ValueError(message)


def uint(value, maximum=2**64-1):
    return type(value) is int and 0 <= value <= maximum


def span(value, image_bytes):
    require(isinstance(value, dict) and set(value) == {'offset', 'bytes'}, 'section span keys differ')
    offset, size = value['offset'], value['bytes']
    require(uint(offset) and uint(size) and size > 0 and offset <= image_bytes
            and size <= image_bytes-offset, 'section span is outside the serialized image')
    return offset, offset+size


def prepare(value, image, text, page, *, expected_native_sha256=None):
    """Validate once and prepare bounded page sets; never read native bytes."""
    require(isinstance(value, dict) and set(value) == {'schema', 'serialized_sha256', 'serialized_bytes',
            'elf_header', 'section_headers', 'sections'}, 'serialized section metadata keys differ')
    require(value['schema'] == 'scriptw-module-sections-v1', 'serialized section schema differs')
    digest = value['serialized_sha256']
    require(isinstance(digest, str) and re.fullmatch('[0-9a-f]{64}', digest) is not None,
            'invalid serialized image digest')
    require(expected_native_sha256 is None or digest == expected_native_sha256,
            'actual serialized image differs from the admitted native producer')
    image_bytes = value['serialized_bytes']
    require(uint(image_bytes, MAX_IMAGE_BYTES) and image_bytes == image[1]-image[0]
            and image_bytes >= 64, 'serialized image length differs from the actual owner')
    require(image[0] <= text[0] < text[1] <= image[1],
            'section attribution requires text inside its actual serialized image')
    header = span(value['elf_header'], image_bytes)
    table = span(value['section_headers'], image_bytes)
    require(header == (0, 64) and table[0] >= 64 and table[0] % 8 == 0
            and (table[1]-table[0]) % 64 == 0
            and 1 <= (table[1]-table[0])//64 <= MAX_SECTIONS, 'invalid ELF64 header/table extent')
    rows = value['sections']
    require(isinstance(rows, list) and 0 < len(rows) < MAX_SECTIONS
            and len(rows)+1 <= (table[1]-table[0])//64, 'section inventory exceeds the actual table')
    names, total_names, extents, normalized = set(), 0, [], []
    for row in rows:
        require(isinstance(row, dict) and set(row) == {'name', 'offset', 'bytes', 'flags', 'section_type'},
                'file-backed section keys differ')
        name = row['name']
        require(isinstance(name, str) and name and '\0' not in name and name not in names,
                'empty, duplicate or NUL-containing section name')
        name_bytes = len(name.encode('utf-8'))
        total_names += name_bytes
        require(name_bytes <= MAX_SECTION_NAME_BYTES and total_names <= MAX_TOTAL_NAME_BYTES,
                'section names exceed their byte budget')
        require(uint(row['flags']) and uint(row['section_type'], 2**32-1)
                and row['section_type'] not in (0, 8), 'section is not nonempty file-backed data')
        lo, hi = span({'offset': row['offset'], 'bytes': row['bytes']}, image_bytes)
        require(not (lo < header[1] and header[0] < hi)
                and not (lo < table[1] and table[0] < hi), 'section overlaps ELF structural metadata')
        names.add(name)
        extents.append((lo, hi))
        normalized.append(dict(row))
    ordered = sorted(extents)
    require(all(left[1] <= right[0] for left, right in zip(ordered, ordered[1:])),
            'file-backed section byte extents overlap')
    native_text = [row for row in normalized if row['name'] == '.text']
    require(len(native_text) == 1 and native_text[0]['section_type'] == 1
            and native_text[0]['flags'] & 4
            and image[0]+native_text[0]['offset'] == text[0]
            and native_text[0]['bytes'] == text[1]-text[0],
            'serialized text does not anchor the actual module text range')
    work = 0

    def pages(offset, size):
        nonlocal work
        lo, hi = image[0]+offset, image[0]+offset+size
        first, end = lo//page*page, (hi+page-1)//page*page
        work += (end-first)//page
        require(work <= MAX_PAGE_WORK, 'section/page work exceeds its budget')
        overlaps = set(range(first, end, page))
        full = set(range((lo+page-1)//page*page, hi//page*page, page))
        return overlaps, full

    sections = [(row, *pages(row['offset'], row['bytes'])) for row in normalized]
    header_pages, _ = pages(header[0], header[1]-header[0])
    table_pages, _ = pages(table[0], table[1]-table[0])
    image_pages, _ = pages(0, image_bytes)
    text_pages, _ = pages(text[0]-image[0], text[1]-text[0])
    counts, union, full_union = {}, set(), set()
    for _, overlaps, full in sections:
        union.update(overlaps)
        full_union.update(full)
        for address in overlaps:
            counts[address] = counts.get(address, 0)+1
    mixed = {address for address, count in counts.items() if count > 1}
    return dict(metadata=dict(value, sections=normalized), sections=sections, image_pages=image_pages,
                text_pages=text_pages, header_pages=header_pages, table_pages=table_pages,
                section_union=union, boundary_pages=union-full_union, mixed_pages=mixed)


def attribute(prepared, present):
    """Report overlaps and whole pages separately; never prorate process RSS."""
    return dict(
        interpretation='Actual serialized-image section/page overlaps; boundary pages may have multiple sections or padding. Overlap counts are nonadditive and do not prove execution or live heap ownership.',
        image_present_pages=len(present & prepared['image_pages']),
        nontext_image_present_pages=len((present & prepared['image_pages'])-prepared['text_pages']),
        section_union_present_pages=len(present & prepared['section_union']),
        mixed_section_present_pages=len(present & prepared['mixed_pages']),
        section_boundary_present_pages=len(present & prepared['boundary_pages']),
        uncovered_present_image_pages=len((present & prepared['image_pages'])-prepared['section_union']),
        elf_header_present_page_overlaps=len(present & prepared['header_pages']),
        section_headers_present_page_overlaps=len(present & prepared['table_pages']),
        sections=[dict(row, present_page_overlaps=len(present & overlaps),
                       full_present_pages=len(present & full)) for row, overlaps, full in prepared['sections']])
