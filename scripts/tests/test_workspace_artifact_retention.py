"""File-retention and verdict controls; these are not native suite evidence."""
import ast
from scripts.tests.workspace_runner_support import runner_body
import fcntl
import hashlib
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest

SOURCE=Path(__file__).resolve().parents[1]/'run_workspace_tests.py'

class ArtifactRetentionTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.profile=Path(self.temp.name)/'debug'
        (self.profile/'deps').mkdir(parents=True)
        self.binary=self.profile/'deps'/'test-output'
        self.binary.write_bytes(b'immutable compiled test output')
        self.watch=self.profile/'native.log'
        self.watch.write_text('test result: ok. 2 passed; 0 failed; 0 ignored;\n')

    def functions(self,preserve,*,native_status=0):
        case={'inventoried_tests':2,'executable_sha256':hashlib.sha256(self.binary.read_bytes()).hexdigest(),'package':'fixture','target':'fixture','kind':'test'}
        active={0:{'run':SimpleNamespace(poll=lambda:native_status),
                   'stream':SimpleNamespace(close=lambda:None),'case':case,
                   'watch':self.watch,'log':self.watch,'start':0,
                   'executable':self.binary,'label':'fixture'}}
        cleanup=[]
        nodes=[node for node in runner_body(SOURCE)
               if isinstance(node,ast.FunctionDef)and node.name in ('complete','finish_unselected_artifacts')]
        namespace={'active':active,'args':SimpleNamespace(preserve_test_artifacts=preserve),
            'profile_root':self.profile,'hashlib':hashlib,'re':__import__('re'),'fcntl':fcntl,
            'time':SimpleNamespace(monotonic=lambda:1),'evidence_path':str,
            'release_incremental':lambda row:cleanup.append(row),
            'result':{'source_unchanged':True},'save':lambda:None,'print':lambda *a,**k:None}
        exec(compile(ast.Module(body=nodes,type_ignores=[]),str(SOURCE),'exec'),namespace)
        return namespace,case,cleanup

    def test_completed_output_is_preserved_only_with_explicit_option(self):
        before=self.binary.stat()
        namespace,case,cleanup=self.functions(True)
        self.assertTrue(namespace['complete'](0))
        after=self.binary.stat()
        self.assertEqual((before.st_ino,before.st_mtime_ns,before.st_size),
                         (after.st_ino,after.st_mtime_ns,after.st_size))
        self.assertEqual(case['state'],'passed')
        self.assertFalse(case['executable_released'])
        self.assertEqual(len(cleanup),1)

    def test_default_policy_still_releases_the_verified_output(self):
        namespace,case,_=self.functions(False)
        self.assertTrue(namespace['complete'](0))
        self.assertFalse(self.binary.exists())
        self.assertTrue(case['executable_released'])

    def test_example_completion_retains_the_same_hash_and_verdict_checks(self):
        examples=self.profile/'examples';examples.mkdir()
        self.binary=self.binary.rename(examples/self.binary.name)
        namespace,case,cleanup=self.functions(True)
        case['kind']='example'
        self.assertTrue(namespace['complete'](0))
        self.assertEqual(case['state'],'passed')
        self.assertTrue(self.binary.exists())
        self.assertEqual(len(cleanup),1)
        namespace,case,_=self.functions(True)
        with self.assertRaisesRegex(AssertionError,'another profile or target directory'):
            namespace['complete'](0)

    def test_unselected_example_has_its_own_directory_and_no_verdict(self):
        examples=self.profile/'examples';examples.mkdir()
        self.binary=self.binary.rename(examples/self.binary.name)
        namespace,_,_=self.functions(True)
        _,kept=namespace['finish_unselected_artifacts']({('id','fixture','example'):self.binary},set())
        self.assertEqual(len(kept),1)
        self.assertNotIn('state',kept[0])
        with self.assertRaisesRegex(AssertionError,'another profile or target directory'):
            namespace['finish_unselected_artifacts']({('id','fixture','test'):self.binary},set())

    def test_preserving_an_output_never_changes_a_failed_native_verdict(self):
        self.watch.write_text('test result: FAILED. 1 passed; 1 failed; 0 ignored;\n')
        namespace,case,_=self.functions(True,native_status=1)
        namespace['complete'](0)
        self.assertEqual(case['state'],'failed')
        self.assertFalse(case['inventory_matches'])
        self.assertTrue(self.binary.exists())

    def test_changed_output_rejects_before_retirement_or_preservation(self):
        for preserve in [False,True]:
            with self.subTest(preserve=preserve):
                namespace,_,_=self.functions(preserve)
                self.binary.write_bytes(self.binary.read_bytes()+b'changed')
                with self.assertRaisesRegex(AssertionError,'Native artifact changed'):
                    namespace['complete'](0)
                self.assertTrue(self.binary.exists())

    def test_unselected_outputs_remain_unexecuted_under_both_storage_policies(self):
        key=('fixture-id','fixture','lib')
        for preserve in [True,False]:
            with self.subTest(preserve=preserve):
                namespace,_,_=self.functions(preserve)
                retired,kept=namespace['finish_unselected_artifacts']({key:self.binary},set())
                rows=kept if preserve else retired
                self.assertEqual(len(rows),1)
                self.assertEqual(rows[0]['sha256'],hashlib.sha256(b'immutable compiled test output').hexdigest())
                self.assertNotIn('state',rows[0])
                self.assertNotIn('outcomes',rows[0])
                self.assertEqual(self.binary.exists(),preserve)

    def test_wrong_profile_output_cannot_be_retired_or_preserved(self):
        other=Path(self.temp.name)/'wrong-profile';other.write_bytes(b'not selected')
        for preserve in [False,True]:
            namespace,_,_=self.functions(preserve)
            with self.assertRaises(AssertionError):
                namespace['finish_unselected_artifacts']({('id','fixture','lib'):other},set())
        self.assertEqual(other.read_bytes(),b'not selected')
