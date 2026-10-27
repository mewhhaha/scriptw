//! Encoded function bodies of a linked artifact.
//!
//! A heap program is a runtime module (R) holding every builtin and runtime
//! helper, and a small program module (P) holding `main` and the script's own
//! bodies. Function indices belong to their individual module. Resolve P's
//! compact imports through R's actual exports before comparing cross-module
//! call edges. `WasmArtifact::function_sizes` describes
//! only P; these readers decode the encoded modules themselves.

// Each test target reads a different subset of this file.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use scriptw_aot_wasm::WasmArtifact;
use wasmparser::{FunctionBody, KnownCustom, Name, Operator, Parser, Payload, TypeRef};

pub struct EncodedBody<'a> {
    pub index: u32,
    pub name: String,
    body: FunctionBody<'a>,
}

impl EncodedBody<'_> {
    pub fn bytes(&self) -> usize {
        self.body.range().len()
    }

    /// Direct callees, including tail calls.
    pub fn calls(&self) -> BTreeSet<u32> {
        self.body
            .get_operators_reader()
            .expect("body opens")
            .into_iter()
            .filter_map(|operator| match operator.expect("operator decodes") {
                Operator::Call { function_index } | Operator::ReturnCall { function_index } => {
                    Some(function_index)
                }
                _ => None,
            })
            .collect()
    }
}

/// The defined bodies of one module, named by its `name` section.
pub fn module_bodies(bytes: &[u8]) -> Vec<EncodedBody<'_>> {
    let mut names = BTreeMap::new();
    let mut next = 0u32;
    let mut bodies = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("module decodes") {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    if matches!(
                        import.expect("import decodes").ty,
                        TypeRef::Func(_) | TypeRef::FuncExact(_)
                    ) {
                        next += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                bodies.push((next, body));
                next += 1;
            }
            Payload::CustomSection(section) => {
                if let KnownCustom::Name(subsections) = section.as_known() {
                    for subsection in subsections {
                        if let Name::Function(map) = subsection.expect("name subsection decodes") {
                            for naming in map {
                                let naming = naming.expect("function naming decodes");
                                names.insert(naming.index, naming.name.to_owned());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    bodies
        .into_iter()
        .map(|(index, body)| EncodedBody {
            index,
            name: names
                .remove(&index)
                .unwrap_or_else(|| panic!("function {index} is unnamed")),
            body,
        })
        .collect()
}

/// R and P of one heap program.
pub struct LinkedBodies<'a> {
    pub runtime: Vec<EncodedBody<'a>>,
    pub program: Vec<EncodedBody<'a>>,
}

/// Actual P function-import index -> actual R exported function index.
pub fn runtime_function_imports(program: &[u8], runtime: &[u8]) -> BTreeMap<u32, u32> {
    let mut exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(runtime) {
        if let Payload::ExportSection(reader) = payload.expect("R decodes") {
            for export in reader {
                let export = export.expect("R export decodes");
                if export.kind == wasmparser::ExternalKind::Func {
                    assert!(exports.insert(export.name, export.index).is_none());
                }
            }
        }
    }
    let mut result = BTreeMap::new();
    let mut index = 0;
    for payload in Parser::new(0).parse_all(program) {
        if let Payload::ImportSection(reader) = payload.expect("P decodes") {
            for import in reader.into_imports() {
                let import = import.expect("P import decodes");
                if matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                    if import.module == scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE {
                        let target = *exports.get(import.name).expect("P imports a real R export");
                        assert!(result.insert(index, target).is_none());
                    }
                    index += 1;
                }
            }
        }
    }
    result
}

pub fn linked_bodies(artifact: &WasmArtifact) -> LinkedBodies<'_> {
    LinkedBodies {
        runtime: module_bodies(artifact.runtime().expect("heap program links R").bytes()),
        program: module_bodies(&artifact.bytes),
    }
}

impl<'a> LinkedBodies<'a> {
    pub fn all(&self) -> impl Iterator<Item = &EncodedBody<'a>> {
        self.runtime.iter().chain(&self.program)
    }

    /// The one body called `name` in R and P together.
    pub fn unique(&self, name: &str) -> &EncodedBody<'a> {
        let mut matches = self.all().filter(|body| body.name == name);
        let body = matches
            .next()
            .unwrap_or_else(|| panic!("missing emitted body: {name}"));
        assert!(
            matches.next().is_none(),
            "{name} must have exactly one emitted body"
        );
        body
    }
}
