"""T27: failed or ambiguous Cargo queries cannot certify interpreter absence."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "check-no-interpreter-in-product-graph.sh"


class ProductGraphTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        artifact = self.root / "crates/scriptw-aot-wasm/tests/emission/product_artifact.rs"
        artifact.parent.mkdir(parents=True)
        artifact.write_text("fn product_wasm_contains_compiled_semantics_without_a_source_evaluator() {}\n")
        workflow = self.root / ".github/workflows/ci.yaml"
        workflow.parent.mkdir(parents=True)
        workflow.write_text("run: cargo test -p scriptw-aot-wasm --test emission -- product_artifact::\n")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        cargo = self.bin / "cargo"
        cargo.write_text(f"#!{sys.executable} -S\n" + '''
import json, os, sys
args = sys.argv[1:]
with open(os.environ["CALLS"], "a") as stream:
    stream.write(json.dumps(args) + "\\n")
package = args[args.index("-p") + 1] if "-p" in args else "--workspace"
kind = "oracle" if "--features" in args else "default"
mode = os.environ.get("MODE", "clean")
selected = os.environ.get("PACKAGE", package) == package
if selected and mode == kind + "-error":
    if kind == "oracle":
        print("boa_engine v0.21.1")  # Partial stdout must not hide failure.
    print("cargo query failed", file=sys.stderr)
    sys.exit(101)
if selected and mode == kind + "-empty":
    sys.exit(0)
print(package + " v0.1.0" + (" (/tmp/boa_engine-checkout)" if mode == "similar" else ""))
if mode == "similar":
    print("boa_engine_helper v0.1.0")
if (kind == "oracle" and mode != "missing-oracle") or (kind == "default" and mode == "leak" and selected):
    print("boa_engine v0.21.1")
''')
        cargo.chmod(0o755)

    def audit(self, mode="clean", package="scriptw-cli"):
        env = dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}",
                   CALLS=str(self.root / "calls"), MODE=mode, PACKAGE=package)
        return subprocess.run(["bash", str(SCRIPT)], cwd=self.root, env=env,
                              capture_output=True, text=True, timeout=10)

    def test_clean_graph_ignores_similar_names_and_checkout_paths(self):
        result = self.audit("similar")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in (self.root / "calls").read_text().splitlines()]
        # engine, cli, the whole workspace and the oracle crate, then the
        # three oracle feature queries.
        self.assertEqual(len(calls), 7)
        self.assertIn(["tree", "--locked", "--workspace", "--prefix", "none", "--format", "{p}"], calls)
        for args in calls:
            self.assertIn("--locked", args)
            self.assertEqual(args[args.index("--prefix") + 1], "none")

    def test_failed_default_queries_are_not_success(self):
        for package in ("scriptw-engine", "scriptw-cli"):
            with self.subTest(package=package):
                result = self.audit("default-error", package)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("could not inspect", result.stderr)
                self.assertIn("cargo query failed", result.stderr)

    def test_default_workspace_graph_leak_is_rejected(self):
        # `cargo build --workspace` compiles every member: an oracle reachable
        # there is compiled by every workspace build even if no product links it.
        for package in ("--workspace", "scriptw-spec-exec"):
            with self.subTest(package=package):
                result = self.audit("leak", package)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(f"default dependency graph of {package}", result.stderr)

    def test_failed_oracle_queries_reject_partial_output(self):
        result = self.audit("oracle-error")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("could not inspect", result.stderr)

    def test_empty_successful_queries_are_not_evidence(self):
        for kind in ("default", "oracle"):
            with self.subTest(kind=kind):
                result = self.audit(kind + "-empty")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("empty dependency graph", result.stderr)

    def test_product_interpreter_is_rejected_for_each_package(self):
        for package in ("scriptw-engine", "scriptw-cli"):
            with self.subTest(package=package):
                self.assertNotEqual(self.audit("leak", package).returncode, 0)

    def test_oracle_positive_control_is_required(self):
        self.assertNotEqual(self.audit("missing-oracle").returncode, 0)

    def test_artifact_and_ci_audits_remain_required(self):
        (self.root / "crates/scriptw-aot-wasm/tests/emission/product_artifact.rs").unlink()
        (self.root / ".github/workflows/ci.yaml").write_text("name: no audit\n")
        result = self.audit()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing emitted product artifact audit", result.stderr)
        self.assertIn("CI does not execute", result.stderr)


if __name__ == "__main__":
    unittest.main()
