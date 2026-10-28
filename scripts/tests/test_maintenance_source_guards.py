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

    def test_native_control_gates_require_test_and_source_compiler_capability(self):
        sources = {name: (ROOT / name).read_text() for name in host.FILES}
        self.assertEqual(host.check(sources), [])
        path = host.ENGINE + "src/lib.rs"
        source = host.compact(sources[path])
        for owner, count in host.NATIVE_TEST_GATES.items():
            before = host.NATIVE_TEST_GATE + owner
            self.assertEqual(source.count(before), count)
            for weaker in ("", "#[cfg(test)]",
                           '#[cfg(feature="native-source-compiler")]',
                           '#[cfg(any(test,feature="native-source-compiler"))]'):
                # Test both the field and initializer independently when the
                # same declaration prefix occurs twice.
                for occurrence in range(count):
                    with self.subTest(owner=owner, gate=weaker, occurrence=occurrence):
                        position = [m.start() for m in re.finditer(re.escape(before), source)][occurrence]
                        changed = dict(sources)
                        changed[path] = source[:position] + weaker + owner + source[position + len(before):]
                        self.assertIn("static_test_gates", {
                            row["invariant"] for row in host.check(changed)
                        })

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

    def test_pool_receipt_rejects_bad_data_before_freezing_and_keeps_raw_fallback(self):
        self.assertEqual(pool.check(ROOT), [])
        path = pool.A + "data/compiler_owned_pool.rs"
        source = pool.compact((ROOT / path).read_text())
        invariant = "actual runtime boundary and bytes precede prefix freezing"
        for before, after in [
            ("runtime.layout().require_pool(&prefix.0)?;", ""),
            ("if!matches{returnErr(", "ifmatches{returnErr("),
            ("else{prefix.require_runtime_data(runtime.bytes())?;}", "else{}"),
            ("runtime.admitted_compiler_pool_matches(&prefix.0.pooled_code_unit_bytes[..],&prefix.0.bytes[..],)",
             "Some(true)"),
        ]:
            with self.subTest(before=before):
                self.assertEqual(source.count(before), 1)
                self.assertIn(invariant, pool.check(ROOT, {path: source.replace(before, after)}))
        freeze = "prefix.0.bytes.freeze();"
        boundary = "runtime.layout().require_pool(&prefix.0)?;"
        changed = source.replace(freeze, "").replace(boundary, freeze + boundary)
        self.assertIn(invariant, pool.check(ROOT, {path: changed}))

        invariant = "immutable pool proof checks both actual reconstructed image dimensions and hashes"
        for path, before in [
            (pool.A + "runtime_artifact.rs", "Some(self.build_admission.as_ref()?.matches_pool(utf16,data))"),
            (pool.A + "data.rs", "pub(crate)constRUNTIME_DATA_SEGMENT_COUNT:usize=8;"),
            (pool.A + "data.rs", "pub(crate)constRUNTIME_STATIC_SEGMENT_ORDER:[usize;7]=[4,2,5,1,6,3,7];"),
            (pool.A + "runtime_artifact/build_package/metadata.rs", "bytes.len()==expected.span.len()"),
            (pool.A + "runtime_artifact/build_package/metadata.rs", "Sha256::digest(bytes).as_slice()==expected.digest"),
        ]:
            with self.subTest(before=before):
                source = pool.compact((ROOT / path).read_text())
                self.assertEqual(source.count(before), 1)
                self.assertIn(invariant, pool.check(ROOT, {path: source.replace(before, "true")}))

        invariant = "same-sized foreign data or offset cannot certify the canonical R prefix"
        path = pool.A + "data/compiler_owned_pool.rs"
        source = pool.compact((ROOT / path).read_text())
        for before in ("section.count()asusize!=RUNTIME_DATA_SEGMENT_COUNT", "ifrow.data!=bytes",
                       "(&self.0.bytes[year.clone()],None)", "Some(region.end())",
                       "value==addressasi32", "!ops.eof()"):
            with self.subTest(sparse_admission=before):
                self.assertEqual(source.count(before), 1)
                self.assertIn(invariant, pool.check(ROOT, {path: source.replace(before, "true")}))

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
            ("crates/scriptw-runtime-image/src/native_storage.rs", "hash.finalize().into()"),
            ("crates/scriptw-runtime-image/src/native_storage.rs", "digest != header.expected"),
        ]:
            with self.subTest(before=before):
                source = (ROOT / path).read_text()
                self.assertEqual(source.count(before), 1)
                self.assertTrue(profile.check(ROOT, {path: source.replace(before, "missing_owned_byte_proof")}))

    def test_streamed_backing_requires_independent_admission_and_exact_executable_source(self):
        self.assertEqual(profile.check(ROOT), [])
        for path, before in [
            (profile.E + "embedded_runtime/native_backing.rs", "written.digest() != expected"),
            (profile.E + "embedded_runtime/native_backing.rs", "file.metadata().ok()?.len() != written.decoded_bytes() as u64"),
            ("crates/scriptw-runtime-image/src/native_input.rs", "inode != self.inode"),
            ("crates/scriptw-runtime-image/src/native_input.rs", "major != expected_major || minor != expected_minor"),
            ("crates/scriptw-runtime-image/src/native_input.rs", "address.checked_add(length)? > end"),
            ("crates/scriptw-runtime-image/src/native_storage.rs", "decoder.finish().read(&mut buffer[..1]).ok()? != 0"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "table != loaded"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "bias.checked_add(header.entry)? != aux.entry"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "phdr.offset != header.phoff"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "p.address < phdr_end"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "owner.flags != PF_R"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "load.flags != PF_R"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "end > start.checked_add(load.file_size)?"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "actual != inode"),
            ("crates/scriptw-runtime-image/src/native_input/elf_origin.rs", "offset(line, address, length, inode) == Some(expected_offset)"),
        ]:
            with self.subTest(before=before):
                source = (ROOT / path).read_text()
                self.assertEqual(source.count(before), 1)
                self.assertTrue(profile.check(ROOT, {path: source.replace(before, "false")}))


