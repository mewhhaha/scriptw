#!/usr/bin/env python3
"""Realm-owned host prototype and checked ABI9 ownership. Source checks only."""
from pathlib import Path
import re
import sys

A = "crates/scriptw-aot-wasm/src/"
E = "crates/scriptw-engine/tests/aot_module_sources.rs"


def check(root: Path, overrides=None):
    overrides = overrides or {}
    failures = []
    def source(relative):
        return overrides[relative] if relative in overrides else (root / relative).read_text()
    def need(ok, reason):
        if not ok:
            failures.append(reason)
    def compact(relative):
        return re.sub(r"\s+", "", re.sub(r"//[^\n]*", "", source(relative)))
    slots = compact(A + "functions.rs")
    # Pinned to the current table (179 entries since 40ff4b4ba): a resize must
    # revisit slot 158's allocation and update this number.
    need("AbstractModuleSourcePrototype,ModuleSourcePrototype," in slots and
         "Self::ModuleSourcePrototype=>158," in slots and
         "constTABLE_LENGTH:u32=179;" in slots,
         "closed host prototype slot has its own allocated entry")
    indexes = re.findall(r"Self::\w+=>158,", slots)
    need(indexes == ["Self::ModuleSourcePrototype=>158,"], "host prototype slot has no intrinsic alias")
    need("NonArrayRealmIntrinsicSlot::TABLE_LENGTH+intl_fallback_symbol::PRIVATE_REALM_INTRINSIC_COUNT" in slots and
         "NonArrayRealmIntrinsicSlot::TABLE_LENGTH" in compact(A + "functions/intl_fallback_symbol.rs"),
         "private Intl symbol remains after the enlarged object intrinsic table")
    init = compact(A + "intrinsics/abstract_module_source.rs")
    # e03f584d0 builds the abstract constructor on first read; its prototype is
    # one complete eager layout whose accessor rows replace the explicit install.
    steps = ["self.emit_alloc_intrinsic_layout_header(",
             "IntrinsicPropertyLayoutId::AbstractModuleSourcePrototype,",
             "NonArrayRealmIntrinsicSlot::AbstractModuleSourcePrototype,",
             "letsource_prototype=",
             "self.emit_alloc_plain_object_with_prototype(Some(&prototype),function)?",
             "prototype.set_reference(&source_prototype,schema,function);",
             "self.emit_store_non_array_realm_intrinsic(realm.realm(),NonArrayRealmIntrinsicSlot::ModuleSourcePrototype,&prototype,function,);"]
    position = -1
    for step in steps:
        position = init.find(step, position + 1)
        need(position >= 0, "Realm bootstrap publishes abstract intrinsic then its stable host child: " + step)
        if position < 0:
            break
    need(init.count("NonArrayRealmIntrinsicSlot::ModuleSourcePrototype,") == 1,
         "one defining-Realm bootstrap owner publishes the host prototype")
    # The eager layout row is the accessor install the header allocation replaced.
    eager = compact(A + "intrinsics/property_layouts/eager.rs")
    need(eager.count("Layout::AbstractModuleSourcePrototype=>{plan.constructor(Slot::AbstractModuleSourceConstructor)?;"
                     "getter(plan,IntrinsicKey::Symbol(WellKnownSymbol::ToStringTag),"
                     "Builtin::AbstractModuleSourcePrototypeToStringTagGetter,)?;}") == 1 and
         eager.count("Slot::AbstractModuleSourcePrototype=>Some(Layout::AbstractModuleSourcePrototype),") == 1,
         "abstract prototype layout carries its constructor row and @@toStringTag getter")
    cell = compact(A + "modules/source.rs")
    need(".field(ModuleRecordSchema::REALM)" in cell and
         "self.emit_load_non_array_realm_intrinsic(&realm,NonArrayRealmIntrinsicSlot::ModuleSourcePrototype,&prototype,function,);" in cell and
         "self.emit_alloc_plain_object_with_prototype(Some(&prototype),function)?" in cell and
         "NonArrayRealmIntrinsicSlot::AbstractModuleSourcePrototype" not in cell,
         "canonical source cell uses its record Realm's host prototype")
    need(".field(ModuleRecordSchema::SOURCE_CELL)" in cell and "Instruction::I32Eqz" in cell and
         "schema.struct_type::<ModuleSourceObject>().construct((" in cell and
         "Instruction::RefTestNonNull(" in cell and ".field(ModuleSourceObjectSchema::SOURCE_TAG)" in cell,
         "source-cell identity and getter's private GC brand remain intact")
    host = compact(A + "builtins/host/abstract_module_source.rs")
    need("self.emit_current_function_realm(function)" in host and
         "NonArrayRealmIntrinsicSlot::AbstractModuleSourceConstructor" in host,
         "Test262 getter exposes the actual defining-Realm abstract constructor")
    need(compact(A + "builtins/host/create_realm.rs").count("NonArrayRealmIntrinsicSlot::AbstractModuleSourceConstructor,") == 2,
         "created-Realm facades retain that Realm's actual abstract intrinsic")
    runtime = compact(A + "runtime_artifact.rs")
    need((current_layout := re.search(r'constLAYOUT_VERSION:u32=(\d+);', runtime)) is not None and int(current_layout.group(1)) >= 15 and
         "(encoded==LAYOUT_VERSION).then_some(Self(encoded))" in runtime,
         "layout9 rejects old158-entry R8 before linking new slot consumers")
    proof = compact(A + "runtime_artifact/validated_profile.rs")
    need("!runtime.layout_admission.is_some_and(|version|version.0==LAYOUT_VERSION)" in proof,
         "opaque native runtime proof requires current admitted layout")
    for relative, token in [("runtime_artifact/cache.rs", "CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(take(&mutpayload)?))?"),
                            ("runtime_artifact/build_package/envelope.rs", "CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(cache::take(&mutpayload)?))?")]:
        need(token in compact(A + relative), "authenticated current layout checked by " + relative)
    for relative, helper in [("runtime_artifact/cache/tests.rs", "assert_previous_runtime_cache_entry_replaced"),
                             ("runtime_artifact/build_package/tests.rs", "assert_previous_build_package_replaced")]:
        control = compact(A + relative)
        need("letprevious_version=7u32;" in control and "letprevious_version=8u32;" in control and
             helper + "(previous_version);" in control and "assert_ne!(LAYOUT_VERSION,previous_version);" in control and
             "previous[MAGIC.len()..MAGIC.len()+4].copy_from_slice(&previous_version.to_le_bytes());" in control,
             "authenticated ABI7 and ABI8 negative controls retained: " + relative)
    admission_tests = compact(A + "runtime_artifact/validated_profile/tests.rs")
    need("forwrongin(0..LAYOUT_VERSION).chain([LAYOUT_VERSION+1,u32::MAX])" in admission_tests and
         "CurrentRuntimeLayoutVersion::admit(wrong).is_none()" in admission_tests,
         "native proof rejects every older layout and invalid future versions")
    controls = source(E)
    for text in ["Object.getPrototypeOf(sourcePrototype) === C.prototype", "Object.getPrototypeOf(other) === sourcePrototype",
                 "tag.call(sourcePrototype) === undefined", "host prototype crossed Realms",
                 '#[path = "aot_module_sources/prototype_chain.rs"]', "mod prototype_chain;"]:
        need(text in controls, "actual identity/brand/Realm regression control: " + text)
    pinned = source("crates/scriptw-engine/tests/aot_module_sources/prototype_chain.rs")
    for text in ["module-source-prototype-chain.js", "EmbeddedModuleGoal::Script", "__scriptwGetAbstractModuleSource()",
                 'for directive in ["",', "use strict", "observed.output_events", "source-prototype:ok", "asyncHelpers.js"]:
        need(text in pinned, "pinned original fixture runs through both real Script modes: " + text)
    return failures


if __name__ == "__main__":
    errors = check(Path(sys.argv[1]) if len(sys.argv) > 1 else Path("."))
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        raise SystemExit(1)
    print("ModuleSource Realm prototype/ABI10 source controls passed")
