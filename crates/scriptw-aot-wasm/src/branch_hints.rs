//! The branch hinting section of an emitted module.
//!
//! An emitter marks a two-way branch where it emits it
//! (`code_sink::Function::unlikely` and `likely`), or later by the ordinal
//! of the branch among its body's operators. The code sink keeps the byte
//! offset of every two-way branch it encodes and gives each hinted one its
//! offset when the body is finished, after the body's local declaration is
//! final. Import compaction shortens index operands and moves the offsets
//! after them by what it removed ([`BodySites::compact`]). The section,
//! `metadata.code.branch_hint` of the WebAssembly branch hinting proposal,
//! names each branch by its byte offset from the start of the body's local
//! declarations; it is written last, after the final bytes, and placed
//! before the code section as the proposal requires. Writing it decodes no
//! operator: it checks that each offset holds a two-way branch opcode.
//!
//! The proposal hints `if` and `br_if`. The pinned Wasmtime translation
//! reads the same record at `br_on_null`, `br_on_non_null`, `br_on_cast` and
//! `br_on_cast_fail` too, so the section carries those as well; a consumer
//! that follows the proposal alone never looks at their offsets. A hint
//! changes where native code is placed, never what it computes.
use crate::module::EmitError;
use wasmparser::{Parser, Payload, TypeRef};

pub(crate) const SECTION_NAME: &str = "metadata.code.branch_hint";
const CODE_SECTION_ID: u8 = 10;

/// One hinted branch of a finished body: its byte offset in the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FunctionSite {
    offset: u32,
    likely: bool,
}

impl FunctionSite {
    /// Only the code sink mints a site, as it finishes the body.
    pub(crate) fn new(offset: u32, likely: bool) -> Self {
        Self { offset, likely }
    }
}

/// The hinted branches of one code-section entry, in operator order.
#[derive(Clone, Debug)]
pub(crate) struct BodySites {
    body: u32,
    sites: Vec<FunctionSite>,
}

impl BodySites {
    pub(crate) fn new(body: u32, sites: Vec<FunctionSite>) -> Option<Self> {
        (!sites.is_empty()).then_some(Self { body, sites })
    }

    pub(crate) fn body(&self) -> u32 {
        self.body
    }

    /// Follow import compaction of this body: `removed` gives, in body
    /// order, the offset of each rewritten operand and how many bytes its
    /// rewriting removed. Every byte removed before a branch moves it.
    pub(crate) fn compact(&mut self, removed: impl IntoIterator<Item = (u32, u32)>) {
        let mut removed = removed.into_iter().peekable();
        let mut shift = 0;
        for site in &mut self.sites {
            while let Some((_, bytes)) = removed.next_if(|(offset, _)| *offset < site.offset) {
                shift += bytes;
            }
            site.offset -= shift;
        }
    }
}

impl BodySites {
    /// Follow a re-encoding of this body that kept its operators but may
    /// have changed operand lengths: `moved` gives, in body order, the old
    /// and new offset of every two-way branch of the body.
    pub(crate) fn relocate(&mut self, moved: &[(u32, u32)]) -> Result<(), EmitError> {
        for site in &mut self.sites {
            let at = moved
                .binary_search_by_key(&site.offset, |&(old, _)| old)
                .map_err(|_| {
                    fail(format!(
                        "offset {} of body {} names no two-way branch to relocate",
                        site.offset, self.body
                    ))
                })?;
            site.offset = moved[at].1;
        }
        Ok(())
    }
}

/// Whether the operator at the start of `code` is a two-way branch.
fn starts_two_way_branch(code: &[u8]) -> bool {
    match code {
        [0x04 | 0x0D | 0xD5 | 0xD6, ..] => true,
        // br_on_cast and br_on_cast_fail.
        [0xFB, 0x18 | 0x19, ..] => true,
        _ => false,
    }
}