class ImmutableReceiptGuards(unittest.TestCase):
    def rejected(self, path, before, after, invariant):
        source = (ROOT / path).read_text()
        pattern = r"\s*".join(re.escape(character) for character in before)
        matches = list(re.finditer(pattern, source))
        self.assertEqual(len(matches), 1, before)
        match = matches[0]
        changed = source[:match.start()] + after + source[match.end():]
        self.assertIn(invariant, profile.check(ROOT, {path: changed}))

    def test_actual_receipt_owners_are_admitted(self):
        self.assertEqual(profile.check(ROOT), [])

    def test_safe_external_and_cache_inputs_cannot_reuse_producer_validation(self):
        invariant = "safe external packages and disk caches still validate every function at use"
        for path, before in [
            (profile.A + "runtime_artifact/build_package/envelope.rs",
             "jobs,FunctionBodyValidation::AtUse,"),
            (profile.A + "runtime_artifact/cache.rs",
             "jobs,super::validation::FunctionBodyValidation::AtUse,"),
        ]:
            with self.subTest(path=path):
                self.rejected(path, before, before.replace("AtUse", "ValidatedAtBuild"), invariant)

    def test_immutable_metadata_has_no_safe_constructor_or_raw_decode_entry(self):
        invariant = "only unsafe immutable Inputs mint receipt authority; the raw parser stays private"
        path = profile.A + "runtime_artifact/build_package/inputs.rs"
        for before, after in [
            ("pubconstunsafefnwith_immutable_minimal_build(",
             "pubconstfnwith_immutable_minimal_build("),
            ("package:&'static[u8],source:&'static[u8;32],intl:&'static[u8],receipt:&'static[u8],}",
             "pubpackage:&'static[u8],source:&'static[u8;32],intl:&'static[u8],receipt:&'static[u8],}"),
            ("implImmutableMetadata{", "implImmutableMetadata{pubfnforged()->Self{panic!()}"),
        ]:
            with self.subTest(before=before):
                self.rejected(path, before, after, invariant)
        path = profile.A + "runtime_artifact/build_package/metadata/decode.rs"
        self.rejected(path, "proof:super::super::inputs::ImmutableMetadata,",
                      "proof:&'static[u8],", invariant)
        # A parser exposed from its child could mint BuildMetadata without the
        # opaque token even if the legitimate decode entry retained its proof.
        source = (ROOT / path).read_text()
        self.assertEqual(source.count("    fn parse("), 1)
        self.assertIn(invariant, profile.check(ROOT, {
            path: source.replace("    fn parse(", "    pub(super) fn parse(")
        }))
        self.rejected(path, "#[cfg(test)]pub(super)fnparser_rejects(",
                      "pub(super)fnparser_rejects(", invariant)
        path = profile.A + "runtime_artifact/build_package/inputs/admission.rs"
        source = (ROOT / path).read_text()
        forged = '\nfn forged() -> ImmutableMetadata { ImmutableMetadata { package: b"", source: &[0; 32], intl: b"", receipt: b"" } }\n'
        self.assertIn(invariant, profile.check(ROOT, {path: source + forged}))

    def test_producer_identity_cannot_be_minted_from_a_bare_digest(self):
        invariant = "receipt identity binds opaque selected data, source profile and producer digest"
        path = profile.A + "runtime_artifact/cache.rs"
        self.rejected(path, "proof:super::build_package::metadata::BuildIdentity,",
                      "proof:[u8;32],", invariant)
        path = profile.A + "runtime_artifact/build_package/metadata.rs"
        for before, after in [
            ("data:Arc<scriptw_intl::IntlCompilationData>,profile:RuntimeSourceCompilationProfile,digest:[u8;32],}",
             "pubdata:Arc<scriptw_intl::IntlCompilationData>,profile:RuntimeSourceCompilationProfile,digest:[u8;32],}"),
            ("implBuildIdentity{", "implBuildIdentity{pubfnforged()->Self{panic!()}"),
            ("data.service_selection().is_some()", "false"),
            ("data.identity().artifact_identity().as_bytes()!=self.intl_identity.as_bytes()", "false"),
            ("profile:self.profile,digest:self.selection_digest,", "profile:self.profile,digest:[0;32],"),
        ]:
            with self.subTest(before=before):
                self.rejected(path, before, after, invariant)

    def test_receipt_identity_fallback_retains_actual_profile_and_jobs(self):
        invariant = "receipt selection delegates exact identity with full external and cache fallback"
        path = profile.A + "runtime_artifact/build_package/inputs/admission.rs"
        self.rejected(path, "selection,self.source_compilation_profile,self.admission_jobs,",
                      "selection,RuntimeSourceCompilationProfile::Portable,self.admission_jobs,", invariant)
        path = profile.A + "runtime_artifact/build_package/inputs.rs"
        self.rejected(path, "envelope::decode_with_jobs(bytes,identity,expected_source,self.admission_jobs)",
                      "decode_immutable_build(bytes,identity,expected_source,self.admission_jobs)", invariant)

    def test_supplied_only_packages_keep_the_checked_no_emission_boundary(self):
        invariant = "supplied-only admission reuses exact identity and byte checks without emission or cache fallback"
        path = profile.A + "runtime_artifact/build_package/inputs.rs"
        self.rejected(path, "self.admit_package(&identity).map(Arc::new).ok_or_else(",
                      "cache::load_or_emit(&identity,self.cache,emit).ok_or_else(", invariant)

    def test_receipt_still_checks_source_profile_spans_and_exact_package_owner(self):
        invariant = "compact receipt retains checked framing, exact profile, bounded layout and package ownership"
        for path, before in [
            ("metadata/decode.rs", "Sha256::digest(body).as_slice()!=checksum"),
            ("metadata/decode.rs", "&r.take::<32>()?!=source"),
            ("metadata/decode.rs", "r.take::<1>()?!=[profile.wire()]"),
            ("metadata/decode.rs", "spans.windows(2).any(|pair|pair[0].end>pair[1].start)"),
            ("metadata.rs", "std::ptr::eq(self.package,package).then_some(self.package_digest)"),
        ]:
            with self.subTest(before=before):
                self.rejected(profile.A + "runtime_artifact/build_package/" + path,
                              before, "false", invariant)

    def test_receipt_accessors_select_two_distinct_immutable_images(self):
        invariant = "eleven immutable image definitions use seven original and four receipt accessors"
        self.rejected("crates/scriptw-runtime-image/src/metadata.rs",
                      "returnunsafe{&SCRIPTW_RUNTIME_PORTABLE_METADATA}",
                      "returnunsafe{&SCRIPTW_RUNTIME_METADATA}", invariant)
        self.rejected("crates/scriptw-runtime-image/build/objects.rs",
                      "constPARTS:[(&str,&str);11]", "constPARTS:[(&str,&str);9]", invariant)
        self.rejected("crates/scriptw-runtime-image/src/lib.rs",
                      "returnunsafe{&SCRIPTW_RUNTIME_PORTABLE_EXECUTION}",
                      "returnunsafe{&SCRIPTW_RUNTIME_EXECUTION}", invariant)
        self.rejected("crates/scriptw-runtime-image/src/lib.rs",
                      "returnunsafe{&SCRIPTW_RUNTIME_EXECUTION}",
                      "returnunsafe{&SCRIPTW_RUNTIME_PORTABLE_EXECUTION}", invariant)
