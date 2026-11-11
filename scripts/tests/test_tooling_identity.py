"""Identity checks ignore local recovery files but still reject product drift."""

from pathlib import Path
import hashlib
import importlib.util
import os
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/check-scriptw-identity.sh"
RETIRED_NAME = "por" + "ffor"

spec = importlib.util.spec_from_file_location(
    "identity_provenance", REPO / "scripts/identity_provenance.py",
)
provenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provenance)


class FrozenProofTests(unittest.TestCase):
    def test_pre_rename_proof_requires_every_original_byte(self):
        old = 'li' + 'la'
        raw = ('{"executable":"' + old + '","count":7}\n').encode()
        path = 'docs/rust-rewrite/closed-rename-proof.json'
        with patch.dict(provenance.HISTORY, files={path: hashlib.sha256(raw).hexdigest()}):
            projected = provenance.project_history_projection(path, raw)
            self.assertNotIn(old.encode(), projected)
            self.assertEqual(projected.count(b'\n'), raw.count(b'\n'))
            with self.assertRaisesRegex(ValueError, 'frozen pre-rename evidence changed'):
                provenance.project_history_projection(path, raw.replace(b'7', b'8'))
            self.assertEqual(provenance.project_history_projection('unregistered.json', raw), raw)

    def test_historical_readme_counts_cannot_be_projected_after_drift(self):
        old = 'li' + 'la'
        raw = f'<!-- {old}-status:start -->\n7 passes\n<!-- {old}-status:end -->'.encode()
        with patch.dict(provenance.HISTORY, readme_status_sha256=hashlib.sha256(raw).hexdigest()):
            self.assertNotIn(old.encode(), provenance.readme_history_projection(raw))
            with self.assertRaisesRegex(ValueError, 'frozen pre-rename README publication changed'):
                provenance.readme_history_projection(raw.replace(b'7 passes', b'8 passes'))

    def test_current_closed_checkpoints_are_byte_bound(self):
        for name in ('exact-replay-checkpoint-20261013.json',):
            with self.subTest(name=name):
                path = Path('docs/rust-rewrite') / name
                raw = (REPO / path).read_bytes()
                admitted = provenance.audit_source(path, raw)
                self.assertNotEqual(admitted, raw)
                self.assertNotIn(('/workspace/' + RETIRED_NAME).encode(), admitted)
                self.assertEqual((REPO / path).read_bytes(), raw)
                with self.assertRaisesRegex(ValueError, 'frozen historical proof changed'):
                    provenance.audit_source(path, raw + b' ')

    def test_workspace_relative_asset_paths_require_exact_frozen_admission(self):
        import json
        name = 'closed-test-proof.json'
        fields = {'path': RETIRED_NAME + '/target/proof.json',
                  'product': RETIRED_NAME + '/target/proof.json',
                  'product_identifier': RETIRED_NAME + '-cli',
                  'undeclared_path': RETIRED_NAME + '/target/runtime',
                  'other_path': RETIRED_NAME + '/target/other.json',
                  'assets': [{'path': RETIRED_NAME + '/target/proof.json'}],
                  'references': [RETIRED_NAME + '/target/proof.json'],
                  'nearby': {'path': RETIRED_NAME + '/targetish/proof.json'}}
        raw = (json.dumps(fields) + '\n').encode()
        path = Path('docs/rust-rewrite') / name
        with patch.dict(provenance.PROOFS, {name: (None, hashlib.sha256(raw).hexdigest())}):
            actual = json.loads(provenance.audit_source(path, raw))
            self.assertEqual(actual, dict(fields, path='HISTORICAL_CHECKOUT/target/proof.json',
                                         product='HISTORICAL_CHECKOUT/target/proof.json',
                                         assets=[{'path': 'HISTORICAL_CHECKOUT/target/proof.json'}],
                                         references=['HISTORICAL_CHECKOUT/target/proof.json']))
            with self.assertRaisesRegex(ValueError, 'frozen historical proof changed'):
                provenance.audit_source(path, raw + b' ')
        with patch.dict(provenance.PROOFS, {name: ('closed-test', None)}):
            schema_raw = json.dumps(dict(fields, schema='closed-test')).encode()
            self.assertEqual(provenance.audit_source(path, schema_raw), schema_raw)
        self.assertEqual(provenance.audit_source(path, raw), raw)
        self.assertEqual(provenance.audit_source(Path('crates/scriptw-cli') / name, raw), raw)

    def test_frozen_admission_changes_only_host_paths_and_preserves_product_tokens(self):
        raw = (
            '{"cwd":"/workspace/' + RETIRED_NAME + '-next",'
            '"product":"' + RETIRED_NAME + '-cli"}\n'
        ).encode()
        name = "closed-test-proof.json"
        with patch.dict(provenance.PROOFS, {name: (None, hashlib.sha256(raw).hexdigest())}):
            actual = provenance.audit_source(Path("docs/rust-rewrite") / name, raw)
            expected = raw.replace(
                ("/workspace/" + RETIRED_NAME + "-next").encode(),
                b"/workspace/HISTORICAL_CHECKOUT",
            )
            self.assertEqual(actual, expected)
            self.assertIn((RETIRED_NAME + "-cli").encode(), actual)
            self.assertEqual(provenance.audit_source(Path("crates/scriptw-cli") / name, raw), raw)
            self.assertEqual(provenance.audit_source(Path("docs/rust-rewrite/unknown.json"), raw), raw)

    def test_any_changed_frozen_proof_byte_is_rejected(self):
        raw = b'{"schema":"same","value":1}\n'
        name = "closed-test-proof.json"
        with patch.dict(provenance.PROOFS, {name: (None, hashlib.sha256(raw).hexdigest())}):
            for changed in (raw + b" ", raw.replace(b"1", b"2")):
                with self.subTest(changed=changed):
                    with self.assertRaisesRegex(ValueError, "frozen historical proof changed"):
                        provenance.audit_source(Path("docs/rust-rewrite") / name, changed)


class IdentityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        shutil.copyfile(REPO / ".gitignore", self.root / ".gitignore")
        mapping = self.root / "docs/rust-rewrite/scriptw-identity-map.tsv"
        mapping.parent.mkdir(parents=True)
        shutil.copyfile(REPO / mapping.relative_to(self.root), mapping)
        for package in (
            "front", "ir", "runtime", "spec-exec", "aot-wasm", "backend-c",
            "backend-native", "engine", "test262", "cli", "runtime-image",
            "intl", "intl-collator-data", "verification-feature-roots",
        ):
            (self.root / f"crates/scriptw-{package}").mkdir(parents=True)
        entry = self.root / "crates/scriptw-cli/src/bin/scriptw.rs"
        entry.parent.mkdir(parents=True)
        entry.write_text("fn main() {}\n", encoding="utf-8")

    def git(self, *args):
        return subprocess.run(
            ["git", "-C", str(self.root), *args], check=True,
            capture_output=True, text=True,
        )

    def old_identity_file(self, relative):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        contents = f'{{"recovery_path": "/old/checkout/{RETIRED_NAME}/source"}}\n'
        path.write_text(contents, encoding="utf-8")
        return path, contents

    def audit(self):
        return subprocess.run(
            ["bash", str(SCRIPT)], cwd=self.root,
            capture_output=True, text=True, timeout=10,
        )

    def test_local_recovery_is_ignored_and_preserved(self):
        path, contents = self.old_identity_file(".scriptw-task-work/recovery/SOURCE.json")
        result = self.audit()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("check-scriptw-identity: ok", result.stdout)
        self.assertEqual(path.read_text(encoding="utf-8"), contents)
        listed = self.git("ls-files", "--others", "--exclude-standard").stdout
        self.assertNotIn(".scriptw-task-work/", listed)

    def test_previous_private_helpers_and_pascal_case_state_are_rejected(self):
        old = ('li' + 'la').title()
        path = self.root / 'crates/scriptw-ir/live.rs'
        for token in ('$' + old + 'GeneratorThrow', 'Current' + old + 'V8'):
            with self.subTest(token=token):
                path.write_text('const TOKEN: &str = "' + token + '";\n')
                self.assertNotEqual(self.audit().returncode, 0)
        path.write_text('const CURRENCY: &str = "Swazi Lilangeni";\n')
        self.assertEqual(self.audit().returncode, 0)

    def test_forced_tracked_recovery_still_gets_audited(self):
        relative = ".scriptw-task-work/recovery/SOURCE.json"
        self.old_identity_file(relative)
        self.git("add", "-f", "--", relative)
        result = self.audit()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(relative, result.stderr)

    def test_untracked_product_and_nested_recovery_still_get_audited(self):
        for relative in (
            "crates/scriptw-cli/src/new_product.json",
            "crates/scriptw-cli/.scriptw-task-work/SOURCE.json",
        ):
            with self.subTest(path=relative):
                path, _ = self.old_identity_file(relative)
                result = self.audit()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(relative, result.stderr)
                path.unlink()

    def test_frozen_native_doctest_retains_exact_checkout_path(self):
        path = self.root / "docs/rust-rewrite/native-comparison.json"
        path.write_text(
            '{"eliminated": [\n'
            f'  "/workspace/{RETIRED_NAME}/crates/scriptw-intl-data/src/generated/mod.rs - baked::provider (line 5)"\n'
            ']}\n', encoding="utf-8",
        )
        result = self.audit()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_native_receipt_exception_does_not_admit_product_drift(self):
        for relative, contents in (
            ("docs/rust-rewrite/native-comparison.json", f'{{"message": "{RETIRED_NAME} runtime"}}\n'),
            ("docs/rust-rewrite/native-comparison.json", f'  "/workspace/{RETIRED_NAME}/crates/{RETIRED_NAME}-cli/src/main.rs - main (line 5)"\n'),
            ("crates/scriptw-cli/src/receipt.json", f'  "/workspace/{RETIRED_NAME}/crates/scriptw-cli/src/main.rs - main (line 5)"\n'),
        ):
            with self.subTest(path=relative, contents=contents):
                path = self.root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents, encoding="utf-8")
                result = self.audit()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(relative, result.stderr)
                path.unlink()

    def write_proof(self, value, filename="default-workspace-eb1ca63c9-native-checkpoint-20261015.json"):
        import json
        path = self.root / "docs/rust-rewrite" / filename
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
        return path

    def proof(self, **fields):
        return {"schema": "portable-current-workspace-retention-v1", **fields}

    def test_captured_proof_roots_and_worktree_paths_keep_literal_bytes(self):
        roots = [f"/workspace/{RETIRED_NAME}{suffix}" for suffix in ("", "-next", "-repairs")]
        value = self.proof(cwd=roots[1], argv=[root + "/crates/scriptw-engine/src/lib.rs" for root in roots],
            cargo_package="path+file://" + roots[1] + "/crates/scriptw-engine#0.1.0",
            index=roots[0] + f"/.git/worktrees/{RETIRED_NAME}-next/index",
            diagnostic="actual checkout " + roots[1] + "; source retained",
            dependency_output="left\n\n" + roots[0] + "/target/native.rmeta: input.rs\n")
        path = self.write_proof(value)
        original = path.read_bytes()
        result = self.audit()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(path.read_bytes(), original)

    def test_historical_proof_paths_do_not_admit_product_api_or_ui_drift(self):
        for drift in (f"{RETIRED_NAME}-cli", f"{RETIRED_NAME}_engine", "$" + RETIRED_NAME.title(),
                RETIRED_NAME + " runtime", f"/workspace/{RETIRED_NAME}-next/crates/{RETIRED_NAME}-cli/src/main.rs"):
            with self.subTest(drift=drift):
                self.write_proof(self.proof(cwd=f"/workspace/{RETIRED_NAME}-next", product=drift))
                result = self.audit()
                self.assertNotEqual(result.returncode, 0)

    def test_unregistered_json_wrong_schema_and_product_context_are_rejected(self):
        for relative, value in (
                ("docs/rust-rewrite/unregistered-proof.json", self.proof(cwd=f"/workspace/{RETIRED_NAME}-next")),
                ("docs/rust-rewrite/default-workspace-eb1ca63c9-native-checkpoint-20261015.json", {"schema": "wrong", "cwd": f"/workspace/{RETIRED_NAME}-next"}),
                ("crates/scriptw-cli/src/proof.json", self.proof(cwd=f"/workspace/{RETIRED_NAME}-next"))):
            with self.subTest(path=relative):
                import json
                path = self.root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(value) + "\n", encoding="utf-8")
                self.assertNotEqual(self.audit().returncode, 0)
                path.unlink()

    def test_lookalike_checkout_roots_are_not_historical_paths(self):
        for suffix in ("-nextish", "-next.bad", "-next-product"):
            with self.subTest(suffix=suffix):
                self.write_proof(self.proof(cwd=f"/workspace/{RETIRED_NAME}{suffix}"))
                self.assertNotEqual(self.audit().returncode, 0)
        for prefix in ("/foreign", "https://example.invalid"):
            with self.subTest(prefix=prefix):
                self.write_proof(self.proof(cwd=prefix + f"/workspace/{RETIRED_NAME}-next"))
                self.assertNotEqual(self.audit().returncode, 0)

    def test_malformed_or_duplicate_registered_schema_is_rejected(self):
        for text in ('{"schema":', '{"schema":"portable-current-workspace-retention-v1","schema":"portable-current-workspace-retention-v1"}'):
            with self.subTest(text=text):
                path = self.write_proof(self.proof())
                path.write_text(text, encoding="utf-8")
                self.assertNotEqual(self.audit().returncode, 0)

    def test_binary_content_and_case_sensitivity_keep_grep_semantics(self):
        path = self.root / "crates/scriptw-cli/src/sample.dat"
        path.parent.mkdir(parents=True, exist_ok=True)
        for contents in ((RETIRED_NAME + "-cli\0").encode(), b"PORFFOR runtime\n"):
            with self.subTest(contents=contents):
                path.write_bytes(contents)
                result = self.audit()
                self.assertEqual(result.returncode, 0, result.stderr)
        path.write_text(RETIRED_NAME + "-cli\n", encoding="utf-8")
        result = self.audit()
        self.assertEqual(result.returncode, 1)
        # A token match and a word match remain two separate findings.
        self.assertEqual(result.stderr.count("sample.dat:1:"), 2)

    def test_nul_filename_records_preserve_unusual_names(self):
        for name in ("-leading.json", "colon:name.json", "new\nline.json", "bad-\udcff.json"):
            with self.subTest(name=name):
                path = self.root / name
                path.write_bytes((RETIRED_NAME + " runtime\n").encode())
                result = subprocess.run(
                    ["bash", str(SCRIPT)], cwd=self.root,
                    capture_output=True, timeout=10,
                )
                self.assertEqual(result.returncode, 1)
                self.assertIn(os.fsencode(name) + b":1:", result.stderr)
                path.unlink()

    def test_url_exception_uses_content_and_only_the_word_scan(self):
        path = self.root / (RETIRED_NAME + ".dev.json")
        path.write_text(RETIRED_NAME + " runtime\n", encoding="utf-8")
        self.assertEqual(self.audit().returncode, 1)
        path.write_text("https://github.com/mewhhaha/" + RETIRED_NAME + "\n", encoding="utf-8")
        self.assertEqual(self.audit().returncode, 0)
        path.write_text(RETIRED_NAME + ".dev " + RETIRED_NAME + "-cli\n", encoding="utf-8")
        result = self.audit()
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr.count(path.name + ":1:"), 1)

    def test_symlinks_are_followed_and_missing_paths_are_skipped(self):
        target = self.root / "target" / "fixture.json"
        target.parent.mkdir()
        target.write_text(RETIRED_NAME + " runtime\n", encoding="utf-8")
        link = self.root / "visible.json"
        link.symlink_to(target)
        self.assertEqual(self.audit().returncode, 1)
        self.git("add", "visible.json")
        target.unlink()
        self.assertEqual(self.audit().returncode, 0)
        link.unlink()
        self.assertEqual(self.audit().returncode, 0)

    def test_near_miss_doctest_and_non_newline_whitespace_are_not_lost(self):
        path = self.root / "docs/rust-rewrite/native-comparison.json"
        exact = f'  "/workspace/{RETIRED_NAME}/crates/scriptw-cli/src/main.rs - main (line 5)"'
        path.write_text(exact + "\n", encoding="utf-8")
        self.assertEqual(self.audit().returncode, 0)
        for suffix in (" extra", "\v" + RETIRED_NAME + " runtime"):
            with self.subTest(suffix=suffix):
                path.write_text(exact + suffix + "\n", encoding="utf-8")
                result = self.audit()
                self.assertEqual(result.returncode, 1)
                self.assertIn(exact + suffix, result.stderr)

    def test_empty_git_universe_does_not_read_stdin(self):
        (self.root / ".git/info/exclude").write_text("*\n", encoding="utf-8")
        result = self.audit()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_findings_cross_batch_boundaries_in_git_order(self):
        directory = self.root / "batch"
        directory.mkdir()
        for ordinal in range(270):
            content = RETIRED_NAME + " runtime\n" if ordinal in (0, 127, 128, 269) else "canonical\n"
            (directory / f"{ordinal:03}.json").write_text(content, encoding="utf-8")
        result = self.audit()
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr.splitlines(), [
            f"check-scriptw-identity: batch/{ordinal:03}.json:1:{RETIRED_NAME} runtime"
            for ordinal in (0, 127, 128, 269)
        ])

    def test_unmerged_index_stages_do_not_multiply_findings(self):
        relative = "conflicted.json"
        path = self.root / relative
        path.write_text(RETIRED_NAME + " runtime\n", encoding="utf-8")
        sha = self.git("hash-object", "-w", relative).stdout.strip()
        subprocess.run(
            ["git", "-C", str(self.root), "update-index", "--index-info"],
            input="".join(f"100644 {sha} {stage}\t{relative}\n" for stage in (1, 2, 3)),
            check=True, text=True, capture_output=True,
        )
        result = self.audit()
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr.splitlines(), [
            f"check-scriptw-identity: {relative}:1:{RETIRED_NAME} runtime"
        ] * 3)


if __name__ == "__main__":
    unittest.main()
