"""Bounded inert ownership/page and ELF metadata inspection for paused diagnostics.

Addresses identify retained Modules, not executed functions. No memory contents,
PFNs, native loading, executable copies, or proportional RSS estimates are used.
"""
import hashlib
import os
from pathlib import Path
import platform
import re
import stat
import struct

MAX_ADDRESS = 2**64 - 1
MAX_PAGES = 131072  # 512 MiB at the usual 4-KiB base page size.
MAX_MAPPINGS = 4096
MAX_ELF = 512 * 1024**2
MAX_SYMBOLS = 200000
MAX_NAME_BYTES = 32 * 1024**2


def require(ok, message):
    if not ok:
        raise ValueError(message)


def integer(value):
    return type(value) is int and 0 <= value <= MAX_ADDRESS


def interval(value, name):
    require(isinstance(value, dict) and set(value) == {'start', 'end'}, name+' range keys differ')
    lo, hi = value['start'], value['end']
    require(integer(lo) and integer(hi) and 0 < lo < hi and hi-lo <= MAX_ELF, name+' range invalid/oversized')
    return lo, hi


def page_size(value):
    require(type(value) is int and 4096 <= value <= 65536 and value & (value-1) == 0, 'unsupported base-page size')
    return value


def page_interval(lo, hi, page):
    require(integer(lo) and integer(hi) and lo < hi and hi <= MAX_ADDRESS-(page-1), 'page rounding overflow')
    return lo//page*page, (hi+page-1)//page*page


def checked_rows(rows, page):
    require(isinstance(rows, list) and 0 < len(rows) <= MAX_MAPPINGS, 'mapping count bound exceeded')
    previous = 0
    for row in rows:
        lo, hi = row['start'], row['end']
        require(integer(lo) and integer(hi) and previous <= lo < hi and lo % page == hi % page == 0,
                'unordered/overlapping/unaligned mappings')
        require(isinstance(row.get('permissions'),str) and re.fullmatch('[r-][w-][x-][ps]',row['permissions']) is not None,'invalid mapping permissions')
        require(type(row.get('Rss')) is int and 0<=row['Rss']<=hi-lo,'invalid mapping RSS extent')
        previous = hi


