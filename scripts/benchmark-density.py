#!/usr/bin/env python3
"""Process density, startup and runtime: Scriptw against Node/V8, with Porffor native as a peer.

  prepare  identify the tools, write the six-program corpus (a print-only startup
           program and five kernels of the Porffor panel), build Scriptw native
           images and Porffor native executables untimed, and run every actor once
           with checked output. Failures are retained and never measured.
  run      (1) density: N concurrent processes per actor/program are held at an
           equivalent post-execution point and every process's
           /proc/<pid>/smaps_rollup is read while all N are alive;
           (2) process wall: fresh sequential processes, spawn to exit;
           (3) steady state: per-entry wall in fresh Stores/contexts through the
           Porffor panel's harnesses (Scriptw against Node/V8 only).
  publish  write the checked public summary without host paths.
  tables   print the Markdown tables of a published summary.

Hold points. Scriptw: the shared precompiled host's phase barrier after the
second execution (one warmup, the host's minimum), its Store and output still
live. Node: a preload (scripts/density_node_hold.cjs) blocks at `beforeExit`,
after the program, with the isolate and heap live. Porffor native: a ptrace
exit stop (PTRACE_O_TRACEEXIT), before the address space is torn down; its C
runtime frees nothing before exit. Only the readings are taken while held.

Capacity. Summed PSS over N held processes counts every resident page once
(shared pages split between their mappers), so total(N) = fixed + N x marginal.
marginal = (total(Nmax) - total(1)) / (Nmax - 1), fixed = total(1) - marginal,
processes in M bytes = floor((M - fixed) / marginal). RSS is never used: it
counts shared executable pages once per process.
"""
import argparse
import ctypes
from datetime import datetime, timezone
import importlib.util
import json
import math
import os
from pathlib import Path
import secrets
import selectors
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import time

from benchmark_metrics import digest, measure_process
from compiler_comparison import ADAPTER

ROOT = Path(__file__).resolve().parents[1]


def _module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# Shared helpers and pins of the Porffor runtime panel: tool identity, harnesses, checked entries.
wasm_peer = _module('wasm_peer_benchmark', 'scripts/benchmark-porffor.py')

SCHEMA = 'scriptw-process-density-v1'
FIXTURES = wasm_peer.FIXTURES
HOLD_HARNESS = ROOT / 'scripts/density_node_hold.cjs'
PROGRAMS = ('startup.js', 'numeric-loop-50000.js', 'sort-reverse-512.js', 'dynamic-string-4096.js',
            'objects.js', 'retained-objects.js')
NODE_VERSIONS = dict(node=wasm_peer.NODE_VERSION, node22='22.23.4')
# Memory knobs a deployer packing Node processes would set; the execution tiers stay on.
LEAN_FLAGS = ('--v8-pool-size=0', '--single-threaded-gc', '--max-semi-space-size=1')
PEER = 'porffor.native'
DENSITY_ACTORS = ('scriptw', 'node', 'node.lean', 'node22', PEER)
WALL_ACTORS = DENSITY_ACTORS + ('scriptw.run',)
STEADY_ACTORS = ('scriptw', 'node', 'node.lean')
LABELS = {'scriptw': 'Scriptw (precompiled image, shared host)', 'scriptw.run': 'Scriptw CLI `run` (warm program cache)',
          'node': f'Node {NODE_VERSIONS["node"]}', 'node.lean': f'Node {NODE_VERSIONS["node"]} lean flags',
          'node22': f'Node {NODE_VERSIONS["node22"]}', PEER: 'Porffor 0.61.13 native (gcc -O3)'}
SHORT = {'scriptw': 'Scriptw', 'scriptw.run': 'Scriptw run', 'node': 'Node 24', 'node.lean': 'Node 24 lean',
         'node22': 'Node 22', PEER: 'Porffor native'}
COUNTS, GROUPS = (1, 8, 32), 2
WALL_PROCESSES = 10
STEADY_PROCESSES = 4  # two per block, the second block reversed
SAMPLES, WARMUPS = wasm_peer.SAMPLES, wasm_peer.WARMUPS
HOST_SAMPLES, HOST_WARMUPS, HOST_HOLD_INDEX = 3, 1, 1  # the host's minimum; hold the first measured entry
GIB = 1 << 30
RESERVE_BYTES = 6 * GIB
HOLD_DEADLINE_S = 25  # below the host's 30 s barrier timeout
FIELDS = ('rss', 'pss', 'uss', 'private_dirty', 'pss_anon', 'threads')
POLICY = dict(
    density=('N processes of one actor/program start together and are held at an equivalent post-execution point; '
             'each held process reads /proc/<pid>/smaps_rollup once, then all are released and their output is checked. '
             'Scriptw: shared precompiled host, phase barrier after its second execution (one warmup) with the Store live. '
             'Node: the adapted source with a beforeExit preload. Porffor: its native executable at a ptrace exit stop.'),
    capacity=('Summed PSS of N held processes is the resident memory they use together. marginal = (total(Nmax) - total(1)) '
              '/ (Nmax - 1); fixed = total(1) - marginal; processes in M bytes = floor((M - fixed) / marginal). The 16 GiB '
              'figure gives all 16 GiB to these processes: no kernel, page-cache or OS reserve. USS (private clean + dirty) at '
              'Nmax is reported beside it.'),
    wall=('Fresh sequential processes under GNU time, spawn to exit, actor order rotated per round; output checked. Scriptw '
          'runs the host with a build-source report identity: it executes the program four times (3 samples + 1 warmup, '
          'its minimum) and does not hash its own 200 MB executable, which this driver hashes before and after the run. '
          'The host identity command, which does hash it, is timed separately. Scriptw run is the product CLI on a warm '
          'program cache: it reads the source and loads its cached native code instead of compiling it.'),
    steady=('Per-entry wall in fresh Stores/contexts through the Porffor panel harnesses: 21 measured entries after 10 '
            'checked warmups, four processes per actor/program in two blocks (second reversed); median of process medians.'),
    source=('Scriptw runs the original bytes with its print. Node and Porffor run the same print-to-console.log adapter '
            'the Porffor panel uses; the steady-state V8 harness provides print in its context instead.'),
    node_flags=('Node default and lean (' + ' '.join(LEAN_FLAGS) + '): no V8 platform worker threads, single-threaded GC, '
                '1 MiB semi-spaces. NODE_OPTIONS, compile cache and snapshots are unset; Scriptw runs its defaults.'))

