use super::*;

#[test]
fn large_static_regexp_programs_keep_literal_and_constructor_bodies_bounded() {
    let program = scriptw_ir::RegExpProgram::compile(r"^\p{RGI_Emoji}$", "iv").unwrap();
    let descriptor = scriptw_ir::ValidatedRegExpProgram::from_program(&program).unwrap();
    assert!(
        descriptor.bytes().len() > 256 * 1024,
        "the regression must exercise a large finite matcher"
    );
    for source in [
        r"/^\p{RGI_Emoji}$/iv;",
        r#"new RegExp("^\\p{RGI_Emoji}$", "iv");"#,
    ] {
        let artifact = emit_script(source).expect("large static RegExp should emit");
        let data = data_segment_at(
            &artifact.bytes,
            StringPool::PROGRAM_STATIC_DATA_SEGMENT as usize,
        );
        assert_eq!(
            data.windows(descriptor.bytes().len())
                .filter(|bytes| *bytes == descriptor.bytes())
                .count(),
            1,
            "the exact validated matcher belongs once in P metadata: {source}"
        );
        let mut functions = 0;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            let Payload::CodeSectionEntry(body) = payload.expect("P should parse") else {
                continue;
            };
            functions += 1;
            assert!(
                body.range().len() < 64 * 1024,
                "a static matcher must not become per-byte code or a giant live operand stack: {} bytes for {source}",
                body.range().len()
            );
        }
        assert!(functions > 0);
        expect_valid_module(&artifact, 0);
    }
}
