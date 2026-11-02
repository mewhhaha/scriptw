"""Compilation charts require the complete, source-bound fresh-process plan."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from benchmark_metrics import digest
from compilation_comparison import ACTORS, ADAPTER, FIXTURES, POLICY, SCHEMA, SCRIPTC_COMPANIONS, load_report, process_environment, process_plan, summarize

spec = importlib.util.spec_from_file_location('compilation_benchmark', ROOT / 'scripts/benchmark-compilation.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class CompilationComparisonTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in ('sources', 'adapted', 'expected', 'logs', 'artifacts', 'tooling'):
            (self.root / name).mkdir()
        manifest = json.loads((FIXTURES / 'manifest.json').read_text())
        self.cases = manifest['execution']
        self.names = [case['file'] for case in self.cases]
        (self.root / 'sources/manifest.json').write_text(json.dumps(manifest))
        for case in self.cases:
            name = case['file']
            value = (FIXTURES / name).read_bytes()
            (self.root / 'sources' / name).write_bytes(value)
            (self.root / 'adapted' / name).write_bytes(ADAPTER.encode() + value)
            (self.root / 'expected' / name).write_text(case['stdout'])
        artifact_path = self.root / 'artifacts/program.wasm'
        artifact_path.write_bytes(b'\0asm\1\0\0\0')
        artifact = dict(path='artifacts/program.wasm', bytes=8, sha256=digest(artifact_path))
        tools = {name:dict(path='/frozen/' + name, bytes=1, sha256='a' * 64)
                 for name in ('scriptw', 'scriptc', 'zig', 'time')}
        tools['scriptw']['identity'] = dict(executable_sha256='a' * 64, source_fingerprint='c' * 64,
                                          source_revision=dict(kind='git-commit', commit='b' * 40))
        tools['scriptc']['version'] = '0.4.5'
        tools['zig']['version'] = '0.16.0'
        pack_path = self.root / 'tooling/runtime-pack.json'
        pack_path.write_text(json.dumps(dict(version='0.4.5', target=dict(name='wasm32-wasi'))))
        runtime_inputs = {'runtime-pack.json':digest(pack_path), 'program.o':'d' * 64}
        runtime_input_path = self.root / 'tooling/runtime-pack-inputs.json'
        runtime_input_path.write_text(json.dumps(runtime_inputs))
        tools['scriptc']['runtime_pack'] = dict(path='/frozen/runtime-pack', manifest_sha256=digest(pack_path),
                                              tree_sha256=digest(runtime_input_path), files=len(runtime_inputs))
        manifest = dict(schema='scriptc.native-toolchain.v1', compiler_version='0.4.5')
        manifest.update({field:'companions/' + field for field in SCRIPTC_COMPANIONS})
        toolchain_path = self.root / 'tooling/scriptc-toolchain.json'
        toolchain_path.write_text(json.dumps(manifest))
        companions = {'/frozen/scriptc.json':digest(toolchain_path)}
        companions.update({'/frozen/companions/' + field:'e' * 64 for field in SCRIPTC_COMPANIONS})
        companion_path = self.root / 'tooling/scriptc-toolchain-inputs.json'
        companion_path.write_text(json.dumps(companions))
        tools['scriptc']['toolchain'] = dict(manifest=dict(path='/frozen/scriptc.json', bytes=toolchain_path.stat().st_size,
                                                         sha256=digest(toolchain_path)),
                                           inputs_sha256=digest(companion_path), files=len(companions))
        (self.root / 'logs/scriptw-identity.stdout').write_text(json.dumps(tools['scriptw']['identity']))
        (self.root / 'logs/scriptc-identity.stdout').write_text('scriptc 0.4.5\n')
        (self.root / 'logs/zig-identity.stdout').write_text('0.16.0\n')
        plan = dict(process_plan=process_plan(self.names), tools=tools, policy=dict(POLICY), work_root='/measurement/work',
                    zig_global_cache_dir='/zig-cache', machine=dict(affinity=[0],
                    resources={'memory.max':str(4 * 1024**3), 'memory.swap.max':'0', 'memory.oom.group':'1'}))
        (self.root / 'plan.json').write_text(json.dumps(plan))
        inputs = {tool['path']:tool['sha256'] for tool in tools.values()}
        inputs.update({'/frozen/runtime-pack/' + name:sha for name, sha in runtime_inputs.items()})
        inputs.update(companions)
        for name in ('inputs-before.json', 'inputs-after.json'):
            (self.root / name).write_text(json.dumps(inputs))
        rows = []
        for index, entry in enumerate(plan['process_plan']):
            source_dir = 'sources'
            if entry['actor'] == 'scriptc':
                source_dir = 'adapted'
            row = dict(entry, source_sha256=digest(self.root / source_dir / entry['case']),
                       raw=f'logs/{index}.json', stdout=f'logs/{index}.stdout', stderr=f'logs/{index}.stderr',
                       resources=dict(exit_code=0, timed_out=False, elapsed_ns=(index + 1) * 1000,
                                      peak_rss_bytes=(index + 1) * 1024), artifacts=dict(program=artifact))
            if entry['actor'] == 'scriptw':
                row['artifacts']['runtime'] = artifact
            work = Path(plan['work_root']) / f"{index:03d}-{entry['actor']}-{entry['case']}"
            row['argv'], row['environment'] = runner.build_invocation(entry['actor'], tools[entry['actor']]['path'],
                work / entry['case'], work / 'program.wasm',
                process_environment(work, tools['scriptc']['runtime_pack']['path'], plan['zig_global_cache_dir']))
            for field in ('stdout', 'stderr'):
                (self.root / row[field]).touch()
            (self.root / row['raw']).write_text(json.dumps(row))
            rows.append(row)
        self.report = dict(schema=SCHEMA, status='complete', source_commit='b' * 40, finished_at='2026-11-02',
                           workloads=self.cases, tools=tools, policy=dict(POLICY), plan=plan, rows=rows)
        self.report['summary'] = summarize(self.report)
        self.path = self.root / 'report.json'

    def save(self):
        self.report['retained_files'] = [dict(path=str(path.relative_to(self.root)), bytes=path.stat().st_size,
                                              sha256=digest(path)) for path in sorted(self.root.rglob('*'))
                                        if path.is_file() and path != self.path]
        self.path.write_text(json.dumps(self.report))
        return self.path

    def test_plan_balances_each_actor_in_both_blocks_and_reverses_cases(self):
        plan = process_plan(self.names)
        self.assertEqual(len(plan), 136)
        self.assertEqual(plan[0]['case'], self.names[0])
        self.assertEqual(plan[68]['case'], self.names[-1])
        for name in self.names:
            first = [row['actor'] for row in plan if row['case'] == name and row['block'] == 0]
            second = [row['actor'] for row in plan if row['case'] == name and row['block'] == 1]
            self.assertEqual(second, [ACTORS[1 - ACTORS.index(actor)] for actor in first])
            self.assertEqual([first.count(actor) for actor in ACTORS], [2, 2])

    def test_complete_bound_report_is_admitted(self):
        report = load_report(self.save())
        self.assertEqual(len(report['summary']), 17)
        self.assertEqual(report['summary'][0]['actors']['scriptw']['processes'], 4)

    def test_missing_or_reordered_process_cannot_be_reduced(self):
        for rows in (self.report['rows'][:-1], self.report['rows'][::-1]):
            changed = dict(self.report, rows=rows)
            with self.assertRaisesRegex(ValueError, 'order or inventory'):
                summarize(changed)

    def test_failed_compile_has_no_successful_timing_bar(self):
        self.report['rows'][0]['resources']['exit_code'] = 1
        with self.assertRaisesRegex(ValueError, 'failed build'):
            summarize(self.report)

    def test_summary_cannot_be_edited_independently_of_processes(self):
        self.report['summary'][0]['actors']['scriptw']['ms'] = .0001
        with self.assertRaisesRegex(ValueError, 'summary differs'):
            load_report(self.save())

    def test_modified_source_rejected_even_with_updated_retained_hash(self):
        (self.root / 'sources' / self.names[0]).write_text('print(1);')
        with self.assertRaisesRegex(ValueError, 'original source differs'):
            load_report(self.save())

    def test_scriptc_adapter_cannot_rewrite_original_workload(self):
        (self.root / 'adapted' / self.names[0]).write_text('console.log(1);')
        with self.assertRaisesRegex(ValueError, 'print adapter differs'):
            load_report(self.save())

    def test_wasm_output_must_be_portable_not_serialized_native(self):
        (self.root / 'artifacts/program.wasm').write_bytes(b'\x7fELFxxxx')
        artifact = self.report['rows'][0]['artifacts']['program']
        artifact['sha256'] = digest(self.root / 'artifacts/program.wasm')
        for row in self.report['rows']:
            (self.root / row['raw']).write_text(json.dumps(row))
        with self.assertRaisesRegex(ValueError, 'portable Wasm'):
            load_report(self.save())

    def test_changed_tool_inputs_are_rejected(self):
        (self.root / 'inputs-after.json').write_text('{"compiler":"changed"}')
        with self.assertRaisesRegex(ValueError, 'inputs changed'):
            load_report(self.save())

    def test_publication_requires_source_fingerprint_and_all_binary_hashes(self):
        identity = self.report['tools']['scriptw']['identity']
        fingerprint = identity.pop('source_fingerprint')
        with self.assertRaisesRegex(ValueError, 'source fingerprint'):
            load_report(self.save())
        identity['source_fingerprint'] = fingerprint
        for actor in ('scriptc', 'zig', 'time'):
            with self.subTest(actor=actor):
                tool = self.report['tools'][actor]
                sha = tool.pop('sha256')
                with self.assertRaisesRegex(ValueError, 'tool binary identity'):
                    load_report(self.save())
                tool['sha256'] = sha

    def test_consistent_but_unbound_tool_hash_is_rejected(self):
        self.report['tools']['scriptc']['sha256'] = 'f' * 64
        (self.root / 'plan.json').write_text(json.dumps(self.report['plan']))
        with self.assertRaisesRegex(ValueError, 'tool binary differs from measured inputs'):
            load_report(self.save())

    def test_manifest_selected_companion_cannot_be_omitted_from_publication(self):
        path = self.root / 'tooling/scriptc-toolchain-inputs.json'
        companions = json.loads(path.read_text())
        del companions['/frozen/companions/ts7']
        path.write_text(json.dumps(companions))
        toolchain = self.report['tools']['scriptc']['toolchain']
        toolchain.update(inputs_sha256=digest(path), files=len(companions))
        (self.root / 'plan.json').write_text(json.dumps(self.report['plan']))
        with self.assertRaisesRegex(ValueError, 'missing selected Scriptc companion'):
            load_report(self.save())

    def test_native_compile_command_cannot_be_published_as_portable_compilation(self):
        row = self.report['rows'][0]
        row['argv'][1] = 'run'
        (self.root / row['raw']).write_text(json.dumps(row))
        with self.assertRaisesRegex(ValueError, 'command boundary differs'):
            load_report(self.save())

    def test_target_and_fresh_cache_settings_are_required(self):
        row = self.report['rows'][0]
        original = dict(row['environment'])
        for field, value in (('SCRIPTC_TARGET', 'linux-x64-gnu'), ('SCRIPTC_NO_CACHE', '0'),
                             ('SCRIPTW_CACHE_DIR', '/shared-cache'), ('ZIG_LIB_DIR', '/unidentified-zig-lib')):
            with self.subTest(field=field):
                row['environment'] = dict(original)
                row['environment'][field] = value
                (self.root / row['raw']).write_text(json.dumps(row))
                with self.assertRaisesRegex(ValueError, 'cache or target environment differs'):
                    load_report(self.save())

    def test_matching_policy_copies_cannot_change_measurement_boundary(self):
        self.report['policy']['timing'] = 'includes execution'
        self.report['plan']['policy']['timing'] = 'includes execution'
        (self.root / 'plan.json').write_text(json.dumps(self.report['plan']))
        with self.assertRaisesRegex(ValueError, 'measurement policy differs'):
            load_report(self.save())

    def test_scriptc_capture_tracks_selected_companions_without_scanning_installation(self):
        compiler = self.root / 'installed/scriptc'
        compiler.parent.mkdir()
        manifest = dict(schema='scriptc.native-toolchain.v1', compiler_version='0.4.5')
        expected = {Path(str(compiler) + '.json')}
        for field in SCRIPTC_COMPANIONS:
            selected = compiler.parent / 'selected' / field
            selected.mkdir(parents=True)
            content = selected / 'input'
            content.write_text(field)
            expected.add(content)
            manifest[field] = str(selected.relative_to(compiler.parent))
        Path(str(compiler) + '.json').write_text(json.dumps(manifest))
        unrelated = compiler.parent / 'unrelated-package'
        unrelated.write_text('not selected by Scriptc')
        captured, tracked = runner.retain_scriptc_toolchain(compiler, self.root)
        self.assertEqual(tracked, expected)
        hashes = json.loads((self.root / 'tooling/scriptc-toolchain-inputs.json').read_text())
        self.assertEqual(captured['files'], len(expected))
        self.assertNotIn(str(unrelated), hashes)
        selected_input = compiler.parent / 'selected/ts7/input'
        selected_input.write_text('changed executable')
        self.assertNotEqual(hashes[str(selected_input)], digest(selected_input))

    def test_edited_raw_timing_and_summary_cannot_override_archived_process(self):
        self.report['rows'][0]['resources']['elapsed_ns'] = 1
        self.report['summary'] = summarize(self.report)
        with self.assertRaisesRegex(ValueError, 'retained process measurement differs'):
            load_report(self.save())

    def test_build_commands_never_execute_or_native_compile_wasm(self):
        env = {'SCRIPTC_TARGET': 'wasm32-wasi'}
        argv, child_env = runner.build_invocation('scriptw', '/frozen/scriptw', '/source.js', '/out.wasm', env)
        self.assertEqual(argv, ['/frozen/scriptw', 'build', 'wasm', '/source.js'])
        self.assertEqual(child_env['SCRIPTW_WASM_DUMP'], '/out.wasm')
        self.assertNotIn('SCRIPTW_WASM_DUMP', env)
        argv, child_env = runner.build_invocation('scriptc', '/scriptc', '/source.js', '/out.wasm', env)
        self.assertEqual(argv, ['/scriptc', 'build', '/source.js', '-o', '/out.wasm'])
        self.assertEqual(child_env['SCRIPTC_TARGET'], 'wasm32-wasi')
        with self.assertRaisesRegex(ValueError, 'unknown compiler'):
            runner.build_invocation('v8', '/node', '/source.js', '/out.wasm', env)


if __name__ == '__main__':
    unittest.main()