libc = ctypes.CDLL(None, use_errno=True)
libc.ptrace.restype = ctypes.c_long
libc.ptrace.argtypes = (ctypes.c_long, ctypes.c_long, ctypes.c_void_p, ctypes.c_void_p)
PTRACE_TRACEME, PTRACE_CONT, PTRACE_SETOPTIONS = 0, 7, 0x4200
PTRACE_O_TRACEEXIT, PTRACE_O_EXITKILL, PTRACE_EVENT_EXIT = 0x40, 0x100000, 6

save, load, stamp = wasm_peer.save, wasm_peer.load, wasm_peer.stamp


def require(condition, message):
    if not condition:
        raise ValueError('process density: ' + message)


def parse_rollup(text):
    """Bytes of one /proc/<pid>/smaps_rollup; USS is private clean plus private dirty."""
    values = {}
    for line in text.splitlines()[1:]:
        key, _, rest = line.partition(':')
        fields = rest.split()
        require(len(fields) == 2 and fields[1] == 'kB' and fields[0].isdecimal(), 'unexpected smaps_rollup line: ' + line)
        values[key.strip()] = int(fields[0]) * 1024
    required = ('Rss', 'Pss', 'Pss_Anon', 'Private_Clean', 'Private_Dirty', 'Swap')
    require(all(key in values for key in required), 'smaps_rollup lacks ' + ', '.join(k for k in required if k not in values))
    require(values['Swap'] == 0, 'a held process has swapped pages')
    return dict(rss=values['Rss'], pss=values['Pss'], uss=values['Private_Clean'] + values['Private_Dirty'],
                private_dirty=values['Private_Dirty'], pss_anon=values['Pss_Anon'])


def read_process(pid):
    sample = parse_rollup(Path(f'/proc/{pid}/smaps_rollup').read_text())
    sample['threads'] = len(os.listdir(f'/proc/{pid}/task'))
    return [sample[field] for field in FIELDS]


def mem_available():
    for line in Path('/proc/meminfo').read_text().splitlines():
        if line.startswith('MemAvailable:'):
            return int(line.split()[1]) * 1024
    raise ValueError('density: /proc/meminfo has no MemAvailable')


def admission(count, rss_estimate, available, reserve=RESERVE_BYTES):
    """Admit N processes only when even unshared RSS (x1.25, +64 MiB) leaves the reserve free."""
    need = int(count * rss_estimate * 1.25) + 64 * 1024**2
    return dict(admitted=available - need >= reserve, need_bytes=need, available_bytes=available, reserve_bytes=reserve)


def capacity(totals):
    """Linear capacity from {N: median summed PSS}; None when it cannot be derived."""
    counts = sorted(totals)
    if len(counts) < 2 or counts[0] != 1:
        return None
    high = counts[-1]
    marginal = (totals[high] - totals[1]) / (high - 1)
    if marginal <= 0:
        return None
    fixed = totals[1] - marginal
    result = dict(marginal_bytes=marginal, fixed_bytes=fixed, from_counts=[1, high],
                  per_gib=math.floor((GIB - fixed) / marginal), per_16_gib=math.floor((16 * GIB - fixed) / marginal))
    if len(counts) > 2:
        middle = counts[1]
        result['marginal_check_bytes'] = dict(count=middle, bytes=(totals[middle] - totals[1]) / (middle - 1))
    return result


def rotate(actors, offset):
    offset %= len(actors)
    return list(actors[offset:]) + list(actors[:offset])


def density_plan(programs, counts, groups, admitted):
    result = []
    for program in programs:
        for count in counts:
            for group in range(groups):
                for actor in rotate(DENSITY_ACTORS, group + counts.index(count)):
                    if actor in admitted[program]:
                        result.append(dict(program=program, count=count, group=group, actor=actor))
    return result


