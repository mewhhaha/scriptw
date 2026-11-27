//! Bitwise results reach their int32 consumers without a binary64 round trip.
use super::*;

/// The encoded operators of each named source function's primary body.
/// Exact-helper-context clones (`name#id$...`) are left out: a clone can add
/// its own argument handling, so only primary bodies compare like for like.
fn bodies(source: &str) -> BTreeMap<String, Vec<Operator<'static>>> {
    let artifact = emit_script(source).expect("int32 source emits");
    expect_valid_module(&artifact, 1);
    let bytes: &'static [u8] = Box::leak(artifact.bytes.clone().into_boxed_slice());
    let names = function_names(&artifact);
    let mut index = imported_function_count(&artifact);
    let mut bodies = BTreeMap::<String, Vec<Operator<'static>>>::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("Wasm parses") {
            if let Some((name, _)) = names
                .get(&index)
                .and_then(|name| name.strip_prefix("js::"))
                .and_then(|name| name.split_once('#'))
                .filter(|(_, id)| !id.contains('$'))
            {
                bodies
                    .entry(name.to_owned())
                    .or_default()
                    .extend(crate::tests::emitted_operators(&body));
            }
            index += 1;
        }
    }
    bodies
}

fn body<'a>(
    bodies: &'a BTreeMap<String, Vec<Operator<'static>>>,
    name: &str,
) -> &'a [Operator<'static>] {
    bodies
        .get(name)
        .unwrap_or_else(|| panic!("source function {name} has an emitted body"))
}

fn count(operators: &[Operator<'_>], matches: impl Fn(&Operator<'_>) -> bool) -> usize {
    operators
        .iter()
        .filter(|operator| matches(operator))
        .count()
}

