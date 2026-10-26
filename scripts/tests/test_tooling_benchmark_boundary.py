"""External benchmark installation must not reopen retired product workflows."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SETUP = '- uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4; benchmark oracle and external compiler only\n'
INSTALL = 'npm install --prefix "$RUNNER_TEMP/scriptc" --no-audit --no-fund --save-exact scriptc@0.4.5 @scriptc/runtime-wasm32-wasi@0.4.5\n'


class BenchmarkBoundaryTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        for folder in ('.github/workflows', 'scripts', 'crates'):
            (self.root / folder).mkdir(parents=True)
        for name in ('README.md', 'CONTRIBUTING.md', 'AGENTS.md'):
            (self.root / name).touch()
        for name in ('check-no-legacy-js.sh', 'check-benchmark-workflow-tools.py'):
            shutil.copyfile(ROOT / 'scripts' / name, self.root / 'scripts' / name)
        subprocess.run(['git', 'init', '-q'], cwd=self.root, check=True)

    def audit(self, content, name='compiler-benchmarks.yaml'):
        (self.root / '.github/workflows' / name).write_text(content)
        return subprocess.run(['bash', 'scripts/check-no-legacy-js.sh'], cwd=self.root,
                              capture_output=True, text=True, timeout=10)

    def test_exact_pinned_external_benchmark_setup_is_admitted(self):
        result = self.audit(SETUP + INSTALL)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_precompiled_comparison_accepts_only_the_same_pinned_setup(self):
        result = self.audit(SETUP + INSTALL, 'precompiled-benchmarks.yaml')
        self.assertEqual(result.returncode, 0, result.stderr)
        for extra in ('npm publish\n', 'node compiler/index.js\n'):
            result = self.audit(SETUP + INSTALL + extra, 'precompiled-benchmarks.yaml')
            self.assertNotEqual(result.returncode, 0)

    def test_other_workflows_cannot_use_the_exception(self):
        self.assertNotEqual(self.audit(SETUP + INSTALL, 'publish.yaml').returncode, 0)

    def test_benchmark_workflow_cannot_publish_packages_or_run_other_tools(self):
        for command in ('npm publish', 'node compiler/index.js', 'npx arbitrary', 'bun run build'):
            with self.subTest(command=command):
                self.assertNotEqual(self.audit(SETUP + INSTALL + command + '\n').returncode, 0)

    def test_modified_install_or_unpinned_action_is_rejected(self):
        for text in (SETUP.replace('@49933ea5288caeca8642d1e84afbd3f7d6820020', '@v4'),
                     INSTALL.replace('0.4.5', 'latest'), INSTALL.rstrip() + '; npm publish\n'):
            with self.subTest(text=text):
                self.assertNotEqual(self.audit(text).returncode, 0)

    def test_legacy_product_manifest_stays_rejected(self):
        (self.root / 'package.json').write_text('{}\n')
        self.assertNotEqual(self.audit(SETUP + INSTALL).returncode, 0)

    def test_missing_exception_filter_fails_closed(self):
        (self.root / 'scripts/check-benchmark-workflow-tools.py').unlink()
        self.assertNotEqual(self.audit(SETUP).returncode, 0)


if __name__ == '__main__':
    unittest.main()
