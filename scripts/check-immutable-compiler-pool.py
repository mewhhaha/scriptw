#!/usr/bin/env python3
"""Source ownership controls for exact-R immutable compiler-pool reuse."""
from pathlib import Path
import re
import sys

A = 'crates/scriptw-aot-wasm/src/'
FILES = tuple(A + path for path in (
    'data.rs', 'data/shared_pool_storage.rs', 'data/compiler_owned_pool.rs',
    'data/compiler_owned_pool/tests.rs', 'runtime_artifact.rs',
    'runtime_artifact/validated_profile.rs', 'runtime_artifact/validated_profile/tests.rs',
    'runtime_artifact/validated_profile/pool_tests.rs',
    'runtime_artifact/build_package/metadata.rs',
    'data/intl_supported_values.rs', 'data/regexp_unicode_properties.rs',
    'data/temporal_east_asian_years.rs', 'emit/module_assembly.rs',
    'emit/module_assembly/program_entries.rs', 'emit/module_assembly/native_program.rs',
    'runtime_function.rs', 'runtime_global_script.rs', 'runtime_direct_eval.rs',
))

def compact(source):
    return re.sub(r'\s+', '', re.sub(r'//[^\n]*', '', source))

def check(root, overrides=None):
    sources = {p: (overrides[p] if overrides and p in overrides else (Path(root) / p).read_text()) for p in FILES}
    c = {p: compact(text) for p, text in sources.items()}
    failures = []
    def require(ok, name):
        if not ok:
            failures.append(name)
    runtime = c[A + 'runtime_artifact.rs']
    proof = c[A + 'runtime_artifact/validated_profile.rs']
    data = c[A + 'data.rs']
    pool = c[A + 'data/compiler_owned_pool.rs']
    storage = c[A + 'data/shared_pool_storage.rs']
    require('compiler_owned_pool:OnceLock<Result<Arc<crate::data::CompilerOwnedPool>,EmitError>>' in runtime and
            runtime.count('compiler_owned_pool:OnceLock::new()') == 2,
            'one bounded immutable prefix cell on each actual runtime owner')
    require('implPartialEqforRuntimeArtifact{fneq(&self,other:&Self)->bool{self.key==other.key}}' in runtime and
            '.field("bytes",&self.bytes.len())' in runtime and
            '.field("compiler_owned_pool"' not in runtime,
            'logical runtime equality and debug identity exclude lazy memo state')
    require('self.runtime.compiler_owned_pool.get_or_init(||{' in proof and
            'CompilerOwnedPool::for_runtime(&self.runtime,functions,builtins,temporal,&self.selection,)' in proof and
            '.map(Arc::new)' in proof and '.as_ref().map_err(Clone::clone)' in proof,
            'only retained exact-R/profile authority initializes the one-time pool')
    require('pub(crate)structCompilerOwnedPool(StringPool);' in data and
            'shared_prefix:Option<std::sync::Arc<CompilerOwnedPool>>' in data and
            'letprefix=validated.compiler_owned_pool(functions,builtins,temporal)?;' in pool and
            'CompilerOwnedPool::collect_program(prefix,script,functions)' in pool,
            'opaque checked prefix starts every reused program pool')
    require(('runtime.layout().require_pool(&prefix.0)?;'
             'ifletSome(matches)=runtime.admitted_compiler_pool_matches('
             '&prefix.0.pooled_code_unit_bytes[..],&prefix.0.bytes[..],){'
             'if!matches{returnErr(EmitError::unsupported('
             '"canonicalcompilerpooldiffersfromitsimmutableproducerreceipt",));}}'
             'else{prefix.require_runtime_data(runtime.bytes())?;}'
             'prefix.0.bytes.freeze();prefix.0.pooled_code_unit_bytes.freeze();prefix.0.refs.freeze();') in pool,
            'actual runtime boundary and bytes precede prefix freezing')
    receipt = c[A + 'runtime_artifact/build_package/metadata.rs']
    require('Some(self.build_admission.as_ref()?.matches_pool(utf16,data))' in runtime and
            'data:[SpanDigest;6]' in receipt and
            'forindexin[3,2,4,1,5]' in receipt and
            'cursor.checked_add(self.data[index].span.len())' in receipt and
            'data.get(cursor..end)' in receipt and
            'ifcursor!=data.len(){returnfalse;}' in receipt and
            ('parts.into_iter().zip(&self.data).all(|(bytes,expected)|{'
             'bytes.len()==expected.span.len()&&Sha256::digest(bytes).as_slice()==expected.digest})') in receipt,
            'immutable pool proof checks both actual reconstructed image dimensions and hashes')
    require('section.count()!=6' in pool and
            'letregion=self.0.normalization_image_region();letspan=region.offsets();' in pool and
            '(&self.0.pooled_code_unit_bytes[..],None)' in pool and
            '(&self.0.bytes[span.clone()],None)' in pool and
            '(&self.0.bytes[property.clone()],None)' in pool and
            '(&self.0.bytes[..property.start],Some(STATIC_DATA_OFFSET))' in pool and
            '(&self.0.bytes[property.end..span.start],Some(properties.end()),)'  in pool and
            '(&self.0.bytes[span.end..],Some(region.end()))' in pool and
            'for(row,(bytes,address))insection.into_iter().zip(expected)' in pool and
            'ifrow.data!=bytes' in pool and
            '(DataKind::Passive,None)=>{}' in pool and
            re.search(r'DataKind::Active\{memory_index:0,offset_expr,?\}', pool) and
            'Operator::I32Const{value}ifvalue==addressasi32' in pool and
            'Operator::End' in pool and '!ops.eof()' in pool,
            'same-sized foreign data or offset cannot certify the canonical R prefix')
    require('prefix:Arc<[u8]>,appended:Vec<u8>' in storage and
            'prefix:Arc<BTreeMap<String,StringRef>>,appended:BTreeMap<String,StringRef>' in storage and
            'Arc::from(std::mem::take(&mutself.appended))' in storage and
            'Arc::new(std::mem::take(&mutself.appended))' in storage and
            'prefix:Arc::clone(&self.prefix),appended:Vec::new()' in storage and
            'prefix:Arc::clone(&self.prefix),appended:BTreeMap::new()' in storage and
            storage.count('self.prefix.len()+self.appended.len()') == 2,
            'shared images/maps move once and each P starts with empty appendices')
    start = pool.index('fnfrom_shared_prefix(')
    end = pool.index('pub(crate)fncollect_with_validated_runtime(', start)
    overlay = pool[start:end]
    require('bytes:base.bytes.program_overlay()' in overlay and
            'pooled_code_unit_bytes:base.pooled_code_unit_bytes.program_overlay()' in overlay and
            'refs:base.refs.program_overlay()' in overlay and
            not any(token in overlay for token in ('.clone()', '.to_vec()', 'Sha256', 'Self::collect(')),
            'per-P initialization neither clones nor hashes/recollects the prefix')
    require('self.prefix.get(value).or_else(||self.appended.get(value))' in storage and
            '!self.prefix.contains_key(&value)' in storage and
            'length>=self.prefix.len()' in storage and
            'start>=self.prefix.len()' in storage and
            'PooledStringIndex::for_insertion(self.refs.len())' in data,
            'runtime ordinals and virtual static/UTF16 bases remain immutable')
    require('self.refs.program_values().filter(move|reference|reference.index.ordinal()>=boundary.strings)' in data and
            'self.program_utf16_bytes().iter().copied()' in data and
            'self.program_metadata_bytes().iter().copied()' in data and
            'self.program_recipe_bytes().iter().copied()' in data,
            'program publication visits only P-local literals and byte images')
    for relative in ('data/intl_supported_values.rs','data/regexp_unicode_properties.rs','data/temporal_east_asian_years.rs'):
        require('self.runtime_prefix()' in c[A + relative], 'immutable metadata fallback: ' + relative)
    catalogue = c[A + 'data/intl_supported_values.rs']
    require('self.runtime_prefix().and_then(|prefix|prefix.intl_supported_values.as_ref())' in catalogue and
            'self.runtime_prefix().map_or(true,|prefix|prefix.intl_supported_values.is_none())' in catalogue,
            'both catalogue access and availability retain the selected immutable prefix')
    require('prefix.regexp_programs.get(&key)' in data and
            'prefix.regexp_case_folding_tables.get(&folding)' in data and
            'prefix.intl_default_locale.as_ref()' in data,
            'R-owned regexp and selected locale metadata remain available without map clones')
    assembly = c[A + 'emit/module_assembly.rs']
    require('match(kind,validated_runtime)' in assembly and
            'std::sync::Arc::ptr_eq(runtime,validated.runtime())' in assembly and
            'StringPool::collect_with_validated_runtime(' in assembly and
            '(_,None)=>StringPool::collect(' in assembly,
            'native proof binds exact R while ordinary checked collection remains')
    for kind in ('function','global_script','direct_eval'):
        source = c[A + 'runtime_' + kind + '.rs']
        require('emit_runtime_'+kind+'_module(program,validated)?' in source and
                'ValidatedRuntimeArtifact::new(Arc::clone(runtime),intl_profile.clone())?' in source and
                'validated.require_linked_program(&emitted,' in source and
                'runtime:Arc::clone(runtime)' in source,
                kind + ' keeps checked raw API and exact linked R')
    require(('pub(crate)fnrequire_linked_program(&self,emitted:&super::EmittedModule,'
             "diagnostic:&'staticstr,)->Result<(),EmitError>{"
             'ifemitted.runtime_layout.is_some()||!emitted.artifact.runtime().is_some_and('
             '|actual|Arc::ptr_eq(actual,self.runtime())){'
             'returnErr(EmitError::unsupported(diagnostic));}Ok(())}') in proof,
            'shared linkage owner rejects a new layout or a different runtime Arc')
    entries = c[A + 'emit/module_assembly/program_entries.rs']
    native = c[A + 'emit/module_assembly/native_program.rs']
    require(entries.count('Some(validated)') == 1 and native.count('Some(validated)') == 2 and
            entries.count('ModuleKind::Program(validated.runtime())') == 1 and
            native.count('ModuleKind::Program(validated.runtime())') == 2,
            'all three actual native factories use the same retained pool proof')
    tests = c[A + 'data/compiler_owned_pool/tests.rs']
    require('Arc::ptr_eq(pool.shared_prefix.as_ref().unwrap(),&prefix)' in tests and
            'std::ptr::eq(pool.refs.get("undefined").unwrap(),prefix.0.refs.get("undefined").unwrap())' in tests and
            'assert_eq!(shared_module.finish(),fresh_module.finish())' in tests and
            'letdescriptors=prefix.0.runtime_string_descriptors().unwrap();' in tests and
            'descriptor_payload_byte(&wrong_bytes,descriptors.address())' in tests and
            'letpayload_start=row.range.end-row.data.len();' in tests and
            'address.checked_sub(valueasu32)' in tests and
            'wrong_bytes[descriptor_byte]^=1' in tests and
            'assert!(prefix.require_runtime_data(&wrong_bytes).is_err())' in tests,
            'authored controls cover real sharing, P isolation, exact images and corrupted R data')
    require(c[A + 'runtime_artifact/validated_profile/tests.rs'].count('Arc::ptr_eq(&prefix,runtime.compiler_owned_pool.get().unwrap().as_ref().unwrap())') == 3,
            'two proofs and all three entry kinds retain the one R-owned prefix')
    identity_test = c[A + 'runtime_artifact/validated_profile/pool_tests.rs']
    require('assert!(cold.compiler_owned_pool.get().is_none())' in identity_test and
            'cold.key(),cold.wasm_sha256(),format!("{cold:?}")' in identity_test and
            'assert_eq!(actual.as_ref(),cold.as_ref())' in identity_test and
            'Arc::ptr_eq(&memo,cold.compiler_owned_pool.get().unwrap().as_ref().unwrap())' in identity_test,
            'cold/warm cache controls preserve logical identity and exact owner reuse')
    require(not any(token in pool for token in ('staticRUNTIMES','Mutex<BTreeMap','HashMap<','unsafe{')),
            'prefix lifetime is bounded by exact R rather than an ambient global cache')
    return failures

def main():
    failures = check(Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd())
    for reason in failures:
        print('immutable compiler pool: ' + reason, file=sys.stderr)
    if not failures:
        print('immutable compiler pool source controls passed')
    return bool(failures)

if __name__ == '__main__':
    raise SystemExit(main())