fn count_u32_max_bounds(operators: &[Operator<'_>]) -> usize {
    operators
        .windows(2)
        .filter(|pair| {
            matches!(
                pair,
                [Operator::I64Const { value }, Operator::I64LtU]
                    if *value == i64::from(u32::MAX)
            )
        })
        .count()
}

#[test]
fn nested_bitwise_operators_convert_to_binary64_only_at_the_number_consumer() {
    let bodies = bodies(
        "function single(a) { return a ^ 5; }
         function chain(a) { return ((((a ^ 5) << 2) >> 1) ^ 9) << 3; }
         single(1); chain(1);",
    );
    let single = body(&bodies, "single");
    let chain = body(&bodies, "chain");
    // Both coerce the same dynamic operand once and return one Number. Every
    // nested operator consumes its operand's bits: neither a convert-back nor
    // another ToUint32 of an intermediate result is emitted.
    for (name, matches) in [
        (
            "f64.convert_i32",
            (|operator: &Operator<'_>| {
                matches!(
                    operator,
                    Operator::F64ConvertI32S | Operator::F64ConvertI32U
                )
            }) as fn(&Operator<'_>) -> bool,
        ),
        ("i64.trunc_sat_f64_s", |operator| {
            matches!(operator, Operator::I64TruncSatF64S)
        }),
    ] {
        assert_eq!(
            count(chain, matches),
            count(single, matches),
            "{name}: a bitwise chain adds no binary64 round trip"
        );
    }
    let shifts = |operators: &[Operator<'_>]| {
        count(operators, |operator| {
            matches!(operator, Operator::I32Shl | Operator::I32ShrS)
        })
    };
    assert_eq!(
        shifts(chain),
        shifts(single) + 3,
        "the chain's shifts stay in i32"
    );
}

#[test]
fn int32_array_index_probes_without_binary64_revalidation() {
    let bodies = bodies(
        "function masked(rows, i) { return rows[i & 63]; }
         function raw(rows, i) { return rows[+i]; }
         var steps = [function (row) { return row; }, function (row) { return -row; }];
         function callMasked(index, row) { return steps[index & 1](row); }
         function callRaw(index, row) { return steps[+index](row); }
         masked([1], 0); raw([1], 0); callMasked(0, 1); callRaw(0, 1);",
    );
    // The raw Number key's probe tests its binary64 range against 2^32-1;
    // the int32 key's bits are an index as they stand (a String receiver's
    // code-unit probe keeps its own bound in both reads).
    for (masked, raw) in [("masked", "raw"), ("callMasked", "callRaw")] {
        let masked_body = body(&bodies, masked);
        let raw_body = body(&bodies, raw);
        assert_eq!(
            count_u32_max_bounds(raw_body),
            count_u32_max_bounds(masked_body) + 1,
            "{masked} skips the binary64 index validation"
        );
        assert_eq!(
            count(masked_body, |operator| matches!(
                operator,
                Operator::F64ConvertI64S
            )) + 1,
            count(raw_body, |operator| matches!(
                operator,
                Operator::F64ConvertI64S
            )),
            "{masked} indexes with its bits, not a binary64 round-trip"
        );
        assert!(masked_body
            .iter()
            .any(|operator| matches!(operator, Operator::I64ExtendI32U)));
    }
}

#[test]
fn int32_discriminant_reaches_the_dense_table_without_exactness_test() {
    let cases = (0..32)
        .map(|index| format!("case {index}: return {};", index + 1))
        .collect::<String>();
    let source = format!(
        "function masked(value) {{ switch (value & 31) {{ {cases} }} return 0; }}
         function unsigned(value) {{ switch (value >>> 0) {{ {cases} }} return 0; }}
         masked(0); unsigned(0);"
    );
    let bodies = bodies(&source);
    for (name, guard) in [
        ("masked", [Operator::I32Const { value: 1 }].as_slice()),
        (
            "unsigned",
            [Operator::I32Const { value: 0 }, Operator::I32GeS].as_slice(),
        ),
    ] {
        let operators = body(&bodies, name);
        let table = operators
            .iter()
            .position(|operator| matches!(operator, Operator::BrTable { .. }))
            .unwrap_or_else(|| panic!("{name} keeps its table dispatch"));
        let open = operators[..table]
            .iter()
            .rposition(|operator| matches!(operator, Operator::If { .. }))
            .expect("the table dispatch is guarded");
        assert_eq!(
            &operators[open - guard.len()..open],
            guard,
            "{name}: no binary64 exactness test precedes the table"
        );
    }
}

#[test]
fn int32_predicates_compare_integers_without_a_number_or_helper() {
    let bodies = bodies(
        "function truthy(a) { return (a & 1) ? 10 : 20; }
         function strict(a) { return (a & 1) === 0; }
         function loose(a) { return 0 == (a & 1); }
         function less(a) { return (a >>> 0) < 4294967295; }
         truthy(1); strict(1); loose(1); less(1);",
    );
    let truthy = body(&bodies, "truthy");
    assert!(
        truthy.windows(3).any(|window| matches!(
            window,
            [
                Operator::I32And,
                Operator::I32Const { value: 0 },
                Operator::I32Ne
            ]
        )),
        "ToBoolean of `a & 1` tests its bits"
    );
    for name in ["strict", "loose"] {
        let operators = body(&bodies, name);
        assert!(
            operators
                .iter()
                .any(|operator| matches!(operator, Operator::I64Eq)),
            "{name} equality compares exact integers"
        );
        assert_eq!(
            count(operators, |operator| matches!(
                operator,
                Operator::F64ConvertI32S | Operator::F64ConvertI32U
            )),
            count(body(&bodies, "truthy"), |operator| matches!(
                operator,
                Operator::F64ConvertI32S | Operator::F64ConvertI32U
            )),
            "{name} materializes no Number for its comparison"
        );
    }
    let less = body(&bodies, "less");
    assert!(less.windows(2).any(|window| matches!(
        window,
        [
            Operator::I64Const {
                value: 4_294_967_295
            },
            Operator::I64LtS
        ]
    )));
    assert!(less
        .iter()
        .any(|operator| matches!(operator, Operator::I64ExtendI32U)));
}

#[test]
fn captured_slots_written_only_with_int32_values_keep_int32_bits() {
    let ir = lower(
        &parse(
            include_str!("../../tests/emission/fixtures/numeric_body_frame_compact.mjs"),
            ParseOptions::module(),
        )
        .expect("frame fixture parses"),
    );
    let artifact = emit(&ir).expect("frame fixture emits");
    expect_valid_module(&artifact, 1);
    let bytes: &'static [u8] = Box::leak(artifact.bytes.clone().into_boxed_slice());
    let names = function_names(&artifact);
    let mut index = imported_function_count(&artifact);
    let mut run = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("Wasm parses") {
            if names
                .get(&index)
                .is_some_and(|name| name.starts_with("js::run#"))
            {
                run.extend(crate::tests::emitted_operators(&body));
            }
            index += 1;
        }
    }
    // `tick = (tick + k) & 255` reads the captured slot's bits, adds and
    // masks in i32, and stores bits: no binary64 add, ToUint32 or convert.
    for step in [1, 3] {
        assert!(
            run.windows(5).any(|window| matches!(
                window,
                [
                    Operator::LocalGet { .. },
                    Operator::I32Const { value },
                    Operator::I32Add,
                    Operator::I32Const { value: 255 },
                    Operator::I32And,
                ] if *value == step
            )),
            "the closed region updates tick by {step} in the int32 domain"
        );
    }
}
