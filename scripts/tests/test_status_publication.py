import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import status_publication as publication


def projection_fixture():
    return {
        "producer": "scriptw",
        "real_suite": {
            "backend": "wasm-aot",
            "refresh_date": "2026-10-21",
            "passed": 102990,
            "total": 102994,
            "failed": 4,
            "pinned_revisions": {"ecma262": "spec-pin", "test262": "suite-pin"},
            "compiler_identity": {
                "source_fingerprint": "a" * 64,
                "executable_sha256": "b" * 64,
            },
            "snapshot_paths": {"json": "published.json", "txt": "published.txt"},
        },
    }


class StatusGeneration(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.parent = self.root / Path(publication.MANIFEST).parent
        self.parent.mkdir(parents=True)
        self.json_bytes = (
            b'{"producer":"scriptw","real_suite":{"backend":"wasm-aot","total":123}}'
        )
        self.text_bytes = b"total=123\n"
        generation = publication.generation_id(self.json_bytes, self.text_bytes)
        self.manifest = {
            "schema_version": 1,
            "backend": "wasm-aot",
            "generation": generation,
        }
        for field, suffix, data in (
            ("json", "json", self.json_bytes),
            ("text", "txt", self.text_bytes),
        ):
            name = f"published-status-wasm-aot-{generation}.{suffix}"
            (self.parent / name).write_bytes(data)
            self.manifest[field] = {
                "file": name,
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        self.write_manifest()

    def write_manifest(self):
        (self.root / publication.MANIFEST).write_text(json.dumps(self.manifest))

    def test_one_manifest_selects_both_members_and_ignores_orphans(self):
        (self.parent / "unselected.json").write_text("not valid JSON")
        parsed, source = publication.load_publication(self.root)
        self.assertEqual(parsed["real_suite"]["total"], 123)
        self.assertEqual(
            source,
            (
                Path(publication.MANIFEST).parent / self.manifest["json"]["file"]
            ).as_posix(),
        )

    def test_damage_cannot_fall_back_to_the_legacy_pair(self):
        (self.root / publication.SNAPSHOT).write_text('{"legacy":true}')
        member = self.parent / self.manifest["text"]["file"]
        member.write_text("wrong generation")
        with self.assertRaisesRegex(ValueError, "digest mismatch"):
            publication.load_publication(self.root)
        member.unlink()
        with self.assertRaises(FileNotFoundError):
            publication.load_publication(self.root)

    def test_pointer_cannot_escape_its_directory_or_mix_generation_members(self):
        original = copy.deepcopy(self.manifest)
        for bad in (
            "../../README.md",
            "published-status-wasm-aot-other.txt",
            "/tmp/report.txt",
        ):
            self.manifest["text"]["file"] = bad
            self.write_manifest()
            with self.assertRaisesRegex(ValueError, "own immutable generation"):
                publication.load_publication(self.root)
        self.manifest = original
        self.manifest["generation"] = "0" * 64
        self.write_manifest()
        with self.assertRaises(ValueError):
            publication.load_publication(self.root)

    def test_legacy_evidence_is_readable_only_when_no_manifest_exists(self):
        (self.root / publication.MANIFEST).unlink()
        (self.root / publication.SNAPSHOT).write_text('{"legacy":true}')
        self.assertEqual(
            publication.load_publication(self.root),
            ({"legacy": True}, publication.SNAPSHOT),
        )
        (self.root / publication.MANIFEST).write_text("broken manifest")
        with self.assertRaises(ValueError):
            publication.load_publication(self.root)

    def test_consistent_hashes_cannot_relabel_oracle_or_unknown_payloads(self):
        for value in (
            {"producer": "scriptw", "real_suite": {"backend": "spec-exec"}},
            {},
            [],
        ):
            payload = json.dumps(value).encode()
            generation = publication.generation_id(payload, self.text_bytes)
            self.manifest["generation"] = generation
            for field, suffix, data in (
                ("json", "json", payload),
                ("text", "txt", self.text_bytes),
            ):
                name = f"published-status-wasm-aot-{generation}.{suffix}"
                (self.parent / name).write_bytes(data)
                self.manifest[field] = {
                    "file": name,
                    "sha256": hashlib.sha256(data).hexdigest(),
                }
            self.write_manifest()
            with self.assertRaisesRegex(ValueError, "wasm-aot publication evidence"):
                publication.load_publication(self.root)

    def test_boolean_schema_and_oracle_backend_are_rejected(self):
        for key, value in (("schema_version", True), ("backend", "spec-exec")):
            prior = self.manifest[key]
            self.manifest[key] = value
            self.write_manifest()
            with self.assertRaises(ValueError):
                publication.load_publication(self.root)
            self.manifest[key] = prior


class ReadmeProjection(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.published = projection_fixture()
        self.git("init", "-q")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Status projection test")
        parent = self.root / Path(publication.MANIFEST).parent
        parent.mkdir(parents=True)
        data = (json.dumps(self.published).encode(), b"failed=4\n")
        generation = publication.generation_id(*data)
        manifest = {"schema_version": 1, "backend": "wasm-aot", "generation": generation}
        self.members = []
        for field, suffix, payload in zip(("json", "text"), ("json", "txt"), data):
            name = f"published-status-wasm-aot-{generation}.{suffix}"
            member = parent / name
            member.write_bytes(payload)
            self.members.append(member)
            manifest[field] = {"file": name, "sha256": hashlib.sha256(payload).hexdigest()}
        (self.root / publication.MANIFEST).write_text(json.dumps(manifest))
        self.write_readme("## Current Status\n<!-- scriptw-status:start -->\nold\n<!-- scriptw-status:end -->")
        self.commit()
        self.base = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True)

    def commit(self):
        self.git("add", "-A")
        self.git("commit", "-qm", "fixture")

    def write_readme(self, block):
        (self.root / "README.md").write_text(f"# Fixture\n\n{block}\n\n## Design\nKeep.\n")

    def guard(self):
        script = Path(__file__).resolve().parents[1] / "check-readme-status-artifacts.sh"
        return subprocess.run([str(script), self.base], cwd=self.root, capture_output=True, text=True)

    def publish_projection(self):
        self.write_readme(publication.render_current_status(self.published))
        self.commit()

    def test_exact_projection_is_admitted_without_a_new_generation(self):
        self.publish_projection()
        result = self.guard()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("unchanged verified wasm-aot generation", result.stdout)

    def test_every_evidence_field_must_match_the_existing_publication(self):
        for path in (
            ("backend",), ("refresh_date",), ("passed",), ("total",), ("failed",),
            ("pinned_revisions", "ecma262"), ("pinned_revisions", "test262"),
            ("compiler_identity", "source_fingerprint"),
            ("compiler_identity", "executable_sha256"),
            ("snapshot_paths", "json"), ("snapshot_paths", "txt"),
        ):
            with self.subTest(field=path):
                altered = copy.deepcopy(self.published)
                owner = altered["real_suite"]
                for key in path[:-1]:
                    owner = owner[key]
                owner[path[-1]] = "fabricated"
                self.write_readme(publication.render_current_status(altered))
                self.commit()
                self.assertNotEqual(self.guard().returncode, 0)

    def test_missing_member_is_rejected(self):
        self.publish_projection()
        self.members[1].unlink()
        self.assertNotEqual(self.guard().returncode, 0)

    def test_uncommitted_member_is_rejected(self):
        self.publish_projection()
        member = self.members[1].relative_to(self.root).as_posix()
        self.git("rm", "--cached", member)
        self.git("commit", "-qm", "uncommit member")
        self.assertNotEqual(self.guard().returncode, 0)

    def test_nonregular_member_is_rejected(self):
        self.publish_projection()
        self.members[1].unlink()
        self.members[1].symlink_to(self.root / "README.md")
        self.commit()
        self.assertNotEqual(self.guard().returncode, 0)

    def test_extra_status_content_is_rejected(self):
        self.publish_projection()
        path = self.root / "README.md"
        path.write_text(path.read_text().replace("<!-- scriptw-status:end -->", "100%\n<!-- scriptw-status:end -->"))
        self.commit()
        self.assertNotEqual(self.guard().returncode, 0)

    def test_python_projection_matches_the_rust_publisher_template(self):
        source = Path(__file__).resolve().parents[2] / "crates/scriptw-cli/src/main.rs"
        match = re.search(
            r'fn render_current_status_block\b.*?format!\(\s*("(?:\\.|[^"\\])*")',
            source.read_text(), re.DOTALL,
        )
        self.assertIsNotNone(match)
        template = json.loads(match[1])
        real = self.published["real_suite"]
        rendered = template.format(
            real["backend"], real["refresh_date"], real["passed"], real["total"], real["failed"],
            real["pinned_revisions"]["ecma262"], real["pinned_revisions"]["test262"],
            real["compiler_identity"]["source_fingerprint"], real["compiler_identity"]["executable_sha256"],
            real["snapshot_paths"]["json"], real["snapshot_paths"]["txt"],
        )
        self.assertEqual(publication.render_current_status(self.published), rendered)
        self.assertIn("`4` failed", rendered)
        self.assertNotIn("100.0%", rendered)


if __name__ == "__main__":
    unittest.main()