def steady_plan(programs, admitted):
    result = []
    for block in range(2):
        ordered = list(enumerate(programs))
        if block:
            ordered.reverse()
        for index, program in ordered:
            for process in range(STEADY_PROCESSES // 2):
                actors = rotate(STEADY_ACTORS, index + process)
                for actor in (actors[::-1] if block else actors):
                    if actor in admitted[program]:
                        result.append(dict(block=block, program=program, actor=actor))
    return result


# ----------------------------------------------------------------------------- tools and corpus

def node_identity(path, env, version):
    path = Path(path).resolve(strict=True)
    versions = json.loads(subprocess.check_output([path, '-p', 'JSON.stringify(process.versions)'], env=env, text=True))
    require(versions['node'] == version, f'Node {version} is required at {path}')
    return dict(wasm_peer.identity(path), versions=dict(node=versions['node'], v8=versions['v8']))


def tool_identities(args, env):
    tools = args.tools.resolve(strict=True)
    entry = wasm_peer.peer_entry(tools)
    node = node_identity(args.node, env, NODE_VERSIONS['node'])
    reported = subprocess.check_output([node['path'], entry, '--version'], env=env, text=True, cwd=tools).strip()
    require(reported == wasm_peer.VERSION, 'the peer compiler reports another version: ' + reported)
    lock = tools / 'package-lock.json'
    require(wasm_peer.load(lock)['packages'].get('node_modules/' + wasm_peer.PACKAGE, {}).get('version') == wasm_peer.VERSION,
            'the lock file does not pin the peer compiler')
    host = args.host.resolve(strict=True)
    host_identity = json.loads(subprocess.check_output([host, 'identity'], env=env, text=True, timeout=60))
    require(host_identity['executable_sha256'] == digest(host), 'host identity does not describe its bytes')
    scriptw = args.scriptw.resolve(strict=True)
    compiler = json.loads(subprocess.check_output([scriptw, 'compiler-identity'], env=env, text=True, timeout=60))
    require(compiler['executable_sha256'] == digest(scriptw)
            and compiler['source_fingerprint'] == host_identity['source_fingerprint'], 'CLI and host differ in source')
    cc = Path(shutil.which('cc') or 'cc').resolve(strict=True)
    return dict(node=node, node22=node_identity(args.node22, env, NODE_VERSIONS['node22']),
                peer=dict(package=wasm_peer.PACKAGE, version=wasm_peer.VERSION, entry=wasm_peer.identity(entry),
                          lock=wasm_peer.identity(lock), target='native', optimization='default (-O1), C at -O3'),
                cc=dict(wasm_peer.identity(cc), version=subprocess.check_output([cc, '--version'], text=True).splitlines()[0]),
                host=dict(wasm_peer.identity(host), identity=host_identity),
                scriptw=dict(wasm_peer.identity(scriptw), identity=compiler),
                time=wasm_peer.identity(args.time.resolve(strict=True)),
                harnesses={path.name: wasm_peer.identity(path) for path in (HOLD_HARNESS, wasm_peer.V8_HARNESS)})


def corpus():
    manifest = load(FIXTURES / 'manifest.json')
    rows = {row['file']: row['stdout'] for row in manifest['execution']}
    rows[manifest['startup']['file']] = manifest['startup']['stdout']
    require(all(name in rows for name in PROGRAMS), 'a selected program is missing from the manifest')
    return {name: rows[name] for name in PROGRAMS}


def write_fixtures(folder, expected):
    """The selected programs as one producer/harness fixture directory (startup.js as an execution row)."""
    folder.mkdir()
    for name in PROGRAMS:
        shutil.copyfile(FIXTURES / name, folder / name)
    save(folder / 'manifest.json', dict(schema='scriptw-v8-workloads-v1',
                                        execution=[dict(file=name, stdout=expected[name]) for name in PROGRAMS]))


def wall_argv(actor, prepared, root, program):
    tools, record = prepared['tools'], prepared['programs'][program]
    if actor == 'scriptw':
        return [tools['host']['path'], 'execute-scriptw', str(root / 'images' / record['scriptw_image']), record['stdout'],
                str(HOST_SAMPLES), str(HOST_WARMUPS)]
    if actor == 'scriptw.run':
        return [tools['scriptw']['path'], 'run', str(root / 'fixtures' / program)]
    if actor == PEER:
        return [str(root / 'native' / (program + '.bin'))]
    node = tools['node22' if actor == 'node22' else 'node']['path']
    return [node, *(LEAN_FLAGS if actor == 'node.lean' else ()), str(root / 'adapted' / program)]


def check_wall_output(actor, stdout, expected):
    """Program output of one wall process, or a failure detail."""
    if actor == 'scriptw':
        try:
            report = json.loads(stdout)
            wasm_peer.entry_times(report, 'scriptw', expected, HOST_SAMPLES, HOST_WARMUPS)
        except (ValueError, KeyError) as error:
            return None, str(error)[:600]
        totals = [row['phases_ns']['total'] for row in report['observations']]
        return dict(load_ns=report['load_phases_ns']['process_total'], first_entry_ns=totals[0],
                    extra_entries_ns=sum(totals[1:])), ''
    text = stdout.decode(errors='replace')
    if actor == 'scriptw.run':
        head, _, tail = text.partition(expected)
        if head == '' and tail.startswith('run outcome: ') and tail.count('\n') == 1:
            return {}, ''
    elif text == expected:
        return {}, ''
    return None, 'printed ' + json.dumps(text[:200]) + ', expected ' + json.dumps(expected)


def run_wall(argv, folder, stem, env, time_binary):
    with (folder / (stem + '.stdout')).open('xb') as stdout, (folder / (stem + '.stderr')).open('xb') as stderr:
        resources = measure_process(argv, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, timeout=300, time_binary=time_binary)
    output = (folder / (stem + '.stdout')).read_bytes()
    return resources, output


def prepare(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('tmp', 'cache', 'adapted', 'native', 'logs', 'checks'):
        (output / name).mkdir()
    machine = wasm_peer.scope(args.cloud, 2)
    env = wasm_peer.environment(output)
    tools = tool_identities(args, env)
    expected = corpus()
    write_fixtures(output / 'fixtures', expected)
    prepared = dict(schema=SCHEMA + '-prepared', status='incomplete', started_at=stamp(), machine=machine, tools=tools,
                    adapter=ADAPTER, programs={})
    resources, producer = wasm_peer.invoke([tools['host']['path'], 'prepare-scriptw', output / 'fixtures', output / 'images'],
                                           output / 'logs', 'scriptw-producer', env, args.time)
    require(resources['exit_code'] == 0 and producer['source_fingerprint'] == tools['host']['identity']['source_fingerprint'],
            'Scriptw producer failed')
    rows = {row['file']: row for row in producer['cases']}
    for name in PROGRAMS:
        row = rows[name]
        require(row['source_sha256'] == digest(FIXTURES / name) and row['stdout'] == expected[name], 'producer corpus differs')
        require(digest(output / 'images' / row['image']) == row['image_sha256'], 'Scriptw image differs')
        adapted = output / 'adapted' / name
        adapted.write_bytes(ADAPTER.encode() + (FIXTURES / name).read_bytes())
        binary = output / 'native' / (name + '.bin')
        resources, _ = wasm_peer.invoke([tools['node']['path'], tools['peer']['entry']['path'], 'native', adapted, binary],
                                        output / 'logs', 'native-' + name, env, args.time, cwd=output / 'native')
        compiled = dict(status='compiled', detail='', elapsed_ns=resources['elapsed_ns'])
        if resources['exit_code'] != 0 or not binary.exists():
            log = (output / 'logs' / f'native-{name}.stderr').read_text(errors='replace').strip()
            compiled.update(status='compile-failed', detail=log[-600:] or f"exit status {resources['exit_code']}")
        else:
            compiled.update(bytes=binary.stat().st_size, sha256=digest(binary))
        prepared['programs'][name] = dict(stdout=expected[name], source_sha256=row['source_sha256'],
                                          adapted_sha256=digest(adapted), scriptw_image=row['image'],
                                          scriptw_image_sha256=row['image_sha256'], scriptw_image_bytes=row['image_bytes'],
                                          scriptw_program_wasm_bytes=row['program_wasm_bytes'], native=compiled, attempts={})
    # Every actor once, output checked; the run warms the Scriptw CLI cache.
    for name in PROGRAMS:
        record = prepared['programs'][name]
        for actor in WALL_ACTORS:
            if actor == PEER and record['native']['status'] != 'compiled':
                record['attempts'][actor] = dict(status='compile-failed', detail=record['native']['detail'])
                continue
            resources, stdout = run_wall(wall_argv(actor, prepared, output, name), output / 'checks', f'{actor}-{name}', env, args.time)
            extra, detail = check_wall_output(actor, stdout, record['stdout'])
            status = 'passed' if resources['exit_code'] == 0 and extra is not None else (
                'wrong-output' if resources['exit_code'] == 0 else 'runtime-failed')
            if status == 'runtime-failed':
                detail = f"exit status {resources['exit_code']}: " + (output / 'checks' / f'{actor}-{name}.stderr').read_text(
                    errors='replace').strip()[-400:] + ' ' + detail
            record['attempts'][actor] = dict(status=status, detail=detail, peak_rss_bytes=resources['peak_rss_bytes'])
            print(name, actor, status, flush=True)
    prepared.update(status='complete', finished_at=stamp())
    shutil.rmtree(output / 'tmp')
    save(output / 'prepared.json', prepared)
    print(output / 'prepared.json')


def inputs(prepared, root):
    tools = prepared['tools']
    paths = [tools[key]['path'] for key in ('node', 'node22', 'host', 'scriptw', 'time', 'cc')]
    paths += [row['path'] for row in tools['harnesses'].values()]
    for name, record in prepared['programs'].items():
        paths += [root / 'images' / record['scriptw_image'], root / 'adapted' / name, root / 'fixtures' / name]
        if record['native']['status'] == 'compiled':
            paths.append(root / 'native' / (name + '.bin'))
    return {str(path): digest(path) for path in sorted(set(map(str, paths)))}


def checked_inputs(prepared, root):
    current = inputs(prepared, root)
    for key in ('node', 'node22', 'host', 'scriptw', 'time', 'cc'):
        require(current[prepared['tools'][key]['path']] == prepared['tools'][key]['sha256'], key + ' changed since prepare')
    for name, record in prepared['programs'].items():
        require(current[str(root / 'images' / record['scriptw_image'])] == record['scriptw_image_sha256'], 'image changed')
        require(current[str(root / 'adapted' / name)] == record['adapted_sha256'], 'adapted source changed')
        if record['native']['status'] == 'compiled':
            require(current[str(root / 'native' / (name + '.bin'))] == record['native']['sha256'], 'native executable changed')
    return current


# ----------------------------------------------------------------------------- held processes

def start_barrier(argv, env, stdout, stderr, scriptw):
    """A child that announces SWPHASE1 lines on a dedicated socket and waits for each SWACK1."""
    parent, child = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
    nonce = secrets.token_hex(16)
    if scriptw:
        extra = dict(SCRIPTW_PRECOMPILED_PHASE_FD=str(child.fileno()), SCRIPTW_PRECOMPILED_PHASE_NONCE=nonce,
                     SCRIPTW_PRECOMPILED_PHASE_INDEX=str(HOST_HOLD_INDEX))
    else:
        extra = dict(SCRIPTW_DENSITY_HOLD_FD=str(child.fileno()), SCRIPTW_DENSITY_HOLD_NONCE=nonce)
    process = subprocess.Popen(argv, cwd=ROOT, env=dict(env, **extra), pass_fds=[child.fileno()], stdout=stdout, stderr=stderr)
    child.close()
    return dict(kind='barrier', pid=process.pid, process=process, socket=parent, nonce=nonce, buffer=b'', held=None, closed=False)


def start_traced(argv, env, stdout, stderr):
    """A child stopped by ptrace at its exit, before its address space is released."""
    pid = os.fork()
    if pid == 0:  # pragma: no cover - the exec child
        try:
            os.dup2(stdout.fileno(), 1)
            os.dup2(stderr.fileno(), 2)
            os.chdir(ROOT)
            if libc.ptrace(PTRACE_TRACEME, 0, None, None) == 0:
                os.execve(argv[0], argv, env)
        finally:
            os._exit(127)
    _, status = os.waitpid(pid, 0)
    require(os.WIFSTOPPED(status) and os.WSTOPSIG(status) == signal.SIGTRAP, 'traced child did not stop at exec')
    require(libc.ptrace(PTRACE_SETOPTIONS, pid, None, ctypes.c_void_p(PTRACE_O_TRACEEXIT | PTRACE_O_EXITKILL)) == 0,
            'PTRACE_SETOPTIONS failed: ' + os.strerror(ctypes.get_errno()))
    require(libc.ptrace(PTRACE_CONT, pid, None, None) == 0, 'PTRACE_CONT failed')
    return dict(kind='traced', pid=pid, held=None, exit_code=None)


def serve_barriers(members, release):
    """Acknowledge every phase but after_execution (held) until all are held, or with release until EOF."""
    selector = selectors.DefaultSelector()
    for member in members:
        if not member['closed']:
            selector.register(member['socket'], selectors.EVENT_READ, member)
    deadline = time.monotonic() + HOLD_DEADLINE_S
    try:
        while selector.get_map():
            if not release and all(member['held'] for member in members):
                return
            require(time.monotonic() < deadline, 'held processes did not reach their hold point in time')
            for key, _ in selector.select(timeout=0.5):
                member = key.data
                data = member['socket'].recv(4096)
                if not data:
                    require(release, 'a process ended before its hold point')
                    selector.unregister(member['socket'])
                    member['closed'] = True
                    continue
                member['buffer'] += data
                while b'\n' in member['buffer']:
                    line, member['buffer'] = member['buffer'].split(b'\n', 1)
                    text = line.decode('ascii')
                    require(text.startswith('SWPHASE1 ' + member['nonce'] + ' '), 'unexpected barrier line: ' + text[:80])
                    fields = text[len('SWPHASE1 '):] + '\n'
                    if fields.split()[2] == 'after_execution' and not release:
                        member['held'] = fields
                    else:
                        member['socket'].sendall(('SWACK1 ' + fields).encode())
    finally:
        selector.close()


def wait_traced(members, release):
    deadline = time.monotonic() + HOLD_DEADLINE_S
    pending = [member for member in members if (member['exit_code'] is None if release else not member['held'])]
    while pending:
        require(time.monotonic() < deadline, 'traced processes did not reach their exit stop in time')
        progressed = False
        for member in list(pending):
            pid, status = os.waitpid(member['pid'], os.WNOHANG)
            if pid == 0:
                continue
            progressed = True
            if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                require(release, 'a traced process ended without an exit stop')
                member['exit_code'] = os.waitstatus_to_exitcode(status)
                pending.remove(member)
            elif os.WSTOPSIG(status) == signal.SIGTRAP and status >> 16 == PTRACE_EVENT_EXIT:
                member['held'] = True
                pending.remove(member)
            else:
                libc.ptrace(PTRACE_CONT, member['pid'], None, ctypes.c_void_p(os.WSTOPSIG(status)))
        if not progressed:
            time.sleep(0.001)


def release_all(members):
    barriers = [member for member in members if member['kind'] == 'barrier']
    for member in barriers:
        if member['held']:
            member['socket'].sendall(('SWACK1 ' + member['held']).encode())
    if barriers:
        serve_barriers(barriers, release=True)
        for member in barriers:
            member['exit_code'] = member['process'].wait(timeout=60)
            member['socket'].close()
    traced = [member for member in members if member['kind'] == 'traced']
    for member in traced:
        libc.ptrace(PTRACE_CONT, member['pid'], None, None)
    wait_traced(traced, release=True)


def kill_all(members):
    """Kill only children that have not been reaped, so a recycled pid is never signalled."""
    for member in members:
        if member['kind'] == 'barrier':
            if member['process'].poll() is None:
                member['process'].kill()
            member['process'].wait()
            member['socket'].close()
        elif member['exit_code'] is None:
            os.kill(member['pid'], signal.SIGKILL)
            try:
                while True:
                    _, status = os.waitpid(member['pid'], 0)
                    if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                        break
            except ChildProcessError:
                pass


def density_group(entry, prepared, root, folder, env):
    """One concurrent group: N held processes of one actor/program, read together, then released and checked."""
    actor, program, count = entry['actor'], entry['program'], entry['count']
    record = prepared['programs'][program]
    argv = wall_argv(actor, prepared, root, program)
    if actor.startswith('node'):
        argv = [argv[0], *argv[1:-1], '--require', str(HOLD_HARNESS), argv[-1]]
    members, streams = [], []
    stem = f"{program}-{actor}-{count}-{entry['group']}"
    try:
        for index in range(count):
            stdout = (folder / f'{stem}-{index}.stdout').open('xb')
            stderr = (folder / f'{stem}-{index}.stderr').open('xb')
            streams += [stdout, stderr]
            if actor == PEER:
                members.append(start_traced(argv, env, stdout, stderr))
            else:
                members.append(start_barrier(argv, env, stdout, stderr, scriptw=actor == 'scriptw'))
        serve_barriers([member for member in members if member['kind'] == 'barrier'], release=False)
        wait_traced([member for member in members if member['kind'] == 'traced'], release=False)
        samples = [read_process(member['pid']) for member in members]
        available = mem_available()
        release_all(members)
    except BaseException:
        kill_all(members)
        raise
    finally:
        for stream in streams:
            stream.close()
    for index, member in enumerate(members):
        output = (folder / f'{stem}-{index}.stdout').read_bytes()
        require(member['exit_code'] == 0, f'{stem}-{index} exited with {member["exit_code"]}')
        if actor == 'scriptw':
            report = json.loads(output)
            wasm_peer.entry_times(report, 'scriptw', record['stdout'], HOST_SAMPLES, HOST_WARMUPS)
            require(report['phase_barriers']['observation_index'] == HOST_HOLD_INDEX, 'host held another entry')
            entry.setdefault('store_allocation', report['store_allocation'])
        else:
            require(output.decode() == record['stdout'], f'{stem}-{index} printed other output')
        for suffix in ('.stdout', '.stderr'):
            (folder / f'{stem}-{index}{suffix}').unlink()
    return dict(entry, samples=samples, held_mem_available_bytes=available)


# ----------------------------------------------------------------------------- run

def summarize_density(groups):
    summary = {}
    for group in groups:
        key = (group['program'], group['actor'])
        totals = summary.setdefault(key, {})
        totals.setdefault(group['count'], []).append(group)
    result = []
    for (program, actor), by_count in summary.items():
        counts = {}
        for count, rows in sorted(by_count.items()):
            pss = [sum(sample[1] for sample in row['samples']) for row in rows]
            counts[str(count)] = dict(groups=len(rows), total_pss_bytes=statistics.median(pss),
                                      pss_per_process_bytes=statistics.median(s[1] for row in rows for s in row['samples']),
                                      uss_per_process_bytes=statistics.median(s[2] for row in rows for s in row['samples']),
                                      rss_per_process_bytes=statistics.median(s[0] for row in rows for s in row['samples']),
                                      threads=statistics.median(s[5] for row in rows for s in row['samples']))
        result.append(dict(program=program, actor=actor, counts=counts,
                           capacity=capacity({int(count): row['total_pss_bytes'] for count, row in counts.items()})))
    return result


def summarize_wall(rows, identity_rows):
    result = []
    for program in PROGRAMS:
        actors = {}
        for actor in WALL_ACTORS:
            group = [row for row in rows if row['program'] == program and row['actor'] == actor]
            if not group:
                continue
            actors[actor] = dict(processes=len(group), median_ns=statistics.median(row['elapsed_ns'] for row in group),
                                 minimum_ns=min(row['elapsed_ns'] for row in group),
                                 peak_rss_bytes=statistics.median(row['peak_rss_bytes'] for row in group))
            if actor == 'scriptw':
                for key in ('load_ns', 'first_entry_ns', 'extra_entries_ns'):
                    actors[actor][key] = statistics.median(row[key] for row in group)
        result.append(dict(program=program, actors=actors))
    return dict(programs=result, host_identity_command=dict(
        processes=len(identity_rows), median_ns=statistics.median(row['elapsed_ns'] for row in identity_rows),
        peak_rss_bytes=statistics.median(row['peak_rss_bytes'] for row in identity_rows)))


def summarize_steady(rows):
    result = []
    for program in PROGRAMS:
        actors = {}
        for actor in STEADY_ACTORS:
            group = [row for row in rows if row['program'] == program and row['actor'] == actor]
            if group:
                medians = [statistics.median(row['wall_ns']) for row in group]
                actors[actor] = dict(processes=len(group), median_ns=statistics.median(medians),
                                     block_median_ns=[statistics.median(statistics.median(row['wall_ns']) for row in group
                                                                        if row['block'] == block) for block in range(2)])
        result.append(dict(program=program, actors=actors))
    return result


def run(args):
    root = args.prepared.resolve()
    prepared = load(root / 'prepared.json')
    require(prepared['schema'] == SCHEMA + '-prepared' and prepared['status'] == 'complete', 'complete preparation required')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('tmp', 'cache', 'held', 'wall', 'steady'):
        (output / name).mkdir()
    machine = wasm_peer.scope(args.cloud, 2)
    env = wasm_peer.environment(output)
    before = checked_inputs(prepared, root)
    admitted = {name: [actor for actor, attempt in record['attempts'].items() if attempt['status'] == 'passed']
                for name, record in prepared['programs'].items()}
    counts, groups = ((1, 4), 1) if args.smoke else (COUNTS, GROUPS)
    wall_processes, steady_samples, steady_warmups = (2, 3, 1) if args.smoke else (WALL_PROCESSES, SAMPLES, WARMUPS)
    report = dict(schema=SCHEMA + '-run', status='incomplete', started_at=stamp(), smoke=args.smoke, machine=machine,
                  policy=POLICY, counts=list(counts), groups=groups, reserve_bytes=RESERVE_BYTES,
                  prepared_sha256=digest(root / 'prepared.json'),
                  measurement_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                  density=[], skipped=[], wall=[], host_identity=[], steady=[])
    # (1) Density. The N=1 group's RSS sizes the admission of the larger groups.
    single = {}
    plan = density_plan(PROGRAMS, counts, groups, admitted)
    for number, entry in enumerate(plan):
        key = (entry['program'], entry['actor'])
        estimate = single.get(key, prepared['programs'][entry['program']]['attempts'][entry['actor']]['peak_rss_bytes'])
        gate = admission(entry['count'], estimate, mem_available())
        if not gate['admitted']:
            report['skipped'].append(dict(entry, **gate))
            print('skipped', entry, gate, flush=True)
            continue
        group = density_group(dict(entry), prepared, root, output / 'held', env)
        if entry['count'] == 1:
            single[key] = max(single.get(key, 0), group['samples'][0][0])
        report['density'].append(group)
        print(f'{number + 1}/{len(plan)}', entry['program'], entry['actor'], 'N=' + str(entry['count']),
              round(sum(s[1] for s in group['samples']) / 2**20, 1), 'MiB PSS total', flush=True)
    # (2) Process wall, spawn to exit; the Scriptw CLI cache was warmed by prepare's run and is warmed again here.
    # A wall process reports its build source only; checked_inputs() binds the host file's digest around the whole run.
    cache_env = dict(env, SCRIPTW_CACHE_DIR=str(output / 'cache'), SCRIPTW_BENCHMARK_REPORT_IDENTITY='build-source')
    for program in PROGRAMS:
        if 'scriptw.run' in admitted[program]:
            run_wall(wall_argv('scriptw.run', prepared, root, program), output / 'wall', 'warm-' + program, cache_env, args.time)
    for round_index in range(wall_processes):
        resources, _ = run_wall([prepared['tools']['host']['path'], 'identity'], output / 'wall', f'{round_index:02d}-identity',
                                env, args.time)
        require(resources['exit_code'] == 0, 'host identity failed')
        report['host_identity'].append(dict(elapsed_ns=resources['elapsed_ns'], peak_rss_bytes=resources['peak_rss_bytes']))
        for index, program in enumerate(PROGRAMS):
            for actor in rotate(WALL_ACTORS, round_index + index):
                if actor not in admitted[program]:
                    continue
                stem = f'{round_index:02d}-{actor}-{program}'
                resources, stdout = run_wall(wall_argv(actor, prepared, root, program), output / 'wall', stem, cache_env, args.time)
                extra, detail = check_wall_output(actor, stdout, prepared['programs'][program]['stdout'])
                require(resources['exit_code'] == 0 and not resources['timed_out'] and extra is not None,
                        f'{stem} failed: {detail}')
                report['wall'].append(dict(round=round_index, program=program, actor=actor, elapsed_ns=resources['elapsed_ns'],
                                           peak_rss_bytes=resources['peak_rss_bytes'], **extra))
        print('wall round', round_index + 1, 'of', wall_processes, flush=True)
    # (3) Steady-state entries through the Porffor panel harnesses.
    steady = steady_plan(PROGRAMS, admitted)
    for number, entry in enumerate(steady):
        record = prepared['programs'][entry['program']]
        if entry['actor'] == 'scriptw':
            argv = [prepared['tools']['host']['path'], 'execute-scriptw', str(root / 'images' / record['scriptw_image']),
                    record['stdout'], str(steady_samples), str(steady_warmups)]
        else:
            argv = [prepared['tools']['node']['path'], *(LEAN_FLAGS if entry['actor'] == 'node.lean' else ()),
                    str(wasm_peer.V8_HARNESS), str(root / 'fixtures'), str(steady_samples), str(steady_warmups), entry['program']]
        stem = f"{number:03d}-{entry['actor']}-{entry['program']}"
        resources, raw = wasm_peer.invoke(argv, output / 'steady', stem, env, args.time)
        require(resources['exit_code'] == 0 and raw is not None, 'steady process failed: ' + stem)
        times = wasm_peer.entry_times(raw, 'scriptw' if entry['actor'] == 'scriptw' else 'v8', record['stdout'],
                                      steady_samples, steady_warmups)
        (output / 'steady' / (stem + '.stdout')).unlink()  # reduced to its checked entry times
        report['steady'].append(dict(entry, wall_ns=times, peak_rss_bytes=resources['peak_rss_bytes']))
        print(f'{number + 1}/{len(steady)}', entry['actor'], entry['program'], round(statistics.median(times) / 1e3, 1), 'us',
              flush=True)
    require(checked_inputs(prepared, root) == before, 'an input changed during measurement')
    require(sorted(os.sched_getaffinity(0)) == machine['affinity'], 'CPU affinity changed')
    report.update(status='complete', finished_at=stamp(), density_summary=summarize_density(report['density']),
                  wall_summary=summarize_wall(report['wall'], report['host_identity']),
                  steady_summary=summarize_steady(report['steady']))
    for name in ('tmp', 'cache', 'held'):
        shutil.rmtree(output / name)
    save(output / 'analysis.json', report)
    print(output / 'analysis.json')


# ----------------------------------------------------------------------------- publication

def publish(args):
    value = load(args.run / 'analysis.json')
    require(value['schema'] == SCHEMA + '-run' and value['status'] == 'complete' and not value['smoke'],
            'a complete, non-smoke run is required')
    prepared = load(args.prepared / 'prepared.json')
    require(value['prepared_sha256'] == digest(args.prepared / 'prepared.json'), 'run used another preparation')
    tools = prepared['tools']
    public = wasm_peer.public_tool
    failures = [dict(program=name, actor=actor, status=attempt['status'], detail=attempt['detail'])
                for name, record in prepared['programs'].items() for actor, attempt in record['attempts'].items()
                if attempt['status'] != 'passed']
    finished = datetime.fromisoformat(value['finished_at'])
    summary = dict(
        schema=SCHEMA, date=finished.date().isoformat(), run_started_at=value['started_at'], run_finished_at=value['finished_at'],
        measurement_commit=value['measurement_commit'],
        scriptw=dict(source_commit=tools['host']['identity']['source_commit'],
                     source_fingerprint=tools['host']['identity']['source_fingerprint'], host_sha256=tools['host']['sha256'],
                     host_bytes=tools['host']['bytes'], cli_sha256=tools['scriptw']['sha256'],
                     shared_configuration=tools['host']['identity']['shared_configuration']),
        tools=dict(node=public(tools['node']), node22=public(tools['node22']),
                   peer=dict(package=tools['peer']['package'], version=tools['peer']['version'], target=tools['peer']['target'],
                             optimization=tools['peer']['optimization'], entry=public(tools['peer']['entry']),
                             lock=public(tools['peer']['lock'])),
                   cc=public(tools['cc']), time=public(tools['time']),
                   harnesses={name: public(row) for name, row in tools['harnesses'].items()}),
        machine=value['machine'], policy=value['policy'], actors={actor: LABELS[actor] for actor in WALL_ACTORS},
        node_lean_flags=list(LEAN_FLAGS), counts=value['counts'], groups=value['groups'], reserve_bytes=value['reserve_bytes'],
        programs={name: dict(stdout=record['stdout'], source_sha256=record['source_sha256'],
                             scriptw_image_bytes=record['scriptw_image_bytes'],
                             scriptw_program_wasm_bytes=record['scriptw_program_wasm_bytes'],
                             native_bytes=record['native'].get('bytes'))
                  for name, record in prepared['programs'].items()},
        density=value['density_summary'], wall=value['wall_summary'], steady=value['steady_summary'],
        failures=failures, skipped=value['skipped'],
        raw=dict(density_fields=list(FIELDS),
                 density=[dict(program=row['program'], actor=row['actor'], count=row['count'], group=row['group'],
                               samples=row['samples']) for row in value['density']],
                 wall=[[row['program'], row['actor'], row['elapsed_ns'], row['peak_rss_bytes']] for row in value['wall']],
                 wall_fields=['program', 'actor', 'elapsed_ns', 'peak_rss_bytes'],
                 host_identity_ns=[row['elapsed_ns'] for row in value['host_identity']],
                 steady=[[row['block'], row['program'], row['actor'], row['wall_ns']] for row in value['steady']],
                 steady_fields=['block', 'program', 'actor', 'wall_ns']),
        evidence=dict(analysis_sha256=digest(args.run / 'analysis.json'), prepared_sha256=digest(args.prepared / 'prepared.json')))
    args.output.write_text(public_json(summary))
    print(args.output)


def public_json(summary):
    """Indented summary; each raw sample row stays on one line so the receipt stays small and diffable."""
    summary = dict(summary)
    raw = summary.pop('raw')
    rows = []
    for key, value in raw.items():
        if isinstance(value, list) and value and isinstance(value[0], (list, dict)):
            items = ',\n'.join('   ' + json.dumps(item, separators=(',', ':')) for item in value)
            rows.append(f'  {json.dumps(key)}: [\n{items}\n  ]')
        else:
            rows.append(f'  {json.dumps(key)}: {json.dumps(value)}')
    text = json.dumps(summary, indent=1)
    require(text.endswith('\n}'), 'unexpected JSON layout')
    text = text[:-2] + ',\n "raw": {\n' + ',\n'.join(rows) + '\n }\n}\n'
    require(json.loads(text) == dict(summary, raw=raw), 'public JSON does not round-trip')
    return text


def mib(value):
    return f'{value / 2**20:.1f}'


def tables(args):
    value = load(args.summary)
    density = {(row['program'], row['actor']): row for row in value['density']}
    failures = {(row['program'], row['actor']): row['status'].replace('-', ' ') for row in value['failures']}
    high = str(max(value['counts']))
    print(f'| Runtime, `startup.js` | PSS alone (MiB) | Marginal PSS per process (MiB) | USS at N={high} (MiB) '
          '| Processes per GiB | Processes in 16 GiB |')
    print('| --- | ---: | ---: | ---: | ---: | ---: |')
    for actor in DENSITY_ACTORS:
        row = density.get(('startup.js', actor))
        if row is None or row['capacity'] is None:
            continue
        print(f"| {LABELS[actor]} | {mib(row['counts']['1']['pss_per_process_bytes'])} | {mib(row['capacity']['marginal_bytes'])} "
              f"| {mib(row['counts'][high]['uss_per_process_bytes'])} | {row['capacity']['per_gib']:,} "
              f"| {row['capacity']['per_16_gib']:,} |")
    print()
    print('| Program | ' + ' | '.join(SHORT[actor] for actor in DENSITY_ACTORS) + ' |')
    print('| --- | ' + ' | '.join('---:' for _ in DENSITY_ACTORS) + ' |')
    for program in PROGRAMS:
        cells = []
        for actor in DENSITY_ACTORS:
            row = density.get((program, actor))
            if row and row['capacity']:
                cells.append(f"{mib(row['capacity']['marginal_bytes'])} MiB · {row['capacity']['per_16_gib']:,}")
            else:
                cells.append(failures.get((program, actor), 'not measured'))
        print(f'| `{program}` | ' + ' | '.join(cells) + ' |')
    print()
    print('| Program | ' + ' | '.join(SHORT[actor] + ' (ms)' for actor in WALL_ACTORS) + ' |')
    print('| --- | ' + ' | '.join('---:' for _ in WALL_ACTORS) + ' |')
    for row in value['wall']['programs']:
        cells = [f"{row['actors'][actor]['median_ns'] / 1e6:.1f}" if actor in row['actors']
                 else failures.get((row['program'], actor), 'not measured') for actor in WALL_ACTORS]
        print(f"| `{row['program']}` | " + ' | '.join(cells) + ' |')
    print()
    print('| Program | Scriptw host wall (ms) | of which: image load (ms) | first entry (ms) | 3 extra entries (ms) |')
    print('| --- | ---: | ---: | ---: | ---: |')
    for row in value['wall']['programs']:
        own = row['actors']['scriptw']
        print(f"| `{row['program']}` | {own['median_ns'] / 1e6:.1f} | {own['load_ns'] / 1e6:.1f} | {own['first_entry_ns'] / 1e6:.2f} "
              f"| {own['extra_entries_ns'] / 1e6:.2f} |")
    identity = value['wall']['host_identity_command']
    print(f"\nHost `identity` command alone (executable hash, no program): {identity['median_ns'] / 1e6:.1f} ms median "
          f"over {identity['processes']} processes.")
    print()
    print('| Program | ' + ' | '.join(SHORT[actor] + ' (µs)' for actor in STEADY_ACTORS) + ' | Scriptw ÷ Node 24 |')
    print('| --- | ' + ' | '.join('---:' for _ in STEADY_ACTORS) + ' | ---: |')
    for row in value['steady']:
        actors = row['actors']
        cells = [f"{actors[actor]['median_ns'] / 1e3:.1f}" if actor in actors else 'not measured' for actor in STEADY_ACTORS]
        ratio = f"{actors['scriptw']['median_ns'] / actors['node']['median_ns']:.2f}×" if 'scriptw' in actors and 'node' in actors else '—'
        print(f"| `{row['program']}` | " + ' | '.join(cells) + f' | {ratio} |')
    if value['failures']:
        print()
        print('| Program | Actor | Failed attempt | Detail |')
        print('| --- | --- | --- | --- |')
        for row in value['failures']:
            detail = row['detail'].replace('|', '\\|').replace('\n', ' ')
            print(f"| `{row['program']}` | {LABELS[row['actor']]} | {row['status']} | {detail[:120]} |")


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    cloud = "admit a managed cloud machine's inherited finite memory cap and record it"
    step = commands.add_parser('prepare')
    step.add_argument('--output', type=Path, required=True, help='new preparation directory')
    step.add_argument('--tools', type=Path, required=True, help='npm prefix holding the pinned peer compiler and its lock')
    step.add_argument('--host', type=Path, required=True, help='shared precompiled host (compiler_precompiled_baseline)')
    step.add_argument('--scriptw', type=Path, required=True, help='Scriptw CLI built from the same source as the host')
    step.add_argument('--node', type=Path, required=True, help='Node ' + NODE_VERSIONS['node'])
    step.add_argument('--node22', type=Path, required=True, help='Node ' + NODE_VERSIONS['node22'])
    step.add_argument('--time', type=Path, required=True, help='GNU time')
    step.add_argument('--cloud', action='store_true', help=cloud)
    step = commands.add_parser('run')
    step.add_argument('--prepared', type=Path, required=True)
    step.add_argument('--output', type=Path, required=True, help='new measurement directory')
    step.add_argument('--time', type=Path, required=True)
    step.add_argument('--cloud', action='store_true', help=cloud)
    step.add_argument('--smoke', action='store_true', help='N=1,4, one group, short harness runs; never publishable')
    step = commands.add_parser('publish')
    step.add_argument('--prepared', type=Path, required=True)
    step.add_argument('--run', type=Path, required=True)
    step.add_argument('--output', type=Path, required=True)
    step = commands.add_parser('tables')
    step.add_argument('summary', type=Path)
    args = parser.parse_args(argv)
    return dict(prepare=prepare, run=run, publish=publish, tables=tables)[args.command](args)


if __name__ == '__main__':
    main()