fn fail(message: impl std::fmt::Display) -> EmitError {
    EmitError::unsupported(format!(
        "compiler invariant: branch hint section: {message}"
    ))
}

fn leb(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// `bytes` with the branch hinting section for `bodies` before its code
/// section. Every site must name a two-way branch of its final body.
pub(crate) fn insert(bytes: Vec<u8>, bodies: &[BodySites]) -> Result<Vec<u8>, EmitError> {
    if bodies.is_empty() {
        return Ok(bytes);
    }
    let mut imported = 0u32;
    let mut code_start = None;
    let mut body = 0u32;
    let mut pending = bodies.iter().peekable();
    let mut functions = Vec::new();
    let mut count = 0u32;
    for payload in Parser::new(0).parse_all(&bytes) {
        match payload.map_err(fail)? {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    if matches!(
                        import.map_err(fail)?.ty,
                        TypeRef::Func(_) | TypeRef::FuncExact(_)
                    ) {
                        imported += 1;
                    }
                }
            }
            Payload::CodeSectionStart { range, .. } => {
                // The section's id and size precede its contents.
                let mut size = Vec::new();
                leb(u32::try_from(range.len()).map_err(fail)?, &mut size);
                let header = range
                    .start
                    .checked_sub(size.len() + 1)
                    .filter(|header| {
                        bytes[*header] == CODE_SECTION_ID && bytes[header + 1..range.start] == size
                    })
                    .ok_or_else(|| fail("code section header is not canonical"))?;
                code_start = Some(header);
            }
            Payload::CodeSectionEntry(entry) => {
                let ordinal = body;
                body += 1;
                let Some(sites) = pending.next_if(|sites| sites.body == ordinal) else {
                    continue;
                };
                let code = &bytes[entry.range()];
                leb(imported + ordinal, &mut functions);
                leb(
                    u32::try_from(sites.sites.len()).map_err(fail)?,
                    &mut functions,
                );
                let mut previous = None;
                for site in &sites.sites {
                    if previous.is_some_and(|previous| site.offset <= previous) {
                        return Err(fail("sites of a body are not in body order"));
                    }
                    previous = Some(site.offset);
                    if !code
                        .get(site.offset as usize..)
                        .is_some_and(starts_two_way_branch)
                    {
                        return Err(fail(format!(
                            "offset {} of body {ordinal} is not a two-way branch",
                            site.offset
                        )));
                    }
                    leb(site.offset, &mut functions);
                    functions.push(1);
                    functions.push(u8::from(site.likely));
                }
                count += 1;
            }
            _ => {}
        }
    }
    if pending.next().is_some() {
        return Err(fail("a hinted body is not in the code section"));
    }
    let code_start = code_start.ok_or_else(|| fail("hinted bodies without a code section"))?;
    let mut data = Vec::with_capacity(functions.len() + 5);
    leb(count, &mut data);
    data.extend_from_slice(&functions);
    let mut module = wasm_encoder::Module::new();
    module.section(&wasm_encoder::CustomSection {
        name: std::borrow::Cow::Borrowed(SECTION_NAME),
        data: std::borrow::Cow::Owned(data),
    });
    // `Module` starts with the eight header bytes; the rest is the section.
    let section = &module.as_slice()[8..];
    let mut output = Vec::with_capacity(bytes.len() + section.len());
    output.extend_from_slice(&bytes[..code_start]);
    output.extend_from_slice(section);
    output.extend_from_slice(&bytes[code_start..]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_sink::{Function, LocalDeclarations};
    use wasm_encoder::{
        BlockType, CodeSection, EntityType, FunctionSection, ImportSection, Instruction, Module,
        TypeSection, ValType,
    };

    /// Two imports, then the given bodies of type `(i32) -> ()`.
    fn module(bodies: Vec<Function>) -> (Vec<u8>, Vec<BodySites>) {
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([ValType::I32], []);
        module.section(&types);
        let mut imports = ImportSection::new();
        imports.import("host", "a", EntityType::Function(0));
        imports.import("host", "b", EntityType::Function(0));
        module.section(&imports);
        let mut functions = FunctionSection::new();
        let mut code = CodeSection::new();
        let mut sites = Vec::new();
        for (ordinal, body) in bodies.into_iter().enumerate() {
            functions.function(0);
            let finished = body.finish_named(&"a test body");
            sites.extend(BodySites::new(ordinal as u32, finished.branch_hints));
            code.function(&finished.body);
        }
        module.section(&functions);
        module.section(&code);
        (module.finish(), sites)
    }

    fn body(emit: impl FnOnce(&mut Function)) -> Function {
        let mut function =
            Function::with_parameters(LocalDeclarations::from_types([]), vec![ValType::I32]);
        emit(&mut function);
        function.instruction(&Instruction::End);
        function
    }

    /// `(function index, [(operator at the hinted offset, likely)])` as a
    /// consumer of the proposal reads the section.
    fn read(bytes: &[u8]) -> Vec<(u32, Vec<(String, bool)>)> {
        let mut bodies = Vec::new();
        let mut hints = Vec::new();
        let mut before_code = None;
        let mut imported = 0;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.unwrap() {
                Payload::ImportSection(reader) => imported = reader.count(),
                Payload::CodeSectionStart { .. } => before_code = Some(!hints.is_empty()),
                Payload::CodeSectionEntry(entry) => bodies.push(entry),
                Payload::CustomSection(section) => {
                    if let wasmparser::KnownCustom::BranchHints(reader) = section.as_known() {
                        for function in reader {
                            let function = function.unwrap();
                            hints.push((
                                function.func,
                                function
                                    .hints
                                    .into_iter()
                                    .map(|hint| hint.unwrap())
                                    .map(|hint| (hint.func_offset, hint.taken))
                                    .collect::<Vec<_>>(),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        assert_eq!(before_code, Some(true), "the section precedes the code");
        hints
            .into_iter()
            .map(|(function, hints)| {
                let entry = &bodies[(function - imported) as usize];
                let named = hints
                    .into_iter()
                    .map(|(offset, taken)| {
                        let mut reader = entry.get_operators_reader().unwrap();
                        loop {
                            let at = reader.original_position() - entry.range().start;
                            let operator = reader.read().unwrap();
                            if at == offset as usize {
                                break (format!("{operator:?}"), taken);
                            }
                            assert!(at < offset as usize, "offset {offset} is no operator");
                        }
                    })
                    .collect();
                (function, named)
            })
            .collect()
    }

    #[test]
    fn a_hint_names_its_branch_after_locals_and_bodies_were_added() {
        let first = body(|function| {
            function.instruction(&Instruction::LocalGet(0));
            function.unlikely(&Instruction::If(BlockType::Empty));
            // A local declared after the hinted branch moves every offset.
            let local = function.reserve_typed_local(ValType::I64);
            function.instruction(&Instruction::I64Const(1 << 40));
            function.instruction(&Instruction::LocalSet(local));
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::LocalGet(0));
            function.likely(&Instruction::BrIf(0));
            function.instruction(&Instruction::End);
        });
        let unhinted = body(|function| {
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::End);
        });
        let last = body(|function| {
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::LocalGet(0));
            function.unlikely(&Instruction::BrIf(0));
            function.instruction(&Instruction::End);
        });
        let (bytes, sites) = module(vec![first, unhinted, last]);
        let hinted = insert(bytes.clone(), &sites).unwrap();
        wasmparser::Validator::new().validate_all(&hinted).unwrap();
        assert_eq!(
            read(&hinted),
            [
                (
                    2,
                    vec![
                        ("If { blockty: Empty }".to_owned(), false),
                        ("BrIf { relative_depth: 0 }".to_owned(), true),
                    ]
                ),
                (4, vec![("BrIf { relative_depth: 0 }".to_owned(), false)]),
            ]
        );
        // Nothing but the section was added, in front of the code section.
        let code = Parser::new(0)
            .parse_all(&bytes)
            .find_map(|payload| match payload.unwrap() {
                // This small section's size is one byte.
                Payload::CodeSectionStart { range, .. } => Some(range.start - 2),
                _ => None,
            })
            .unwrap();
        assert_eq!(bytes[code], CODE_SECTION_ID);
        let added = hinted.len() - bytes.len();
        assert_eq!(hinted[..code], bytes[..code]);
        assert_eq!(hinted[code + added..], bytes[code..]);
    }

    #[test]
    fn a_module_without_hints_is_unchanged() {
        let (bytes, sites) = module(vec![body(|function| {
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::End);
        })]);
        assert!(sites.is_empty());
        assert_eq!(insert(bytes.clone(), &sites).unwrap(), bytes);
    }

    #[test]
    fn a_site_that_is_not_a_branch_is_refused() {
        let (bytes, _) = module(vec![body(|function| {
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::End);
        })]);
        let wrong = BodySites::new(0, vec![FunctionSite::new(0, false)]).unwrap();
        assert!(insert(bytes.clone(), &[wrong]).is_err());
        let missing = BodySites::new(1, vec![FunctionSite::new(1, false)]).unwrap();
        assert!(insert(bytes, &[missing]).is_err());
    }

    #[test]
    fn rare_code_hints_the_one_branch_that_decides_it() {
        let function = body(|function| {
            // 1: the consequent is rare, through a block that nothing leaves.
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::BrIf(0));
            function.instruction(&Instruction::End);
            function.rarely_reached();
            function.instruction(&Instruction::End);
            // 8: the alternative is rare.
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::Else);
            function.rarely_reached();
            function.instruction(&Instruction::End);
            // 12: the arm is left before the rare code, which says nothing
            // of the `if`; 14, the branch that falls through to it, is
            // likely taken.
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::BrIf(0));
            function.rarely_reached();
            function.instruction(&Instruction::End);
            // 17: an inner `if` may return, so the rare code after it says
            // nothing of the outer one.
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::Return);
            function.instruction(&Instruction::End);
            function.rarely_reached();
            function.instruction(&Instruction::End);
            // 24: the emitter's own hint stands.
            function.instruction(&Instruction::LocalGet(0));
            function.likely(&Instruction::If(BlockType::Empty));
            function.rarely_reached();
            function.instruction(&Instruction::End);
            // Where all paths meet again, no branch decides.
            function.rarely_reached();
        });
        assert_eq!(
            function.hinted_operators(),
            [(1, false), (8, true), (14, true), (24, true)]
        );
    }

    #[test]
    fn every_two_way_branch_to_a_rare_label_is_unlikely_while_the_label_is_open() {
        let function = body(|function| {
            function.instruction(&Instruction::Block(BlockType::Empty)); // 0
            let rare = function.label_depth();
            function.rarely_taken(rare);
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::BrIf(0)); // 2: to the rare label
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::BrIf(0)); // 5: to the inner block
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::BrIf(1)); // 7: to the rare label
            function.instruction(&Instruction::LocalGet(0));
            function.likely(&Instruction::BrIf(1)); // 9: its emitter knows better
            function.instruction(&Instruction::Br(1)); // 10: reached by falling through 9
            function.instruction(&Instruction::End);
            assert!(function.branches_rarely_to(rare));
            function.instruction(&Instruction::End);
            // A sibling at the same depth is another label.
            function.instruction(&Instruction::Block(BlockType::Empty));
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::BrIf(0));
            function.instruction(&Instruction::End);
        });
        assert_eq!(
            function.hinted_operators(),
            [(2, false), (7, false), (9, true)]
        );
    }

    #[test]
    #[should_panic(expected = "only a two-way branch takes a hint")]
    fn the_sink_hints_only_two_way_branches() {
        body(|function| {
            function.unlikely(&Instruction::Br(0));
        });
    }
}
