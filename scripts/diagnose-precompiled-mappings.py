#!/usr/bin/env python3
"""Unscored Linux mapping observations of this job's live trusted campaign.

This is not an archive executor. The caller must own the just-completed campaign
and its original producer outputs; checksums detect changes, not native authority.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import secrets
import socket
import signal
import stat
import subprocess
import time

import precompiled_comparison as p
from limited_verification import kernel_group, require_kernel_budget

ROOT = Path(__file__).resolve().parents[1]
MAX_READ = 2 * 1024**2
MAX_TOTAL = 64 * 1024**2
PHASES = ('after_transport_drop', 'after_instantiate', 'after_execution', 'after_store_drop')
MAX_SNAPSHOTS = len(PHASES)
MAX_CONTROL_LINE = 256
MAX_CONTROL_BYTES = MAX_CONTROL_LINE * len(PHASES)
DEADLINE = 120
HEADER = re.compile(r'^([0-9a-f]+)-([0-9a-f]+) ([-rwxps]{4}) ([0-9a-f]+) ([0-9a-f]+):([0-9a-f]+) (\d+)(?: +(.*))?$')


def bounded_read(path, limit=MAX_READ):
    with Path(path).open('rb') as stream:
        value = stream.read(limit + 1)
    p.require(len(value) <= limit, 'diagnostic input exceeds byte bound')
    return value


def unique_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            p.require(key not in result, 'repeated diagnostic metadata')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs)


def mappings(raw):
    rows = []
    for line in raw.decode('utf-8', errors='strict').splitlines():
        match = HEADER.fullmatch(line)
        if match:
            a, b, perms, offset, major, minor, inode, path = match.groups()
            rows.append(dict(start=int(a, 16), end=int(b, 16), permissions=perms,
                             file_offset=int(offset, 16), device=[int(major, 16), int(minor, 16)],
                             inode=int(inode), path=path or ''))
        elif ':' in line and rows:
            key, value = line.split(':', 1)
            if key in ('Rss', 'Pss', 'Private_Dirty', 'Anonymous', 'AnonHugePages', 'KernelPageSize'):
                words = value.split()
                p.require(len(words) == 2 and words[1] == 'kB' and words[0].isdigit()
                          and key not in rows[-1], 'invalid/repeated smaps counter')
                rows[-1][key] = int(words[0]) * 1024
    p.require(rows and all(row['end'] > row['start'] and 'Rss' in row for row in rows),
              'incomplete smaps')
    return rows


def exact_executable(rows, host, info):
    # Inode+actual path are required. Device aliases are recorded, not compared:
    # overlay/proc views can expose different device numbers for the main ELF.
    return any(row['path'] == str(host) and row['inode'] == info.st_ino
               and row['permissions'] == 'r-xp' for row in rows)


def owned_file(path):
    info = path.lstat()
    p.require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_nlink == 1,
              'live input must be an owned regular single-link file')
    return info


def live_campaign(directory, report, commit):
    p.require(report['source_commit'] == commit and report['policy']['consumer_compiler_enabled'] is False,
              'live campaign source/compiler policy differs')
    host = directory / 'bin/consumer'
    owned_file(host)
    p.require(host.resolve() == host, 'symlinked live executable path')
    p.require(report['tools']['consumer']['path'] == str(host)
              and p.digest(host) == report['tools']['consumer']['sha256'], 'original frozen executable differs')
    # An evidence archive has neither these live files nor the original paths.
    for command in report['commands']:
        if (command['kind'] in ('consume', 'wasm-precompile')
                or command['kind'] == 'source-produce' and command['runtime'] == 'scriptw'
                or command['id'] == report['tools']['consumer']['identity_command']):
            p.require(command['argv'][0] == str(host), 'campaign did not use this original frozen executable')
    return host


def presence(pid, rows, host):
    page = os.sysconf('SC_PAGE_SIZE')
    selected = [row for row in rows if row['path'] == str(host) and row['permissions'] in ('r--p', 'r-xp')]
    count = sum((row['end'] - row['start']) // page for row in selected)
    p.require(count <= 65536, 'executable presence page bound exceeded')
    result = []
    with Path(f'/proc/{pid}/pagemap').open('rb', buffering=0) as stream:
        for row in selected:
            pages = (row['end'] - row['start']) // page
            data = os.pread(stream.fileno(), pages * 8, row['start'] // page * 8)
            p.require(len(data) == pages * 8, 'short presence read')
            # Only bit 63 survives; never retain PFNs or other pagemap bits.
            bits = bytearray((pages + 7) // 8)
            for index in range(pages):
                bits[index // 8] |= ((data[index * 8 + 7] >> 7) & 1) << (index % 8)
            result.append(dict(file_offset=row['file_offset'], permissions=row['permissions'],
                               page_bytes=page, pages=pages, present_bits_lsb_hex=bits.hex()))
    return result


def kernel_budget():
    require_kernel_budget(4 * 1024**3)
    affinity = sorted(os.sched_getaffinity(0))
    p.require(len(affinity) == 1, 'diagnostic requires one CPU')
    group = kernel_group()
    return dict(cpu_affinity=affinity, counters={key: (group/key).read_text().strip()
                for key in ('memory.max', 'memory.swap.max', 'memory.oom.group',
                            'memory.current', 'memory.peak', 'memory.events')})


def kill_owned_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


class PhaseProtocol:
    """Closed ordered handshake; neither names nor timing imply a phase."""
    def __init__(self, nonce, observation_index):
        p.require(re.fullmatch('[0-9a-f]{32}', nonce) is not None, 'invalid phase nonce')
        p.require(type(observation_index) is int and observation_index >= 0, 'invalid phase index')
        self.nonce = nonce
        self.index = observation_index
        self.completed = []
        self.buffer = bytearray()
        self.received = 0

    def feed(self, data, capture):
        self.received += len(data)
        p.require(self.received <= MAX_CONTROL_BYTES, 'phase control byte bound exceeded')
        self.buffer.extend(data)
        acknowledgments = bytearray()
        while b'\n' in self.buffer:
            line, _, tail = self.buffer.partition(b'\n')
            self.buffer = bytearray(tail)
            p.require(not tail, 'pipelined phase events before acknowledgment')
            p.require(len(line) + 1 <= MAX_CONTROL_LINE, 'phase line byte bound exceeded')
            seq = len(self.completed) + 1
            p.require(seq <= len(PHASES), 'duplicate or extra phase event')
            phase = PHASES[seq - 1]
            expected = f'SWPHASE1 {self.nonce} {seq} {phase} {self.index}'.encode('ascii')
            p.require(line == expected, 'phase nonce/order/sequence/index mismatch')
            capture(phase, seq, self.index)
            self.completed.append(phase)
            acknowledgments.extend(f'SWACK1 {self.nonce} {seq} {phase} {self.index}\n'.encode('ascii'))
        p.require(len(self.buffer) < MAX_CONTROL_LINE, 'phase line byte bound exceeded')
        return bytes(acknowledgments)

    def eof(self):
        p.require(not self.buffer and self.completed == list(PHASES), 'incomplete phase control EOF')


def drain_owned(process, outputs, control=None, protocol=None, on_phase=None, deadline=DEADLINE):
    """Select control and both pipes together; kill only our owned group on failure."""
    p.require((control is None) == (protocol is None), 'incomplete phase drain configuration')
    started = time.monotonic()
    retained = 0
    control_retained = 0
    error = None
    cleanup_until = None
    pending = bytearray()
    with selectors.DefaultSelector() as selector:
        for name, pipe in (('stdout', process.stdout), ('stderr', process.stderr)):
            os.set_blocking(pipe.fileno(), False)
            selector.register(pipe, selectors.EVENT_READ, name)
        if control is not None:
            control.setblocking(False)
            selector.register(control, selectors.EVENT_READ, 'control')
        try:
            while selector.get_map() or process.poll() is None:
                now = time.monotonic()
                if error is None and now - started >= deadline:
                    error = 'diagnostic deadline exceeded'
                if error is not None and cleanup_until is None:
                    kill_owned_group(process)
                    cleanup_until = now + 2
                    if control is not None and control in selector.get_map():
                        selector.unregister(control)
                if cleanup_until is not None and now >= cleanup_until:
                    error += '; pipe cleanup deadline exceeded'
                    break
                for key, events in selector.select(0.02):
                    try:
                        if key.data == 'control':
                            if events & selectors.EVENT_READ:
                                data = control.recv(4096)
                                if not data:
                                    selector.unregister(control)
                                    protocol.eof()
                                    p.require(not pending, 'phase EOF before acknowledgment drained')
                                    continue
                                keep = min(len(data), MAX_CONTROL_BYTES - control_retained)
                                outputs['control'].write(data[:keep])
                                control_retained += keep
                                p.require(not pending, 'phase event before acknowledgment drained')
                                pending.extend(protocol.feed(data, on_phase))
                            if events & selectors.EVENT_WRITE and pending:
                                sent = control.send(pending)
                                p.require(sent > 0, 'phase acknowledgment write failed')
                                del pending[:sent]
                            selector.modify(control, selectors.EVENT_READ | (selectors.EVENT_WRITE if pending else 0), 'control')
                        else:
                            data = os.read(key.fileobj.fileno(), 65536)
                            if not data:
                                selector.unregister(key.fileobj)
                                continue
                            keep = min(len(data), MAX_READ - retained)
                            outputs[key.data].write(data[:keep])
                            retained += keep
                            if keep < len(data):
                                raise ValueError('consumer stream byte bound exceeded; raw prefix retained')
                    except BlockingIOError:
                        continue
                    except (OSError, ValueError, KeyError) as failure:
                        if error is None:
                            error = str(failure)
            process.wait(timeout=2)
            if error is None and protocol is not None:
                protocol.eof()
        finally:
            if process.poll() is None or selector.get_map():
                kill_owned_group(process)
                process.wait(timeout=2)
            for pipe in (process.stdout, process.stderr):
                pipe.close()
    if error is not None:
        raise ValueError(error)


def validate_endpoint(raw, report, image, case, runtime, nonce, observation_index):
    value = unique_json(raw)
    p.require(isinstance(value, dict), 'diagnostic endpoint is not an object')
    expected = dict(schema='scriptw-precompiled-phase-barriers-v1', nonce=nonce,
                    observation_index=observation_index, completed_phases=list(PHASES))
    p.require(value.get('phase_barriers') == expected
              and value.get('excluded_from_scored_cohorts') is True
              and value.get('diagnostic_total_timings_include_barrier_waits') is True,
              'missing or mismatched completed phase marker')
    # Only a transient copy enters ordinary checksum admission. The raw report
    # remains explicitly unscored and must never be submitted as a scored row.
    ordinary = dict(value)
    del ordinary['phase_barriers']
    del ordinary['excluded_from_scored_cohorts']
    del ordinary['diagnostic_total_timings_include_barrier_waits']
    _, matched = p._consumer(ordinary, report, image, case, runtime)
    p.require(matched, 'diagnostic output checksum mismatch')


def verify_executable(pid, host, info):
    p.require(os.readlink(f'/proc/{pid}/exe') == str(host), 'live proc executable path differs')
    actual = Path(f'/proc/{pid}/exe').stat()
    p.require(actual.st_ino == info.st_ino, 'live proc executable inode differs')
    return actual


def capture(host, argv, destination, remaining, present_bits, observation_index):
    destination.mkdir()
    nonce = secrets.token_hex(16)
    protocol = PhaseProtocol(nonce, observation_index)
    record = dict(argv=list(map(str, argv)), status='failed', snapshots=[], nonce=nonce,
                  observation_index=observation_index, phase_attribution_validated=False,
                  interpretation='unscored phase barriers; waits and timings excluded from canonical cohorts')
    process = None
    started = time.monotonic()
    case_budget = MAX_TOTAL // 4
    parent_socket, child_socket = socket.socketpair()
    try:
        info = owned_file(host)
        with (destination/'stdout').open('xb') as out, (destination/'stderr').open('xb') as err, (destination/'control').open('xb') as control_out:
            environment = dict(os.environ, SCRIPTW_WASM_TRACE='1')
            for key in ('SCRIPTW_WASM_TRACE_DUMP', 'SCRIPTW_PERF_MAP_DIR', 'SCRIPTW_BOUNDARY_READY_FD',
                        'SCRIPTW_BOUNDARY_READY_NONCE', 'SCRIPTW_ROW_PERF_CONTROL', 'SCRIPTW_ROW_PERF_ACK',
                        'SCRIPTW_ROW_PERF_REGION', 'SCRIPTW_EMIT_SIZE_REPORT'):
                environment.pop(key, None)
            environment.update(SCRIPTW_PRECOMPILED_PHASE_FD=str(child_socket.fileno()),
                               SCRIPTW_PRECOMPILED_PHASE_NONCE=nonce,
                               SCRIPTW_PRECOMPILED_PHASE_INDEX=str(observation_index))
            process = subprocess.Popen(argv, cwd=ROOT, env=environment, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE, start_new_session=True, pass_fds=(child_socket.fileno(),))
            child_socket.close()
            record['owned_child_pid'] = process.pid
            def snapshot(phase, seq, index):
                nonlocal case_budget
                capture_started = time.monotonic()
                p.require(remaining[0] >= MAX_READ and case_budget >= MAX_READ, 'phase snapshot budget exhausted')
                exe_info = verify_executable(process.pid, host, info)
                raw = bounded_read(f'/proc/{process.pid}/smaps')
                rows = mappings(raw)
                p.require(exact_executable(rows, host, info), 'phase smaps executable identity differs')
                verify_executable(process.pid, host, info)
                name = f'{seq:03d}-{phase}.smaps'
                (destination/name).write_bytes(raw)
                remaining[0] -= len(raw)
                case_budget -= len(raw)
                item = dict(phase=phase, sequence=seq, observation_index=index, nonce=nonce,
                    path=name, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest(),
                    rss_bytes=sum(row['Rss'] for row in rows), mappings=rows,
                    proc_exe=dict(inode=exe_info.st_ino, device=[os.major(exe_info.st_dev), os.minor(exe_info.st_dev)]))
                if present_bits:
                    try:
                        item['executable_presence'] = dict(status='present-bits-only-no-PFNs', mappings=presence(process.pid, rows, host))
                    except (OSError, ValueError) as failure:
                        item['executable_presence'] = dict(status='unavailable', error=str(failure))
                item['unscored_capture_seconds'] = time.monotonic() - capture_started
                record['snapshots'].append(item)
            drain_owned(process, {'stdout': out, 'stderr': err, 'control': control_out},
                        parent_socket, protocol, snapshot)
            p.require(process.returncode == 0, 'diagnostic consumer failed')
            p.require(len(record['snapshots']) == len(PHASES), 'missing phase captures')
            record['status'] = 'barriers_complete'
            record['completed_phases'] = list(protocol.completed)
        return record
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as failure:
        record['error'] = str(failure)
        return record
    finally:
        parent_socket.close()
        child_socket.close()
        if process is not None and process.poll() is None:
            kill_owned_group(process)
            process.wait(timeout=2)
        if process is not None:
            record['returncode'] = process.returncode
        for stream in ('stdout', 'stderr', 'control'):
            path = destination/stream
            if path.exists():
                record[stream] = dict(path=stream, bytes=path.stat().st_size, sha256=p.digest(path))
        record['unscored_elapsed_seconds'] = time.monotonic() - started


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    result = dict(schema='scriptw-precompiled-mapping-diagnostic-v2', scored=False, excluded_from_scored_cohorts=True, status='unavailable', cases=[],
                  limits=dict(deadline_seconds_per_case=DEADLINE, snapshots_per_case=MAX_SNAPSHOTS,
                              raw_smaps_total_bytes=MAX_TOTAL, stream_bytes_per_case=MAX_READ, control_bytes_per_case=MAX_CONTROL_BYTES),
                  interpretation='four acknowledged phase captures; unscored waits and timings never enter scored reports')
    def checkpoint():
        temporary = output/'report.pending.json'
        temporary.write_text(json.dumps(result, indent=2)+'\n')
        temporary.replace(output/'report.json')
    checkpoint()
    try:
        result['resources'] = dict(before=kernel_budget())
        checkpoint()
        campaign = args.campaign.resolve(strict=True)
        p.require(output != campaign and campaign not in output.parents, 'diagnostic output must be separate from campaign')
        report, _ = p.load_report(campaign/'report.json')
        commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
        host = live_campaign(campaign, report, commit)
        result.update(source_commit=commit, source_fingerprint=report['source_fingerprint'],
                      executable_sha256=p.digest(host), scored_report_sha256=p.digest(campaign/'report.json'),
                      shared_configuration=report['shared_configuration'])
        remaining = [MAX_TOTAL]
        for name in ('integer.js', 'retained-objects.js'):
            case = next(row for row in report['workloads'] if row['file'] == name)
            for runtime in ('scriptw', 'scriptc'):
                item = next(row for row in report['attempts'] if row['file'] == name and row['runtime'] == runtime and row['round'] == 0)
                p.require(item['status'] == 'ok', 'no successful live producer/consumer pair for diagnostic')
                image = campaign/item['image']['file']
                p.require(image.resolve() == image and campaign in image.parents, 'image is outside live campaign')
                owned_file(image)
                p.require(image.stat().st_size <= 256 * 1024**2, 'oversized live image')
                metadata = p.image_metadata(image.read_bytes(), runtime, report['source_fingerprint'])
                p.require(metadata == {key:value for key,value in item['image'].items() if key != 'file'}, 'live producer image changed')
                command = next(row for row in report['commands'] if row['id'] == item['consumer_id'])
                p.require(command['argv'][2] == str(image), 'original live image path differs')
                directory = output/(runtime+'-'+name)
                result['active_case'] = dict(runtime=runtime, file=name, status='capture-in-progress')
                checkpoint()
                observation = capture(host, command['argv'], directory, remaining, args.present_bits, report['warmups'] + report['samples'] - 1)
                observation.update(runtime=runtime, file=name, image_sha256=metadata['sha256'])
                result['cases'].append(observation)
                result.pop('active_case', None)
                checkpoint()
                if observation['status'] != 'failed':
                    try:
                        validate_endpoint(bounded_read(directory/'stdout'), report, item['image'], case, runtime, observation['nonce'], observation['observation_index'])
                        observation['endpoint_stdout_validated'] = True
                    except (ValueError, KeyError) as error:
                        observation.update(status='failed', error=str(error), endpoint_stdout_validated=False)
                p.require(p.digest(host) == result['executable_sha256'] and p.digest(image) == metadata['sha256'],
                          'live executable/image changed during diagnostic')
                if observation.get('endpoint_stdout_validated') and observation['status'] == 'barriers_complete':
                    observation.update(status='captured', phase_attribution_validated=True)
                checkpoint()
        p.require(p.digest(campaign/'report.json') == result['scored_report_sha256'], 'scored report changed')
        result['status'] = 'captured' if all(row['status'] == 'captured' for row in result['cases']) else 'incomplete'
    except (OSError, ValueError, KeyError, StopIteration, subprocess.SubprocessError) as error:
        result['error'] = str(error)
    finally:
        try:
            result.setdefault('resources', {})['after'] = kernel_budget()
        except (OSError, ValueError) as error:
            result['resource_after_error'] = str(error)
        checkpoint()
    print('Unscored mapping diagnostic:', result['status'], output/'report.json', flush=True)
    return 0 if result['status'] == 'captured' else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--campaign', required=True, type=Path, help='this job’s original live benchmark output, never downloaded evidence')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--present-bits', action='store_true')
    raise SystemExit(run(parser.parse_args()))
