"""Moved owners must not weaken the standalone compiler source controls."""
import importlib.util
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


host = load("check-abi8-native-direct-eval-host")
pool = load("check-immutable-compiler-pool")
profile = load("check-runtime-source-compilation-profile")


class MaintenanceSourceGuards(unittest.TestCase):
    def test_moved_engine_selectors_and_exact_test_gates_remain_required(self):
        sources = {name: (ROOT / name).read_text() for name in host.FILES}
        self.assertEqual(host.check(sources), [])

        def rejected(path, replacement, invariant):
            changed = dict(sources)
            changed[path] = replacement
            self.assertIn(invariant, {row["invariant"] for row in host.check(changed)})

        for path, gate in host.MOVED_TEST_GATES.items():
            with self.subTest(gate=path):
                source = host.compact(sources[path])
                self.assertEqual(source.count(gate), 1)
                rejected(path, source.replace(gate, gate.removeprefix("#[cfg(test)]")),
                         "moved_test_gates")
        parent = host.ENGINE + "src/tests.rs"
        for child in host.ENGINE_TEST_CHILDREN:
            with self.subTest(child=child):
                declaration = f'#[path="tests/{Path(child).name}"]mod{Path(child).stem};'
                rejected(parent, host.compact(sources[parent]).replace(declaration, ""),
                         "moved_test_gates")
        child = host.ENGINE_TEST_CHILDREN[0]
        name = re.search(r"#\[test\]\s*fn\s+(\w+)\s*\(", sources[child]).group(1)
        rejected(child, sources[child].replace("fn " + name + "(", "fn lost_selector(", 1),
                 "historical_selectors")
        rejected(child, sources[child] + "\n#[test]\nfn " + name + "() {}\n",
                 "historical_selectors")
        fingerprint = host.ENGINE + "compiler_fingerprint.rs"
        before = 'const TEST_ONLY_INPUTS: &[&str] = &['
        self.assertEqual(sources[fingerprint].count(before), 1)
        rejected(fingerprint, sources[fingerprint].replace(
            before, before + '\n"crates/scriptw-engine/src/wasm_gc_host.rs",'),
            "fingerprint_allowlist")

    def test_immutable_pool_requires_shared_proof_and_all_three_callers(self):
        self.assertEqual(pool.check(ROOT), [])
        path = pool.A + "runtime_artifact/validated_profile.rs"
        source = (ROOT / path).read_text()
        for before, after in [("emitted.runtime_layout.is_some()", "false"),
                              ("Arc::ptr_eq(actual, self.runtime())", "true")]:
            with self.subTest(proof=before):
                self.assertEqual(source.count(before), 1)
                self.assertIn("shared linkage owner rejects a new layout or a different runtime Arc",
                              pool.check(ROOT, {path: source.replace(before, after)}))
        for kind in ("function", "global_script", "direct_eval"):
            path = pool.A + "runtime_" + kind + ".rs"
            source = (ROOT / path).read_text()
            for before in ("validated.require_linked_program(", "runtime: Arc::clone(runtime)"):
                with self.subTest(kind=kind, before=before):
                    self.assertEqual(source.count(before), 1)
                    self.assertIn(kind + " keeps checked raw API and exact linked R",
                                  pool.check(ROOT, {path: source.replace(before, "missing_proof(")}))

    def test_typed_import_slots_keep_distinct_roles_and_complete_admission(self):
        self.assertEqual(profile.check(ROOT), [])
        path = profile.A + "runtime_artifact/source_compilation.rs"
        source = (ROOT / path).read_text()
        for before, after in [
            ("Self::DirectEval => 2,", "Self::DirectEval => 1,"),
            ('"compile_global_script" => Some(Self::GlobalScript)',
             '"compile_global_script" => Some(Self::DynamicFunction)'),
            ("self.imports[slot.index()] = true;", "self.imports[slot.index()] = false;"),
            ("[true; CompilerImportSlot::ALL.len()]", "[false; CompilerImportSlot::ALL.len()]"),
            ("if self.imports != expected", "if false"),
            ('import.module != "scriptw_host"', "false"),
        ]:
            with self.subTest(before=before):
                self.assertEqual(source.count(before), 1)
                self.assertTrue(profile.check(ROOT, {path: source.replace(before, after)}))


class EmbeddedProfileGuards(unittest.TestCase):
    def test_profile_selection_and_native_match_proofs_are_required(self):
        self.assertEqual(profile.check(ROOT), [])
        path = profile.E + "embedded_runtime.rs"
        source = (ROOT / path).read_text()
        for before in ("Images::package_for_profile(source_compilation_profile)",
                       "self.images.profile == runtime.source_compilation_profile()"):
            with self.subTest(before=before):
                self.assertEqual(source.count(before), 1)
                self.assertTrue(profile.check(ROOT, {path: source.replace(before, "missing_profile_proof")}))
        path = profile.E + "embedded_runtime/load.rs"
        source = (ROOT / path).read_text()
        for before in ("if !self.matches(runtime, mode)", "if !self.accepts_native(&lease)", "image.digest() == &self.manifest.native_digest", "drop(lease);"):
            with self.subTest(before=before):
                self.assertEqual(source.count(before), 1)
                self.assertTrue(profile.check(ROOT, {path: source.replace(before, "missing_native_admission_proof")}))

        path = profile.E + "embedded_runtime/native_backing.rs"
        source = (ROOT / path).read_text()
        for before in ("OFlags::EXCL", "file.sync_data().ok()?", "struct NativeBacking(File);"):
            with self.subTest(before=before):
                self.assertEqual(source.count(before), 1)
                self.assertTrue(profile.check(ROOT, {path: source.replace(before, "missing_immutable_backing_proof")}))

        path = profile.E + "embedded_runtime/images.rs"
        source = (ROOT / path).read_text()
        self.assertTrue(profile.check(ROOT, {path: source.replace("image::portable_package()", "image::package()")}))


class TransientNativeImageGuards(unittest.TestCase):
    def test_owned_digest_and_weak_lease_proofs_are_required(self):
        self.assertEqual(profile.check(ROOT), [])
        for path, before in [
            ("crates/scriptw-runtime-image/src/native_lease.rs", "Arc::downgrade(&image)"),
            ("crates/scriptw-runtime-image/src/native_storage.rs", "Sha256::digest(&bytes).into()"),
            ("crates/scriptw-runtime-image/src/native_storage.rs", "digest != expected"),
        ]:
            with self.subTest(before=before):
                source = (ROOT / path).read_text()
                self.assertEqual(source.count(before), 1)
                self.assertTrue(profile.check(ROOT, {path: source.replace(before, "missing_owned_byte_proof")}))