def readable_presence(pid, rows, page_bytes=None):
    page = page_size(os.sysconf('SC_PAGE_SIZE') if page_bytes is None else page_bytes)
    checked_rows(rows, page)
    selected = [r for r in rows if r['permissions'].startswith('r')]
    count = sum((r['end']-r['start'])//page for r in selected)
    require(count <= MAX_PAGES, 'all-readable presence page bound exceeded')
    records = []
    with Path(f'/proc/{pid}/pagemap').open('rb', buffering=0) as stream:
        for row in selected:
            pages = (row['end']-row['start'])//page
            raw = os.pread(stream.fileno(), pages*8, row['start']//page*8)
            require(len(raw) == pages*8, 'short all-readable presence read')
            bits = bytearray((pages+7)//8)
            for n in range(pages):
                bits[n//8] |= ((raw[n*8+7] >> 7) & 1) << (n % 8)
            records.append(dict(start=row['start'], end=row['end'], pages=pages, present_bits_lsb_hex=bits.hex()))
    return dict(status='present-bits-only-no-PFNs', page_bytes=page, mappings=records)


def presence_pages(snapshot):
    value = snapshot['mapped_presence']
    require(set(value) == {'status', 'page_bytes', 'mappings'} and value['status'] == 'present-bits-only-no-PFNs',
            'all-readable presence unavailable or malformed')
    page = page_size(value['page_bytes'])
    rows = snapshot['mappings']; checked_rows(rows, page)
    expected = {(r['start'], r['end']) for r in rows if r['permissions'].startswith('r')}
    require(len(value['mappings']) == len(expected), 'all-readable presence coverage differs')
    actual, present, total = set(), set(), 0
    for record in value['mappings']:
        require(set(record) == {'start', 'end', 'pages', 'present_bits_lsb_hex'}, 'presence record keys differ')
        key = record['start'], record['end']; require(key in expected and key not in actual, 'duplicate/foreign presence mapping')
        actual.add(key); pages = (key[1]-key[0])//page
        require(type(record['pages']) is int and record['pages'] == pages, 'presence extent differs')
        total += pages; require(total <= MAX_PAGES, 'presence page count exceeds bound')
        require(isinstance(record['present_bits_lsb_hex'],str) and re.fullmatch('[0-9a-f]*',record['present_bits_lsb_hex']) is not None,'noncanonical presence bits')
        bits = bytes.fromhex(record['present_bits_lsb_hex'])
        require(len(bits) == (pages+7)//8 and (not pages % 8 or bits[-1] >> (pages % 8) == 0), 'presence bit extent/padding differs')
        present.update(key[0]+i*page for i in range(pages) if bits[i//8] & (1 << (i % 8)))
    require(actual == expected, 'missing readable presence mapping')
    return page, present


def covered(rows, lo, hi, executable=False):
    cursor = lo
    for row in rows:
        if row['end'] <= cursor: continue
        if row['start'] >= hi: break
        require(row['start'] <= cursor and row['permissions'].startswith('r'), 'module range has unmapped/unreadable bytes')
        require(not executable or 'x' in row['permissions'], 'module text lacks executable coverage')
        cursor = min(hi, row['end'])
        if cursor == hi: return
    raise ValueError('module range not fully covered')


def validate_module_images(value, runtime, nonce, observation_index, snapshots, identity):
    require(isinstance(value, dict) and set(value) == {'schema', 'nonce', 'observation_index', 'owners'}, 'module image keys differ')
    require(value['schema'] == 'scriptw-precompiled-module-images-v1' and value['nonce'] == nonce
            and type(value['observation_index']) is int and value['observation_index'] == observation_index,
            'module image schema/nonce/index differs')
    required_identity={'source_commit','source_fingerprint','executable_sha256','image_sha256','program_native_sha256','runtime_key','shared_configuration'}
    require(isinstance(identity,dict) and set(identity)==required_identity,'ownership identity fields differ')
    require(isinstance(identity['source_commit'],str) and re.fullmatch('[0-9a-f]{40}',identity['source_commit']) is not None,'ownership source commit invalid')
    require(all(isinstance(identity[k],str) and re.fullmatch('[0-9a-f]{64}',identity[k]) for k in ['source_fingerprint','executable_sha256','image_sha256','program_native_sha256']),'ownership digest identity invalid')
    require(identity['runtime_key'] is None or isinstance(identity['runtime_key'],str) and re.fullmatch('[0-9a-f]{64}',identity['runtime_key']),'ownership runtime key invalid')
    require(isinstance(identity['shared_configuration'],dict) and identity['shared_configuration'].get('consumer_compiler_enabled') is False,'ownership consumer policy differs')
    expected = {'program', 'runtime'} if runtime == 'scriptw' and identity.get('runtime_key') is not None else {'program'}
    require(runtime in ('scriptw', 'scriptc') and isinstance(value['owners'], list)
            and len(value['owners']) == len(expected), 'module owner count differs')
    require(len(snapshots) == 4 and [s['phase'] for s in snapshots] ==
            ['after_transport_drop', 'after_instantiate', 'after_execution', 'after_store_drop'], 'owner evidence needs all four phases')
    owners = {}; occupied = set(); page,_ = presence_pages(snapshots[0])
    for owner in value['owners']:
        require(isinstance(owner, dict) and set(owner) == {'role', 'image', 'text'}, 'owner keys differ')
        role = owner['role']; require(isinstance(role,str) and role in expected and role not in owners, 'duplicate/unknown owner role')
        image, text = interval(owner['image'], 'image'), interval(owner['text'], 'text')
        require(image[0] % page == 0, 'module image base is not page aligned')
        inside = image[0] <= text[0] < text[1] <= image[1]
        separate = text[1] <= image[0] or image[1] <= text[0]
        require(inside or separate, 'partially overlapping image/text ranges')
        views = {}
        for name, span in [('image', image), ('text', text)]:
            lo, hi = page_interval(*span, page)
            require((hi-lo)//page <= MAX_PAGES, 'module page bound exceeded')
            views[name] = set(range(lo, hi, page))
        union = views['image'] | views['text']
        require(len(union) <= MAX_PAGES and not occupied & union, 'ambiguous/shared owner pages')
        occupied.update(union);require(len(occupied)<=MAX_PAGES,'combined module page bound exceeded')
        owners[role] = dict(image=image, text=text, relation='text-inside-image' if inside else 'separate-text', views=views, union=union)
    result = dict(schema='scriptw-module-page-attribution-v1', identity=dict(identity), owners=[],
                  interpretation='PTE-present base pages and whole-VMA RSS only; function/page overlap is not execution. Views overlap within an owner and must not be summed.')
    for role, owner in owners.items():
        entry = dict(role=role, image=dict(zip(('start','end'), owner['image'])), text=dict(zip(('start','end'), owner['text'])), relation=owner['relation'], phases=[])
        for snapshot in snapshots:
            require(snapshot['sequence']==len(entry['phases'])+1,'snapshot sequence differs')
            require(snapshot['nonce'] == nonce and snapshot['observation_index'] == observation_index, 'snapshot ownership nonce/index differs')
            actual_page, present = presence_pages(snapshot); require(actual_page == page, 'base-page size changed across phases')
            rows = snapshot['mappings']; covered(rows, *owner['image']); covered(rows, *owner['text'], executable=True)
            whole, partial = [], []
            for row in rows:
                lo, hi = row['start'], row['end']
                if not row['permissions'].startswith('r') or not any(lo < span_hi and span_lo < hi for span_lo,span_hi in [page_interval(*owner['image'],page),page_interval(*owner['text'],page)]): continue
                # Containment, never proportional RSS attribution.
                full = (hi-lo)//page <= len(owner['union']) and all(p in owner['union'] for p in range(lo,hi,page))
                (whole if full else partial).append(row)
            entry['phases'].append(dict(phase=snapshot['phase'], page_bytes=page,
                image_present_pages=len(present & owner['views']['image']), text_present_pages=len(present & owner['views']['text']),
                union_present_pages=len(present & owner['union']), whole_vma_rss_bytes=sum(r['Rss'] for r in whole),
                partial_vma_count=len(partial), partial_vma_rss_bytes=None))
        result['owners'].append(entry)
    return result


def _extent(raw, offset, size):
    require(integer(offset) and integer(size) and offset <= len(raw) and size <= len(raw)-offset, 'ELF file extent invalid')
    return memoryview(raw)[offset:offset+size]


def parse_elf(raw):
    require(64 <= len(raw) <= MAX_ELF and raw[:7] == b'\x7fELF\x02\x01\x01', 'requires bounded ELF64 little-endian image')
    header = struct.unpack_from('<16sHHIQQQIHHHHHH',raw)
    _,kind,machine,version,entry,phoff,shoff,flags,ehsize,phsize,phnum,shsize,shnum,shstr = header
    require(kind in (2,3) and machine in (62,183,243) and version == 1 and ehsize == 64
            and phsize == 56 and 0 < phnum <= 256 and shsize == 64 and 0 < shnum <= 4096
            and 0 < shstr < shnum, 'unsupported ELF layout/machine/extended counts')
    _extent(raw,phoff,phnum*56); _extent(raw,shoff,shnum*64)
    loads=[]
    for n in range(phnum):
        typ,pflags,off,va,pa,filesz,memsz,align = struct.unpack_from('<IIQQQQQQ',raw,phoff+n*56)
        if typ != 1:continue
        _extent(raw,off,filesz)
        require(filesz <= memsz and va <= MAX_ADDRESS-memsz and (align in (0,1) or align & (align-1) == 0)
                and (align in (0,1) or off % align == va % align), 'invalid PT_LOAD extent/alignment')
        loads.append(dict(offset=off,vaddr=va,filesz=filesz,memsz=memsz,flags=pflags,align=align))
    require(loads,'ELF has no PT_LOAD')
    sections=[]
    for n in range(shnum):
        name,typ,sflags,va,off,size,link,info,align,entsize = struct.unpack_from('<IIQQQQIIQQ',raw,shoff+n*64)
        require(va <= MAX_ADDRESS-size,'ELF section virtual overflow')
        if typ != 8:_extent(raw,off,size)
        sections.append(dict(name_offset=name,type=typ,flags=sflags,vaddr=va,offset=off,size=size,link=link,entsize=entsize))
    names=sections[shstr];require(names['type']==3,'ELF section names missing')
    name_bytes=0
    def string(table,offset):
        nonlocal name_bytes
        require(0 <= offset < table['size'],'ELF string offset outside table')
        start=table['offset']+offset;stop=raw.find(b'\0',start,min(table['offset']+table['size'],start+2049))
        require(stop>=start,'unterminated/oversized ELF symbol name')
        name_bytes+=stop-start;require(name_bytes<=MAX_NAME_BYTES,'ELF symbol-name budget exceeded')
        return raw[start:stop].decode('utf-8',errors='strict')
    for section in sections:section['name']=string(names,section['name_offset'])
    symbols=[];zero=0;symbol_entries=0
    for table in sections:
        if table['type'] != 2:continue
        require(table['entsize']==24 and table['size']%24==0 and table['link']<len(sections),'invalid ELF symbol table')
        strings=sections[table['link']];require(strings['type']==3,'ELF symbols lack string table')
        symbol_entries+=table['size']//24;require(symbol_entries<=MAX_SYMBOLS,'ELF symbol-count bound exceeded')
        for offset in range(table['offset'],table['offset']+table['size'],24):
            name,info,other,section_index,va,size = struct.unpack_from('<IBBHQQ',raw,offset)
            if info & 15 not in (2,10) or section_index==0:continue
            require(section_index<len(sections),'unsupported ELF symbol section index')
            section=sections[section_index]
            if not section['flags'] & 4:continue
            require(section['type']==1 and section['flags']&2,'function section is not file-backed allocated executable data')
            if size==0:zero+=1;continue
            require(section['vaddr']<=va and size<=section['size'] and va-section['vaddr']<=section['size']-size,'function outside executable section')
            file_offset=section['offset']+va-section['vaddr'];_extent(raw,file_offset,size)
            backing=[load for load in loads if load['flags']&1 and load['offset']<=file_offset
                     and file_offset+size<=load['offset']+load['filesz'] and load['vaddr']+file_offset-load['offset']==va]
            require(len(backing)==1,'function lacks unique executable PT_LOAD translation')
            symbols.append(dict(name=string(strings,name),vaddr=va,bytes=size,file_offset=file_offset,section=section['name']))
    require(symbols,'ELF has no bounded defined function symbols')
    return dict(machine=machine,kind=kind,loads=loads,sections=sections,functions=symbols,zero_size_functions=zero)


def host_symbol_inventory(host):
    host=Path(host);before=host.stat()
    require(stat.S_ISREG(before.st_mode) and before.st_uid==os.getuid() and before.st_nlink==1 and before.st_size<=MAX_ELF,'host ELF is not bounded owned single-link file')
    raw=host.read_bytes();digest=hashlib.sha256(raw).hexdigest();elf=parse_elf(raw);del raw
    with host.open('rb') as stream:after_digest=hashlib.file_digest(stream,'sha256').hexdigest()
    after=host.stat();require((before.st_dev,before.st_ino,before.st_size)==(after.st_dev,after.st_ino,after.st_size) and digest==after_digest,'host ELF changed during inspection')
    expected={'x86_64':62,'aarch64':183,'riscv64':243}.get(platform.machine())
    require(elf['machine']==expected,'host ELF machine differs from live platform')
    return dict(schema='scriptw-host-elf-symbols-v1',sha256=digest,bytes=before.st_size,inode=before.st_ino,**elf)


def attribute_host(inventory,snapshots,host):
    result=dict(schema='scriptw-host-symbol-page-overlap-v1',host_sha256=inventory['sha256'],phases=[],symbols=[],
                interpretation='Nonexclusive PTE-present executable file-page overlaps. Shared pages/aliases overlap; symbol presence does not establish execution. Zero-size functions excluded.')
    by_phase={}
    for snap in snapshots:
        page,present=presence_pages(snap)
        rows=[r for r in snap['mappings'] if r['path']==str(host) and r['inode']==inventory['inode'] and r['permissions'].startswith('r')]
        require(rows and snap['proc_exe']['inode']==inventory['inode'],'snapshot host inode differs')
        biases=None
        for row in rows:
            candidates=set()
            for load in inventory['loads']:
                if not load['filesz'] or not load['flags']&4 or ('x'in row['permissions'] and not load['flags']&1) or ('w'in row['permissions'] and not load['flags']&2):continue
                flo=load['offset']//page*page;fhi=(load['offset']+load['filesz']+page-1)//page*page
                if flo<=row['file_offset'] and row['file_offset']+row['end']-row['start']<=fhi:
                    va=load['vaddr']//page*page+row['file_offset']-flo
                    if row['start']>=va:candidates.add(row['start']-va)
            require(candidates,'host mapping has no PT_LOAD-compatible load bias')
            biases=candidates if biases is None else biases&candidates
        require(len(biases)==1,'host load bias is ambiguous/inconsistent');bias=next(iter(biases))
        file_pages=set()
        for row in rows:
            if 'x'not in row['permissions']:continue
            for address in range(row['start'],row['end'],page):
                if address in present:file_pages.add(row['file_offset']+address-row['start'])
        by_phase[snap['phase']]=(page,file_pages)
        result['phases'].append(dict(phase=snap['phase'],load_bias=bias,present_host_rx_pages=len(file_pages),page_bytes=page))
    work=0
    for symbol in inventory['functions']:
        overlap={}
        for phase,(page,pages)in by_phase.items():
            lo,hi=page_interval(symbol['file_offset'],symbol['file_offset']+symbol['bytes'],page)
            work+=(hi-lo)//page;require(work<=8_000_000,'symbol/page overlap work bound exceeded')
            overlap[phase]=sum(address in pages for address in range(lo,hi,page))
        if any(overlap.values()):result['symbols'].append(dict(**symbol,present_page_overlaps=overlap))
    return result
