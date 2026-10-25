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
    "runtime_artifact/build_package/envelope.rs",
    "runtime_artifact/validated_profile.rs", "emit/module_assembly.rs",
    "emit/module_assembly/metadata.rs",
    "gc_types/host.rs", "prepared_script.rs", "functions/native_direct_eval.rs",
    "builtins/function/dynamic_constructor.rs",
)) + tuple(E + name for name in (
    "lib.rs", "embedded_runtime.rs", "embedded_runtime/build.rs", "embedded_runtime/images.rs",
    "wasm_runtime_link.rs", "wasm_native_source_compiler.rs", "graph_observation.rs",
    "embedded_runtime/load.rs",
    "embedded_runtime/native_backing.rs",
)) + tuple("crates/scriptw-runtime-image/src/" + name for name in ("lib.rs", "native_storage.rs", "native_lease.rs"))


def compact(source):
    return re.sub(r"\s+", "", re.sub(r"//[^\n]*", "", source))


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
            "cache::RuntimeIdentity::admit_with_jobs(selection,inputs.source_compilation_profile(),inputs.admission_jobs(),)?" in runtime and
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
    require("source_compilation_profile:RuntimeSourceCompilationProfile::Portable" in inputs and
            "build_runtime_artifact_with_source_compilation_profile(selection,compiler_source,RuntimeSourceCompilationProfile::Portable,)" in package and
            "RuntimeIdentity::admit(selection,source_compilation_profile)?" in package and
            "identity.digest()" in envelope and
            "cache::admit_payload(payload,identity,version,jobs,body_validation)?" in envelope,
            "ordinary build packages default Portable and retain profile-separated admission")
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
