#!/usr/bin/env python3
"""Check portable/native source-compilation authority at its actual owners."""
from pathlib import Path
import re
import sys

A = "crates/scriptw-aot-wasm/src/"
E = "crates/scriptw-engine/src/"
FILES = tuple(A + name for name in (
    "runtime_artifact.rs", "runtime_artifact/source_compilation.rs",
    "runtime_artifact/cache.rs", "runtime_artifact/build_package.rs",
    "runtime_artifact/build_package/inputs.rs",
    "runtime_artifact/build_package/inputs/admission.rs",
    "runtime_artifact/build_package/envelope.rs",
    "runtime_artifact/build_package/metadata.rs",
    "runtime_artifact/build_package/metadata/decode.rs",
    "runtime_artifact/build_package/metadata/encode.rs",
    "runtime_artifact/validated_profile.rs", "emit/module_assembly.rs",
    "emit/module_assembly/metadata.rs",
    "gc_types/host.rs", "prepared_script.rs", "functions/native_direct_eval.rs",
    "builtins/function/dynamic_constructor.rs",
)) + tuple(E + name for name in (
    "lib.rs", "embedded_runtime.rs", "embedded_runtime/build.rs", "embedded_runtime/images.rs",
    "wasm_runtime_link.rs", "wasm_native_source_compiler.rs", "graph_observation.rs",
    "embedded_runtime/load.rs",
    "embedded_runtime/native_backing.rs",
)) + tuple("crates/scriptw-runtime-image/src/" + name for name in ("lib.rs", "metadata.rs", "native_storage.rs", "native_lease.rs")) + (
    "crates/scriptw-runtime-image/build/objects.rs",
)


def compact(source):
    return re.sub(r"\s+", "", re.sub(r"//[^\n]*", "", source))


def item(source, pattern):
    """Read one actual braced item, excluding neighboring proof spellings."""
    match = re.search(pattern, source)
    if not match:
        return ""
    start = source.find("{", match.end())
    depth = 1
    end = start + 1
    while start >= 0 and end < len(source) and depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return compact(source[match.start():end]) if start >= 0 and depth == 0 else ""


def function(source, name):
    return item(source, r"\bfn\s+" + re.escape(name) + r"\s*(?:<[^\n]*>)?\s*\(")


