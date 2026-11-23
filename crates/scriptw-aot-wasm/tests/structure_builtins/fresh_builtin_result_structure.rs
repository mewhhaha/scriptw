//! Native builtins whose results are fresh until returned publish them
//! directly: split's packed list, the shared `{ value, done }` shape,
//! Array.from's integer-key publication into a fresh %Array% result, and the
//! u32 NumberToString digit loop. Each keeps its general path for every case
//! the fresh proof does not cover.
const SPLIT: &str = include_str!("../../src/builtins/string/string_literal_replacement_scope.rs");
const PACKED_LIST: &str = include_str!("../../src/gc_types/value/packed_value_list.rs");
const ITERATORS: &str = include_str!("../../src/builtins/iterators.rs");
const ITERATOR_RESULT: &str = include_str!("../../src/objects/compact_data/iterator_result.rs");
const ARRAY: &str = include_str!("../../src/builtins/array.rs");
const RYU: &str = include_str!("../../src/operations/number_to_string/ryu.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}`"))
        .0
}

fn normalized(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[test]
fn split_appends_into_one_packed_backing_and_searches_by_leaf_kernel() {
    let split = normalized(bounded(
        SPLIT,
        "pub(super) fn emit_native_string_split_literal(",
        "\n}\n",
    ));
    assert!(split.contains("PackedValueListConstruction::allocate(s,f)"));
    assert!(split.contains("emit_publish_packed_value_list_with_current_function_realm_prototype("));
    for removed in [
        "ArgumentListConstruction",
        "emit_native_string_list_result_array",
        "emit_native_string_array_write",
    ] {
        assert!(
            !split.contains(removed),
            "split still builds through {removed}"
        );
    }
    let search = normalized(bounded(
        SPLIT,
        "pub(super) fn emit_native_string_index_of(",
        "pub(super) fn emit_string_replace_literal_first_occurrence_from_string_locals(",
    ));
    assert!(search.contains("CodeUnitFindArguments::new(&units,at,last,first)"));
    // A longer needle still compares every unit at the found position.
    assert!(search.contains("self.emit_native_string_equal_at("));
    // The published Array is the PackedValues state with COUNT = length.
    let publish = normalized(bounded(
        PACKED_LIST,
        "pub(crate) fn with_packed_value_list(",
        "\n}\n",
    ));
    assert!(publish.contains("GcOperand::i64_local(count),GcOperand::i64_local(count),"));
    assert!(publish.contains("GcOperand::constant(ArrayIndexedStorageMode::PackedValues)"));
}

#[test]
fn iterator_results_share_one_compact_shape_and_actual_arrays_read_by_index() {
    let result = normalized(bounded(
        ITERATORS,
        "pub(crate) fn emit_iterator_result_object_in_realm(",
        "\n    }\n",
    ));
    assert!(
        result.contains("self.emit_alloc_iterator_result_object(&prototype,value,done,function)?")
    );
    assert!(!result.contains("emit_object_append_data_property_with_flags"));
    let shape = normalized(ITERATOR_RESULT);
    assert!(shape.contains("fornamein[\"value\",\"done\"]"));
    assert!(shape.contains("schema.load_iterator_result_shape(&result,f)"));
    assert!(shape.contains("schema.publish_iterator_result_shape(&shape,f)"));
    let next = normalized(bounded(
        ITERATORS,
        "pub(crate) fn emit_array_iterator_next_from_locals(",
        "pub(crate) fn emit_regexp_string_iterator_next_from_locals(",
    ));
    let own = next
        .find("self.emit_array_own_indexed_value(&actual,index,&element,found,function)")
        .expect("actual Arrays read their own data element by integer index");
    let general = next
        .find("self.emit_number_to_string_payload(element.scalar(),function)?")
        .expect("holes, accessors and other objects keep the string-keyed Get");
    assert!(own < general);
}

#[test]
fn array_from_publishes_by_integer_key_only_into_a_fresh_array() {
    let fresh = normalized(bounded(
        ARRAY,
        "fn emit_array_from_fresh_target(",
        "fn emit_array_from_define(",
    ));
    for proof in [
        "self.emit_is_constructor_i32(constructor,f);f.instruction(&Instruction::I32Eqz);",
        "NonArrayRealmIntrinsicSlot::ArrayConstructor",
        "f.instruction(&Instruction::RefEq);",
        "reference_type::<ArrayObject>",
    ] {
        assert!(fresh.contains(proof), "fresh-result proof lost {proof}");
    }
    let define = normalized(bounded(
        ARRAY,
        "fn emit_array_from_define(",
        "pub(crate) fn emit_array_from_builtin(",
    ));
    let fast = define
        .find("self.emit_array_indexed_publish_default_data(&array,index,value,f)?")
        .unwrap();
    let general = define
        .find("self.emit_create_data_property_or_throw(target,&key,value,pending,f)?")
        .unwrap();
    assert!(fast < general);
    let from = normalized(bounded(
        ARRAY,
        "pub(crate) fn emit_array_from_builtin(",
        "\n}\n",
    ));
    assert_eq!(
        from.matches("self.emit_array_from_fresh_target(&constructor,&target,fresh,f);")
            .count(),
        2
    );
    assert!(
        from.contains("self.emit_array_from_define(fresh,&target,index,&value,true,&pending,f)?")
    );
    assert!(
        from.contains("self.emit_array_from_define(fresh,&target,index,&value,false,&pending,f)?")
    );
    assert!(!from.contains("emit_array_native_index_key"));
}

#[test]
fn integral_u32_numbers_take_the_digit_loop_before_the_general_formatter() {
    let entry = normalized(bounded(
        RYU,
        "pub(super) fn emit_ryu_number_to_string_payload(",
        "fn emit_u32_decimal_string(",
    ));
    let digits = entry
        .find("self.emit_u32_decimal_string(small,function)")
        .unwrap();
    let general = entry
        .find("letabsolute=schema.reserve_i64_local(function);")
        .unwrap();
    assert!(digits < general);
}
