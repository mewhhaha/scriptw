#!/usr/bin/env python3
"""Source controls for one opaque, immutable R/layout/Intl admission proof."""
from pathlib import Path
import re
import sys

A = "crates/scriptw-aot-wasm/src/"
E = "crates/scriptw-engine/src/"
P = A + "runtime_artifact/validated_profile.rs"
C = E + "wasm_native_source_compiler.rs"
KINDS = ("function", "global_script", "direct_eval")
FILES = (P, C, A + "runtime_artifact.rs", A + "runtime_artifact/cache.rs",
         A + "runtime_artifact/build_package.rs", A + "runtime_function/runtime_profile.rs",
         A + "runtime_artifact/build_package/envelope.rs",
         A + "runtime_artifact/validated_profile/tests.rs", E + "wasm_native_source_compiler/tests.rs",
         A + "lib.rs", E + "lib.rs") + tuple(A + "runtime_" + kind + ".rs" for kind in KINDS) + tuple(
    E + "wasm_native_" + kind + "_compiler/factory.rs" for kind in KINDS)


def compact(text):
    return re.sub(r"\s+", "", re.sub(r"//[^\n]*", "", text))


def function(source, name):
    start = source.index("fn " + name + "(")
    start = source.index("{", start)
    depth = 1
    cursor = start + 1
    while depth:
        depth += (source[cursor] == "{") - (source[cursor] == "}")
        cursor += 1
    return compact(source[start + 1:cursor - 1])


def ordered(body, tokens):
    offsets = [body.find(token) for token in tokens]
    return all(offset >= 0 for offset in offsets) and offsets == sorted(offsets)