def check(root, overrides=None):
    source = {name: overrides[name] if overrides and name in overrides
              else (Path(root) / name).read_text() for name in FILES}
    c = {name: compact(text) for name, text in source.items()}
    failures = []

    def require(condition, message):
        if not condition:
            failures.append(message)

    profile = c[A + "runtime_artifact/source_compilation.rs"]
    require("#[default]Portable,NativeHost," in profile and
            "[0]=>Some(Self::Portable),[1]=>Some(Self::NativeHost),_=>None" in profile,
            "closed profile defaults Portable and decodes exactly one 0/1 byte")
    require("ifself.profile.is_some(){returnErr(" in profile and
            "self.imports[slot.index()]" in profile and
            "self.imports[slot.index()]=true;" in profile and
            "letSome(slot)=CompilerImportSlot::from_name(import.name)else{returnOk(());};" in profile and
            'import.module!="scriptw_host"' in profile and
            "TypeRef::Func(_)|TypeRef::FuncExact(_)" in profile and
            "RuntimeSourceCompilationProfile::Portable=>[false;CompilerImportSlot::ALL.len()]" in profile and
            "RuntimeSourceCompilationProfile::NativeHost=>[true;CompilerImportSlot::ALL.len()]" in profile and
            "ifself.imports!=expected" in profile,
            "duplicate/unknown frames and wrong, partial, duplicate or foreign compiler imports reject")
    require("enumCompilerImportSlot{DynamicFunction,GlobalScript,DirectEval,}" in profile and
            "constALL:[Self;3]=[Self::DynamicFunction,Self::GlobalScript,Self::DirectEval];" in profile and
            "imports:[bool;CompilerImportSlot::ALL.len()]" in profile,
            "exactly three typed compiler slots own the import census")
    for index, (name, slot) in enumerate((("compile_dynamic_function", "DynamicFunction"),
                                        ("compile_global_script", "GlobalScript"),
                                        ("compile_direct_eval", "DirectEval"))):
        require('"' + name + '"=>Some(Self::' + slot + '),' in profile and
                'Self::' + slot + '=>' + str(index) + ',' in profile,
                "all three compiler roles have closed distinct admission: " + name)
    runtime = c[A + "runtime_artifact.rs"]
    require("Self::Program(runtime)=>runtime.source_compilation_profile()" in runtime and
            "Self::Standalone=>RuntimeSourceCompilationProfile::Portable" in runtime and
            "letidentity=inputs.admit_identity(selection)?;" in runtime and
            "emit_runtime_artifact(selection,inputs.source_compilation_profile())" in runtime,
            "P inherits its actual R and residency resolves the complete selected profile")
    cache = c[A + "runtime_artifact/cache.rs"]
    require("sections.insert(RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION," in cache and
            "source_compilation_profile.wire()" in cache and
            "source_admission.import(&import).ok()?" in cache and
            "source_admission.section(section.data()).ok()?" in cache and
            "source_admission.finish().ok()?!=identity.source_compilation_profile" in cache and
            "profile:identity.source_compilation_profile" in cache and
            "RuntimeArtifact::new(Cow::Owned(self.bytes.to_vec()),self.layout,self.version,self.profile,self.compilation,)" in cache and
            "RuntimeArtifact::new(Cow::Borrowed(self.bytes),self.layout,self.version,self.profile,self.compilation,)" in cache,
            "identity and full raw admission bind physical frame plus exact import capability")
    package = c[A + "runtime_artifact/build_package.rs"]
    inputs = c[A + "runtime_artifact/build_package/inputs.rs"]
    envelope = c[A + "runtime_artifact/build_package/envelope.rs"]
    admission = c[A + "runtime_artifact/build_package/inputs/admission.rs"]
    package_admission = function(source[A + "runtime_artifact/build_package/inputs.rs"], "admit_package")
    metadata = c[A + "runtime_artifact/build_package/metadata.rs"]
    receipt = c[A + "runtime_artifact/build_package/metadata/decode.rs"]
    require("source_compilation_profile:RuntimeSourceCompilationProfile::Portable" in inputs and
            "build_runtime_artifact_with_source_compilation_profile(selection,compiler_source,RuntimeSourceCompilationProfile::Portable,)" in package and
            "RuntimeIdentity::admit(selection,source_compilation_profile)?" in package and
            "identity.digest()" in envelope and
            "cache::admit_payload(payload,identity,version,jobs,body_validation)?" in envelope,
            "ordinary build packages default Portable and retain profile-separated admission")
    require("admit_envelope(package,identity,expected_source,jobs,FunctionBodyValidation::AtUse,)" in
            function(source[A + "runtime_artifact/build_package/envelope.rs"], "decode_with_jobs") and
            "decode_payload(payload,identity,version,jobs,super::validation::FunctionBodyValidation::AtUse,)" in
            function(source[A + "runtime_artifact/cache.rs"], "decode_with_jobs"),
            "safe external packages and disk caches still validate every function at use")
    require("modadmission;" in inputs and
            "Candidate::ImmutableMinimal(proof)" in admission and
            "metadata::BuildMetadata::decode(proof,self.source_compilation_profile)" in admission and
            "cache::RuntimeIdentity::admit_with_jobs(selection,self.source_compilation_profile,self.admission_jobs,)" in admission and
            re.search(r"Candidate::External\{bytes,expected_source,\}=>envelope::decode_with_jobs\(bytes,identity,expected_source,self\.admission_jobs,?\)", package_admission) and
            re.search(r"Candidate::ImmutableBuild\{bytes,expected_source,\}=>decode_immutable_build\(bytes,identity,expected_source,self\.admission_jobs,?\)", package_admission),
            "receipt selection delegates exact identity with full external and cache fallback")
    supplied = function(source[A + "runtime_artifact/build_package/inputs.rs"], "admit_supplied_package")
    require("letidentity=self.admit_identity(selection)?;" in supplied and
            "self.admit_package(&identity).map(Arc::new).ok_or_else(" in supplied and
            "cache::" not in supplied and "load_or_emit" not in supplied,
            "supplied-only admission reuses exact identity and byte checks without emission or cache fallback")
    require("pub(super)structImmutableMetadata{package:&'static[u8],source:&'static[u8;32],intl:&'static[u8],receipt:&'static[u8],}" in inputs and
            inputs.count("ImmutableMetadata{") == 3 and
            "ImmutableMetadata{" not in admission and
            inputs.count("implImmutableMetadata{") == 1 and
            item(source[A + "runtime_artifact/build_package/inputs.rs"], r"\bimpl\s+ImmutableMetadata\b").count("fn") == 1 and
            "pubconstunsafefnwith_immutable_minimal_build(" in inputs and
            "self.package=Some(Candidate::ImmutableMinimal(ImmutableMetadata{package:bytes,source:expected_source,intl:metadata,receipt:runtime_metadata,}));" in
            function(source[A + "runtime_artifact/build_package/inputs.rs"], "with_immutable_minimal_build") and
            "decode(proof:super::super::inputs::ImmutableMetadata,profile:RuntimeSourceCompilationProfile,)" in receipt and
            re.search(r"(?m)^    fn parse\(", source[A + "runtime_artifact/build_package/metadata/decode.rs"]) is not None and
            receipt.count("Self::parse(") == 2 and
            "#[cfg(test)]pub(super)fnparser_rejects(" in receipt,
            "only unsafe immutable Inputs mint receipt authority; the raw parser stays private")
    require("pub(incrate::runtime_artifact)structBuildIdentity{data:Arc<scriptw_intl::IntlCompilationData>,profile:RuntimeSourceCompilationProfile,digest:[u8;32],}" in metadata and
            metadata.count("BuildIdentity{") == 3 and
            metadata.count("implBuildIdentity{") == 1 and
            item(source[A + "runtime_artifact/build_package/metadata.rs"], r"\bimpl\s+BuildIdentity\b").count("fn") == 1 and
            "cache::RuntimeIdentity::from_admitted_build(BuildIdentity{data:Arc::clone(data),profile:self.profile,digest:self.selection_digest,},)" in
            function(source[A + "runtime_artifact/build_package/metadata.rs"], "identity") and
            "data.service_selection().is_some()" in metadata and
            "data.identity().profile().profile()!=&scriptw_intl::IntlDataProfile::Minimal" in metadata and
            "data.identity().artifact_identity().as_bytes()!=self.intl_identity.as_bytes()" in metadata and
            "from_admitted_build(proof:super::build_package::metadata::BuildIdentity,)" in cache and
            "let(selected,source_compilation_profile,digest)=proof.into_parts();" in cache,
            "receipt identity binds opaque selected data, source profile and producer digest")
    require("Sha256::digest(body).as_slice()!=checksum" in receipt and
            "r.u32()?!=LAYOUT_VERSION" in receipt and
            "&r.take::<32>()?!=source" in receipt and
            "r.take::<1>()?!=[profile.wire()]" in receipt and
            "PoolBoundary::from_runtime_cache(static_len,utf16_len,strings)?" in receipt and
            "spans.windows(2).any(|pair|pair[0].end>pair[1].start)" in receipt and
            "identity.digest()!=self.selection_digest||identity.source_compilation_profile()!=self.profile" in metadata and
            "std::ptr::eq(self.package,package).then_some(self.package_digest)" in metadata,
            "compact receipt retains checked framing, exact profile, bounded layout and package ownership")
    profile_proof = c[A + "runtime_artifact/validated_profile.rs"]
    intl_gate = function(source[A + "runtime_artifact.rs"], "require_intl_profile")
    require("runtime.require_intl_profile(&selection)?;" in profile_proof and
            "ifself.build_admission.is_some()&&self.compilation_data.as_ref().is_some_and(|data|Arc::ptr_eq(data,selected)){returnOk(());}" in intl_gate and
            "crate::runtime_function::runtime_profile::require_runtime_profile(self.bytes(),selection)" in intl_gate,
            "profile reuse requires receipt authority and exact retained data or full raw comparison")
    assembly = c[A + "emit/module_assembly.rs"]
    require("uses_heap,kind.source_compilation_profile()," in assembly and
            "metadata::append_module_declarations(&mutmodule,kind,&script.foreign_modules);" in assembly and
            "data:Cow::Owned(vec![kind.source_compilation_profile().wire()])" in c[A + "emit/module_assembly/metadata.rs"],
            "actual module role owns both import planning and physical profile frame")
    require("|GcHostImport::CompileDirectEval=>{source_compilation_profile==crate::RuntimeSourceCompilationProfile::NativeHost}" in c[A + "gc_types/host.rs"],
            "compiler hooks are selected solely by NativeHost authority")
    for name in ("prepared_script.rs", "functions/native_direct_eval.rs", "builtins/function/dynamic_constructor.rs"):
        require("gc_host_imports().source_compilation_profile()" in c[A + name] and
                "RuntimeSourceCompilationProfile::Portable" in c[A + name] and
                "emit_reject_dynamic_source(" in c[A + name],
                "Portable miss remains an execution-time typed rejection: " + name)
    engine = c[E + "lib.rs"]
    require("self.emit_wasm_with_cache_on_current_thread(unit,cache.as_deref(),scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable,)" in engine and
            "self.emit_wasm_with_cache_on_current_thread(&unit,cache.as_deref(),scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,)?" in engine,
            "public export is Portable while real execution explicitly opts into NativeHost")
    require("RuntimeSourceCompilationProfile::NativeHost" in c[E + "graph_observation.rs"] and
            "build_runtime_artifact_with_source_compilation_profile(&selection,&source,profile,)" in c[E + "embedded_runtime/build.rs"] and
            "forprofilein[RuntimeSourceCompilationProfile::NativeHost,RuntimeSourceCompilationProfile::Portable,]" in c[E + "embedded_runtime/build.rs"],
            "graph observation and native image production explicitly retain native compilation")
    embedded = c[E + "embedded_runtime.rs"]
    require("Images::package_for_profile(source_compilation_profile)" in embedded and
            "with_source_compilation_profile(source_compilation_profile)" in embedded and
            "self.images.profile==runtime.source_compilation_profile()" in embedded and
            "RuntimeSourceCompilationProfile::Portable=>image::portable_package()," in c[E + "embedded_runtime/images.rs"] and
            "letpackage=Self::package_for_profile(profile);" in c[E + "embedded_runtime/images.rs"] and
            "RuntimeSourceCompilationProfile::Portable=>image::portable_manifest_bytes()," in c[E + "embedded_runtime/images.rs"] and
            "RuntimeSourceCompilationProfile::NativeHost=>image::package()," in c[E + "embedded_runtime/images.rs"] and
            "RuntimeSourceCompilationProfile::NativeHost=>image::manifest_bytes()," in c[E + "embedded_runtime/images.rs"],
            "each embedded image is selected and admitted only for its exact source profile")
    load = c[E + "embedded_runtime/load.rs"]
    require("if!self.matches(runtime,mode){returnNone;}" in load and
            "RuntimeSourceCompilationProfile::Portable=>scriptw_runtime_image::portable_native()?," in load and
            "RuntimeSourceCompilationProfile::NativeHost=>scriptw_runtime_image::native()?," in load and
            "if!self.accepts_native(&lease){returnNone;}" in load and
            "image.digest()==&self.manifest.native_digest" in load and
            load.index("self.accepts_native(&lease)") < load.index("NativeBacking::create(bytes)") and
            "Module::deserialize(engine,bytes)" in load and "drop(lease);" in load,
            "both native profiles retain independent digest admission before backing publication and transient byte ownership")
    backing = c[E + "embedded_runtime/native_backing.rs"]
    require("structNativeBacking(File);" in backing and "OFlags::TMPFILE|OFlags::EXCL|OFlags::CLOEXEC" in backing and
            "file.write_all(bytes).ok()?" in backing and "file.sync_data().ok()?" in backing and
            "0x0102_1994|0x8584_58f6" in backing and
            "Module::deserialize_open_file(engine,self.0.try_clone()?)" in backing,
            "native file backing remains private, unlinked, synced, non-tmpfs and owned by Wasmtime")
    image = "crates/scriptw-runtime-image/src/"
    require("modmetadata;pubusemetadata::{metadata,portable_metadata};" in c[image + "lib.rs"] and
            c[image + "lib.rs"].count("#[inline(never)]") == 7 and
            c[image + "metadata.rs"].count("#[inline(never)]") == 2 and
            "pubfnmetadata()->&'static[u8]" in c[image + "metadata.rs"] and
            "pubfnportable_metadata()->&'static[u8]" in c[image + "metadata.rs"] and
            "returnunsafe{&SCRIPTW_RUNTIME_METADATA}" in c[image + "metadata.rs"] and
            "returnunsafe{&SCRIPTW_RUNTIME_PORTABLE_METADATA}" in c[image + "metadata.rs"] and
            "constPARTS:[(&str,&str);9]" in c["crates/scriptw-runtime-image/build/objects.rs"] and
            '"SCRIPTW_RUNTIME_METADATA","scriptw-runtime-metadata.bin"' in c["crates/scriptw-runtime-image/build/objects.rs"] and
            '"SCRIPTW_RUNTIME_PORTABLE_METADATA","scriptw-runtime-portable-metadata.bin"' in c["crates/scriptw-runtime-image/build/objects.rs"],
            "nine immutable image definitions use seven original and two receipt accessors")
    lease = c[image + "native_lease.rs"]
    storage = c[image + "native_storage.rs"]
    require("Mutex<Result<Weak<NativeRuntimeBytes>,()>>" in lease and
            "Arc::downgrade(&image)" in lease and
            "super::native_storage::decode(encoded)" in lease and
            "&super::SCRIPTW_RUNTIME_PORTABLE_NATIVE[..]" in lease and
            "&super::SCRIPTW_RUNTIME_NATIVE[..]" in lease and
            "pubstructNativeRuntimeBytes{bytes:Box<[u8]>,digest:[u8;32],}" in storage and
            "pubfnbytes(&self)->&[u8]{&self.bytes}" in storage and
            "pubfndigest(&self)->&[u8;32]{&self.digest}" in storage and
            "letdigest:[u8;32]=Sha256::digest(&bytes).into();" in storage and
            "ifbytes.len()!=length||digest!=expected" in storage,
            "only verified immutable Cargo bytes enter weak leases with a derived digest")
    link = c[E + "wasm_runtime_link.rs"]
    require(link.count("RuntimeSourceCompilationProfile::from_module(program).ok()?==scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable") == 2 and
            "RuntimeSourceCompilationProfile::from_module(program.bytes)" in link and
            "profile!=runtime.source_compilation_profile()" in link,
            "both compiler-owned standalone decoders and actual linked P/R check physical authority")
    require("runtime.source_compilation_profile()!=scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost" in c[E + "wasm_native_source_compiler.rs"],
            "a native callback context cannot be minted from Portable R")
    return failures


if __name__ == "__main__":
    failures = check(Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd())
    for failure in failures:
        print("runtime source profile: " + failure, file=sys.stderr)
    raise SystemExit(bool(failures))
