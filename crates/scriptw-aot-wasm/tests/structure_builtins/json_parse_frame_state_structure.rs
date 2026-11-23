const JSON_SOURCE: &str = include_str!("../../src/builtins/json.rs");
const PARSE_SOURCE: &str = include_str!("../../src/builtins/json/parse.rs");
const FRAME_SOURCE: &str = include_str!("../../src/builtins/json/parse_frame_state.rs");
const SCHEMA: &str = include_str!("../../src/gc_types/layouts.rs");
const VALUE: &str = include_str!("../../src/gc_types/value.rs");

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}
fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end}"))
        .0
}

#[test]
fn parser_frame_state_has_one_closed_seven_word_domain() {
    let domain = compact(between(
        JSON_SOURCE,
        "json_domain!(JsonParseFrameState {",
        "});",
    ));
    for (variant, word) in [
        ("ArrayFirstOrEnd", 0),
        ("ArrayValue", 1),
        ("ArrayCommaOrEnd", 2),
        ("ObjectFirstKeyOrEnd", 3),
        ("ObjectKey", 4),
        ("ObjectValue", 5),
        ("ObjectCommaOrEnd", 6),
    ] {
        assert_eq!(domain.matches(&format!("{variant}={word},")).count(), 1);
    }
    // The key state consumes its colon, so no state awaits one alone.
    assert_eq!(domain.matches('=').count(), 7);
    let macro_source = compact(between(
        JSON_SOURCE,
        "macro_rules! json_domain {",
        "json_domain!(JsonParseFrameState",
    ));
    assert!(macro_source.contains("pub(crate)constALL:&'static[Self]=&[$(Self::$variant),+];"));
    assert!(macro_source.contains("matchself{$(Self::$variant=>$word),+}"));
    assert!(macro_source.contains("assert!($name::ALL[index].wire_code()==indexasi32);"));
    assert!(!macro_source.contains("_=>"));
}

#[test]
fn persisted_state_is_a_closed_gc_schema_field() {
    // There is no externally decoded heap word: typed operands constrain every
    // constructor/write before Wasm emission, and GC retains the actual frame.
    let frame = compact(between(
        SCHEMA,
        "struct JsonParseFrame => JsonParseFrameSchema {",
        "struct JsonReviverFrame",
    ));
    assert!(frame.contains("STATE:crate::builtins::JsonParseFrameState,Mutable,NonNullable;"));
    assert!(frame.contains("PARENT:GcRef<JsonParseFrame>,Immutable,Nullable;"));
    let codec = compact(between(
        VALUE,
        "impl GcI32Constant for crate::builtins::JsonParseFrameState {",
        "impl GcI32Constant for crate::builtins::JsonReviverFrameState",
    ));
    assert!(codec.contains("fnencode(self)->i32{self.wire_code()}"));
    assert!(!FRAME_SOURCE.contains("state: u32"));
    assert!(!FRAME_SOURCE.contains("state: I32Local"));
    assert!(!FRAME_SOURCE.contains("state: i32"));
    assert_eq!(JSON_SOURCE.matches("mod parse_frame_state;").count(), 1);
    assert!(!JSON_SOURCE.contains("pub mod parse_frame_state"));
}

#[test]
fn frame_construction_and_every_transition_require_typed_states() {
    let create = compact(between(
        FRAME_SOURCE,
        "fn emit_json_parse_frame(",
        "fn emit_json_parse_state(",
    ));
    assert!(create.contains("state:JsonParseFrameState,"));
    assert!(create.contains("parent:&GcLocal<JsonParseFrame,Nullable>,"));
    assert!(create.contains("GcOperand::constant(state),"));
    let write = compact(between(
        FRAME_SOURCE,
        "fn emit_json_parse_state(",
        "fn emit_json_parse_record(",
    ));
    assert!(write.contains("frame:&GcLocal<JsonParseFrame>,state:JsonParseFrameState,"));
    assert!(write.contains(
        ".field(JsonParseFrameSchema::STATE).write(frame,GcOperand::constant(state),s,f)"
    ));
    assert_eq!(
        FRAME_SOURCE
            .matches(".field(JsonParseFrameSchema::STATE)")
            .count(),
        1
    );
    assert_eq!(
        PARSE_SOURCE.matches("self.emit_json_parse_frame(").count(),
        1,
        "one container factory handles arrays and objects"
    );
    assert_eq!(
        PARSE_SOURCE.matches("self.emit_json_parse_state(").count(),
        5,
        "all subsequent transitions use the typed writer"
    );
    assert!(!PARSE_SOURCE.contains(".field(JsonParseFrameSchema::STATE)\n            .write"));
}

#[test]
fn parser_dispatch_exhaustively_projects_the_same_closed_domain() {
    let parser = compact(PARSE_SOURCE);
    let dispatch = between(
        &parser,
        "forselectedinJsonParseFrameState::ALL{",
        "frame.clear(f);",
    );
    assert!(dispatch
        .contains("state.load(f);f.instruction(&Instruction::I32Const(selected.wire_code()));"));
    assert!(dispatch.contains("matchselected{"));
    for variant in [
        "ArrayFirstOrEnd",
        "ArrayValue",
        "ArrayCommaOrEnd",
        "ObjectFirstKeyOrEnd",
        "ObjectKey",
        "ObjectValue",
        "ObjectCommaOrEnd",
    ] {
        assert!(
            dispatch.contains(&format!("JsonParseFrameState::{variant}")),
            "{variant}"
        );
    }
    assert!(!dispatch.contains("_=>"));
    assert!(parser.contains(".field(JsonParseFrameSchema::STATE).read(&frame,s,f).store(state,f);"));
    assert!(!parser.contains("JSON_PARSE_FRAME_STATE_OFFSET"));
    assert!(parser.contains("frame.clear(f);self.emit_branch_to_target(next,f);"));
}