def check(sources):
    failures = []
    def require(ok, key, reason):
        if not ok:
            failures.append({"invariant": key, "reason": reason})
    c = {name: compact(text) for name, text in sources.items()}
    proof = c[P]
    require("pubstructValidatedRuntimeArtifact{runtime:Arc<RuntimeArtifact>,selection:IntlDataSelection,}" in proof and
            "pubfnruntime(&self)->&Arc<RuntimeArtifact>{&self.runtime}" in proof and
            "pub(crate)fnselection(&self)->&IntlDataSelection{&self.selection}" in proof and
            "&mutself" not in proof,
            "opaque_pair", "private fields and immutable projections bind exact Arc R plus actual selected Intl frames")
    require("!runtime.layout_admission.is_some_and(|version|version.0==LAYOUT_VERSION)" in proof and
            "ifprofile==IntlCompilationProfile::Minimal" in proof and
            "runtime.intl_compilation_data()" in proof and
            "IntlDataSelection::from_minimal_compilation_data(Arc::clone(data))" in proof and
            "unwrap_or_else(||IntlDataSelection::new(profile))" in proof and
            len(re.findall(r"require_runtime_profile\(runtime[.]bytes\(\),&selection,?\)\?;", proof)) == 1 and
            ordered(function(sources[P], "new"), ["runtime.layout_admission", "IntlDataSelection::new(profile)", "require_runtime_profile(", "Ok(Self{runtime,selection})"]),
            "single_profile_admission", "only the constructor validates actual layout and physical Intl pair before minting proof")
    runtime = c[A + "runtime_artifact.rs"]
    require((current_layout := re.search(r'constLAYOUT_VERSION:u32=(\d+);', runtime)) is not None and int(current_layout.group(1)) >= 15 and
            "structCurrentRuntimeLayoutVersion(u32);" in runtime and
            "(encoded==LAYOUT_VERSION).then_some(Self(encoded))" in runtime and
            "layout_admission:Option<CurrentRuntimeLayoutVersion>" in runtime and
            "fnnew(bytes:Cow<'static,[u8]>,layout:RuntimeLayout,version:CurrentRuntimeLayoutVersion,source_compilation_profile:RuntimeSourceCompilationProfile,compilation_data:Arc<scriptw_intl::IntlCompilationData>,)->Self" in runtime and
            "layout_admission:Some(version)" in runtime and
            "RuntimeArtifact::new(Cow::Owned(module.artifact.bytes),layout,CurrentRuntimeLayoutVersion::emitted_current(),source_compilation_profile,Arc::clone(compilation),)" in runtime,
            "actual_layout9", "layout authority comes from actual ABI9 emission or checked current-version envelope")
    fixture = function(sources[A + "runtime_artifact.rs"], "unadmitted_fixture")
    require("#[cfg(test)]fnunadmitted_fixture(" in runtime and "layout_admission:None" in fixture and
            "layout_admission:Some(" not in fixture,
            "unadmitted_fixtures", "synthetic fixtures cannot infer layout admission from valid-looking Intl bytes")
    cache = c[A + "runtime_artifact/cache.rs"]
    build = c[A + "runtime_artifact/build_package/envelope.rs"]
    require("CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(take(&mutpayload)?))?" in cache and
            "decode_payload(payload,identity,version,jobs,super::validation::FunctionBodyValidation::AtUse,)" in cache and
            "version:CurrentRuntimeLayoutVersion" in cache and
            "profile:identity.source_compilation_profile" in cache and
            "RuntimeArtifact::new(Cow::Owned(self.bytes.to_vec()),self.layout,self.version,self.profile,self.compilation,)" in cache and
            "RuntimeArtifact::new(Cow::Borrowed(self.bytes),self.layout,self.version,self.profile,self.compilation,)" in cache and
            "CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(cache::take(&mutpayload)?))?" in build and
            "cache::admit_payload(payload,identity,version,jobs,body_validation)?" in build,
            "checked_envelopes", "both cache and build envelopes propagate the admitted actual version token")
    physical = c[A + "runtime_function/runtime_profile.rs"]
    require("selected.component_sections().into_iter().collect()" in physical and
            "INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION" in physical and
            "INTL_SERVICE_SELECTION_CUSTOM_SECTION" in physical and
            ".remove(section.name()).is_some_and(|value|value.as_ref()==section.data())" in physical and
            "if!expected.is_empty(){returnErr(" in physical,
            "physical_profile", "same-name custom profiles, missing/duplicate sections and altered physical services remain rejected")
    context = c[C]
    require("validated_runtime:ValidatedRuntimeArtifact" in context and
            "execution:Arc<WasmAgentExecutionControl>" in context and
            "pub(super)fnvalidated_runtime(&self)->&ValidatedRuntimeArtifact{&self.validated_runtime}" in context and
            "pub(super)fnruntime(&self)->&Arc<RuntimeArtifact>{self.validated_runtime.runtime()}" in context and
            context.count("ValidatedRuntimeArtifact::new(runtime,policy.intl_profile().clone())") == 1 and
            ordered(function(sources[C], "new"), ["execution.remaining()?;", "letvalidated_runtime=ValidatedRuntimeArtifact::new(runtime,policy.intl_profile().clone());", "execution.remaining()?;letvalidated_runtime=validated_runtime.map_err", "Ok(Self{"] ) and
            "#[cfg(test)]modtests;" in context and "#[cfg(test)]modtests;" in proof,
            "context_owns_proof", "original execution Arc brackets one admission and the context retains that immutable proof")
    engine = c[E + "lib.rs"]
    require("policy.clone(),Arc::clone(&execution_control),native_mode,)?;" in engine and "pub(super)policy:WasmCompilationPolicy" in context,
            "original_context_deadline", "context creation uses the already armed original execution control and propagates admission error")
    linked = function(sources[P], "require_linked_program")
    require("emitted.runtime_layout.is_some()" in linked and
            "!emitted.artifact.runtime().is_some_and(|actual|Arc::ptr_eq(actual,self.runtime()))" in linked and
            "returnErr(EmitError::unsupported(diagnostic))" in linked,
            "exact_linkage_owner", "one linkage owner rejects a new layout or a different runtime Arc")
    for kind in KINDS:
        source = sources[A + "runtime_" + kind + ".rs"]
        raw = function(source, "emit_runtime_" + kind)
        validated = function(source, "emit_runtime_" + kind + "_with_validated_runtime")
        emitted = function(source, "emit_validated")
        order = ["program.program().wasm_blocking_diagnostic()"]
        if kind == "direct_eval":
            order.append("witness::root_source(program)?")
        order += ["ValidatedRuntimeArtifact::new(Arc::clone(runtime),intl_profile.clone())?", "emit_validated("]
        require(ordered(raw, order), "raw_error_order", kind + " preserves diagnostic/root checks before raw runtime admission")
        require("program.program().wasm_blocking_diagnostic()" in validated and
                (kind != "direct_eval" or "witness::root_source(program)?" in validated) and
                "ValidatedRuntimeArtifact::new(" not in validated and
                "require_runtime_profile(" not in compact(source),
                "proof_fast_path", kind + " proof entry retains program validation without reparsing R")
        require("letruntime=validated.runtime();" in emitted and
                "validated.require_linked_program(&emitted," in emitted and "runtime:Arc::clone(runtime)" in emitted and
                "emit_runtime_" + kind + "_module(program,validated)?" in emitted,
                "exact_emission_pair", kind + " emission consumes the exact retained proof pair and returns exact R Arc")
        host = c[E + "wasm_native_" + kind + "_compiler/factory.rs"]
        require("scriptw_aot_wasm::emit_runtime_" + kind + "_with_validated_runtime(program,context.validated_runtime()" in host and
                "!Arc::ptr_eq(artifact.runtime(),context.runtime())" in host and
                "scriptw_aot_wasm::emit_runtime_" + kind + "(" not in host and
                "ValidatedRuntimeArtifact::new(" not in host and "require_runtime_profile(" not in host,
                "native_proof_routes", kind + " actual native factory reuses the context proof, preserving exact R identity")
    direct = c[A + "runtime_direct_eval.rs"]
    require("caller:program.caller().clone()" in direct and "source:Arc::from(source)" in direct and
            "caller:RuntimeDirectEvalCaller" in direct and "source:Arc<[u16]>" in direct,
            "direct_witness_retained", "direct eval retains its complete caller and exact UTF16 source witness")
    tests = c[A + "runtime_artifact/validated_profile/tests.rs"]
    require("Arc::ptr_eq(proof.runtime(),&runtime)" in tests and
            "forwrongin(0..LAYOUT_VERSION).chain([LAYOUT_VERSION+1,u32::MAX])" in tests and
            "CurrentRuntimeLayoutVersion::admit(wrong).is_none()" in tests and
            "ValidatedRuntimeArtifact::new(unadmitted,IntlCompilationProfile::Minimal)" in tests and
            "profile_image(IntlCompilationProfile::Minimal,true)" in tests and
            "CustomIntlProfile::for_services(id.clone(),&[\"ListFormat\"])" in tests and
            "CustomIntlProfile::for_services(id,&[\"NumberFormat\"])" in tests and
            tests.count("assert_eq!(raw.bytes(),fast.bytes());") == 3 and
            "assert_eq!(raw.caller(),fast.caller());" in tests and "assert_eq!(raw.source(),fast.source());" in tests,
            "authored_profile_controls", "authored controls cover exact identity, admission/malformed/duplicate/service mismatch and all three APIs")
    tests = c[E + "wasm_native_source_compiler/tests.rs"]
    require("Arc::ptr_eq(&context.execution,&execution)" in tests and
            "forexpiredin[false,true]" in tests and "assert!(execution.remaining().is_err());" in tests and
            "assert!(error.parse_diagnostic().is_none());" in tests and "assert!(error.ir_diagnostic().is_none());" in tests,
            "authored_context_controls", "controls retain exact control Arc, invalid deadlines and typed admission classification")
    require(len(sources[P].splitlines()) <= 100 and
            len(sources[A + "runtime_artifact/validated_profile/tests.rs"].splitlines()) <= 320 and
            len(sources[E + "wasm_native_source_compiler/tests.rs"].splitlines()) <= 160,
            "new_owner_caps", "small caps apply only to the new opaque owner and its dedicated test owners")
    return failures


def main():
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    failures = check({name: (root / name).read_text() for name in FILES})
    for failure in failures:
        print("ABI8 runtime profile: " + failure["invariant"] + ": " + failure["reason"], file=sys.stderr)
    if not failures:
        print("ABI8 validated-runtime-profile source controls passed")
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
