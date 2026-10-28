//! Verify actual R publication code, rather than an installer source spelling.
use std::collections::BTreeMap;
use wasmparser::{KnownCustom, Name, Operator, Parser, Payload, TypeRef};

#[test]
fn deferred_method_descriptors_are_fresh_in_one_shared_publication_body() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let source = scriptw_front::parse(
                "[Set.prototype.keys, Set.prototype.values, Math.abs, RegExp.lastMatch];",
                scriptw_front::ParseOptions::script(),
            ).unwrap();
            let artifact = crate::emit_with_rooted_snapshot(
                &scriptw_ir::lower(&source),
                crate::PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            ).unwrap();
            let bytes = artifact.runtime().expect("fixture links its real R").bytes();
            wasmparser::Validator::new_with_features(crate::runtime_artifact::program_validation_features())
                .validate_all(bytes).expect("shared publication has valid typed operands and roots");
            let mut next=0;
            let mut names=BTreeMap::new();
            let mut bodies=BTreeMap::new();
            for payload in Parser::new(0).parse_all(bytes) {
                match payload.unwrap() {
                    Payload::ImportSection(reader)=>for import in reader.into_imports() {
                        if matches!(import.unwrap().ty, TypeRef::Func(_)|TypeRef::FuncExact(_)) {next+=1;}
                    },
                    Payload::CodeSectionEntry(body)=>{
                        bodies.insert(next,body.get_operators_reader().unwrap().into_iter().map(Result::unwrap).collect::<Vec<_>>());next+=1;
                    },
                    Payload::CustomSection(section)=>if let KnownCustom::Name(reader)=section.as_known() {
                        for subsection in reader {if let Name::Function(reader)=subsection.unwrap(){for row in reader{let row=row.unwrap();names.insert(row.name.to_owned(),row.index);}}}
                    },
                    _=>{},
                }
            }
            let publish=names["helper::intrinsic_data_publish"];
            let accessor=names["helper::intrinsic_accessor_publish"];
            assert_ne!(publish,accessor,"data/accessor flag domains have distinct typed owners");
            let append=names["helper::ordinary_property_append"];
            let resolve=names["helper::property_value_resolve"];
            for publish in [publish,accessor] {
            let body=&bodies[&publish];
            let allocations=body.iter().enumerate().filter_map(|(i,op)|matches!(op,Operator::StructNew{..}).then_some(i)).collect::<Vec<_>>();
            assert_eq!(allocations.len(),1,"every helper invocation constructs its own mutable descriptor; each recipe retains its installation ownership");
            let appends=body.iter().enumerate().filter_map(|(i,op)|matches!(op,Operator::Call{function_index} if *function_index==append).then_some(i)).collect::<Vec<_>>();
            assert_eq!(appends.len(),1,"retain the existing insertion/storage algorithm");
            assert!(allocations[0]<appends[0],"descriptor exists and is rooted before append can grow storage");
            assert!(!body.iter().any(|op|matches!(op,Operator::Call{function_index} if *function_index==resolve)),"publishing a recipe must not observe/materialize its function value");
            let callers=bodies.iter().filter(|(index,_)|**index!=publish).map(|(_,body)|body.iter().filter(|op|matches!(op,Operator::Call{function_index} if *function_index==publish)).count()).sum::<usize>();
            assert!(callers>1,"multiple actual R installations share this physical descriptor publication owner");
            // The concrete descriptor receives a PropertyValue reference, not a
            // second boxed snapshot or eager FunctionObject allocation.
            assert!(!body.iter().any(|op|matches!(op,Operator::ArrayNew{..}|Operator::ArrayNewDefault{..}|Operator::ArrayNewFixed{..})),"publication does not clone or allocate a value/alias table");
            }
        }).unwrap().join().unwrap();
}
