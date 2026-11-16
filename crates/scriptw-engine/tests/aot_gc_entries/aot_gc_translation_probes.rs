//! What the Wasm-to-native translation may leave out, and what it may not.
//!
//! The vendored translation skips a cast or null check that an earlier one
//! on the same value implies, reads a GC reference from the frame once after
//! a call instead of at every use, sends a branch on a fresh truth value
//! straight to its target, and compares once where a chain tests one value
//! against consecutive constants. These probes run hand-assembled Wasm on
//! the product's Wasmtime configuration and check the cases where that would
//! be wrong: a check that only one path made, a reference that a moving
//! collection replaced while the function was in a call, and values at and
//! around the ends of a run.
use scriptw_engine::configure_wasm_runtime;
use wasmtime::{AnyRef, Caller, Config, Func, Instance, Module, Rooted, Store, Trap, Val};

/// The reference whose object the `gc` import expects to move.
#[derive(Default)]
struct Watch {
    watched: Option<Rooted<AnyRef>>,
    collections: u32,
    moves: u32,
}

fn leb(mut value: u32, bytes: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            bytes.push(byte);
            return;
        }
        bytes.push(byte | 0x80);
    }
}

fn section(module: &mut Vec<u8>, id: u8, count: usize, items: &[u8]) {
    let mut body = Vec::new();
    leb(count as u32, &mut body);
    body.extend_from_slice(items);
    module.push(id);
    leb(body.len() as u32, module);
    module.extend_from_slice(&body);
}

/// One function body with `locals` further `i32` locals.
fn body(locals: u32, code: &[u8]) -> Vec<u8> {
    let mut inner = Vec::new();
    if locals == 0 {
        inner.push(0);
    } else {
        inner.push(1);
        leb(locals, &mut inner);
        inner.push(0x7f);
    }
    inner.extend_from_slice(code);
    inner.push(0x0b);
    let mut sized = Vec::new();
    leb(inner.len() as u32, &mut sized);
    sized.extend_from_slice(&inner);
    sized
}

// `local.get 0; ref.cast (ref $a)`
const CAST_A: [u8; 5] = [0x20, 0x00, 0xfb, 0x16, 0x00];
// `struct.get $a 0`
const FIELD: [u8; 4] = [0xfb, 0x02, 0x00, 0x00];
// `local.get 2; local.get 0; struct.get $a 0; i32.add; local.set 2`
const ADD_FIELD: [u8; 11] = [
    0x20, 0x02, 0x20, 0x00, 0xfb, 0x02, 0x00, 0x00, 0x6a, 0x21, 0x02,
];

const EXPORTS: [&str; 16] = [
    "new_a",
    "new_b",
    "new_i31",
    "cast_after_then",
    "cast_in_else",
    "cast_after_test",
    "cast_after_loop",
    "non_null_after_branch",
    "read_around_calls",
    "classify",
    "field_or_zero",
    "one_of",
    "none_of",
    "zero_or_one",
    "top_two",
    "around_zero",
];

// (module
//   (type $a (struct (field (mut i32))))
//   (type $b (struct (field (mut i64))))
//   (import "host" "gc" (func $gc))
//   (func $mutate (param (ref $a) i32)
//     call $gc
//     local.get 0 local.get 1 struct.set $a 0)
//   (func (export "new_a") (param i32) (result eqref) local.get 0 struct.new $a)
//   (func (export "new_b") (result eqref) i64.const 7 struct.new $b)
//   (func (export "new_i31") (result eqref) i32.const 5 ref.i31)
//   ;; The consequent's cast does not cover the code after the `if`.
//   (func (export "cast_after_then") (param eqref i32) (result i32)
//     local.get 1
//     if local.get 0 ref.cast (ref $a) drop end
//     local.get 0 ref.cast (ref $a) struct.get $a 0)
//   ;; Nor does it cover the alternative.
//   (func (export "cast_in_else") (param eqref i32) (result i32)
//     local.get 1
//     if (result i32)
//       local.get 0 ref.cast (ref $a) struct.get $a 0
//     else
//       local.get 0 ref.cast (ref $a) struct.get $a 0 i32.const 1000 i32.add
//     end)
//   ;; A successful test covers its consequent only.
//   (func (export "cast_after_test") (param eqref i32) (result i32)
//     local.get 0 ref.test (ref $a)
//     if (result i32)
//       local.get 0 ref.cast (ref $a) struct.get $a 0
//     else
//       i32.const -1
//     end
//     local.get 1
//     if local.get 0 ref.cast (ref $a) drop end)
//   ;; A cast in a loop body that may not run covers nothing after the loop.
//   (func (export "cast_after_loop") (param eqref i32) (result i32)
//     block
//       loop
//         local.get 1 i32.eqz br_if 1
//         local.get 0 ref.cast (ref $a) drop
//         local.get 1 i32.const 1 i32.sub local.set 1
//         br 0
//       end
//     end
//     local.get 0 ref.cast (ref $a) struct.get $a 0)
//   ;; `br_on_null` proves its operand only where it falls through.
//   (func (export "non_null_after_branch") (param (ref null $a) i32) (result i32)
//     local.get 1
//     if
//       block local.get 0 br_on_null 0 drop end
//     end
//     local.get 0 ref.as_non_null struct.get $a 0)
//   ;; Every read after a call sees what the callee stored after the
//   ;; collection that moved the object.
//   (func (export "read_around_calls") (param (ref null $a) i32) (result i32)
//     (local $sum i32) (local $i i32)
//     local.get 0 struct.get $a 0 local.set $sum
//     local.get 0 ref.as_non_null i32.const 10 call $mutate
//     local.get $sum local.get 0 struct.get $a 0 i32.add local.set $sum
//     local.get 1
//     if local.get 0 ref.as_non_null i32.const 100 call $mutate end
//     local.get $sum local.get 0 struct.get $a 0 i32.add local.set $sum
//     i32.const 3 local.set $i
//     loop
//       local.get 0 ref.as_non_null local.get $i i32.const 1000 i32.mul call $mutate
//       local.get $sum local.get 0 struct.get $a 0 i32.add local.set $sum
//       local.get $i i32.const 1 i32.sub local.tee $i
//       br_if 0
//     end
//     local.get $sum)
//   ;; Branches on truth values that each path has just chosen.
//   (func (export "classify") (param eqref i32) (result i32)
//     local.get 0 ref.is_null
//     if (result i32)
//       i32.const 3
//     else
//       local.get 0 ref.test (ref $a)
//       if (result i32)
//         i32.const 1
//       else
//         local.get 0 ref.test (ref null $b)
//         if (result i32) i32.const 2 else i32.const 4 end
//       end
//     end)
//   (func (export "field_or_zero") (param eqref i32) (result i32)
//     block (result (ref $a))
//       local.get 0
//       br_on_cast 0 eqref (ref $a)
//       drop i32.const 0 return
//     end
//     struct.get $a 0)
//   ;; Chains of tests of one value against constants.
//   (func (export "one_of") (param i32) (result i32)
//     i32.const 0
//     local.get 0 i32.const 6 i32.eq i32.or
//     local.get 0 i32.const 7 i32.eq i32.or
//     local.get 0 i32.const 8 i32.eq i32.or
//     local.get 0 i32.const 9 i32.eq i32.or)
//   (func (export "none_of") (param i32) (result i32)
//     local.get 0 i32.const 2 i32.ne
//     local.get 0 i32.const 4 i32.ne i32.and
//     local.get 0 i32.const 3 i32.ne i32.and)
//   (func (export "zero_or_one") (param i32) (result i32)
//     local.get 0 i32.eqz
//     local.get 0 i32.const 1 i32.eq i32.or)
//   (func (export "top_two") (param i32) (result i32)
//     local.get 0 i32.const -2 i32.eq
//     local.get 0 i32.const -1 i32.eq i32.or)
//   ;; Consecutive as signed numbers only.
//   (func (export "around_zero") (param i32) (result i32)
//     local.get 0 i32.const -1 i32.eq
//     local.get 0 i32.const 0 i32.eq i32.or
//     local.get 0 i32.const 1 i32.eq i32.or))
// Binary fixture avoids adding a second encoder/parser dependency to Engine.
fn probe_bytes() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        9,
        &[
            0x5f, 1, 0x7f, 1, // 0: $a
            0x5f, 1, 0x7e, 1, // 1: $b
            0x60, 0, 0, // 2: $gc
            0x60, 2, 0x64, 0, 0x7f, 0, // 3: $mutate
            0x60, 2, 0x6d, 0x7f, 1, 0x7f, // 4: (eqref, i32) -> i32
            0x60, 2, 0x63, 0, 0x7f, 1, 0x7f, // 5: ((ref null $a), i32) -> i32
            0x60, 0, 1, 0x6d, // 6: () -> eqref
            0x60, 1, 0x7f, 1, 0x6d, // 7: (i32) -> eqref
            0x60, 1, 0x7f, 1, 0x7f, // 8: (i32) -> i32
        ],
    );
    section(&mut module, 2, 1, b"\x04host\x02gc\x00\x02");
    section(
        &mut module,
        3,
        17,
        &[3, 7, 6, 6, 4, 4, 4, 4, 5, 5, 4, 4, 8, 8, 8, 8, 8],
    );
    let mut exports = Vec::new();
    for (index, name) in EXPORTS.iter().enumerate() {
        exports.push(name.len() as u8);
        exports.extend_from_slice(name.as_bytes());
        // The import is function 0 and `$mutate` function 1.
        exports.extend_from_slice(&[0x00, index as u8 + 2]);
    }
    section(&mut module, 7, EXPORTS.len(), &exports);

    let mut code = Vec::new();
    // $mutate
    code.extend(body(
        0,
        &[0x10, 0x00, 0x20, 0x00, 0x20, 0x01, 0xfb, 0x05, 0x00, 0x00],
    ));
    // new_a, new_b, new_i31
    code.extend(body(0, &[0x20, 0x00, 0xfb, 0x00, 0x00]));
    code.extend(body(0, &[0x42, 0x07, 0xfb, 0x00, 0x01]));
    code.extend(body(0, &[0x41, 0x05, 0xfb, 0x1c]));
    // cast_after_then
    code.extend(body(
        0,
        &[
            &[0x20, 0x01, 0x04, 0x40][..],
            &CAST_A,
            &[0x1a, 0x0b],
            &CAST_A,
            &FIELD,
        ]
        .concat(),
    ));
    // cast_in_else
    code.extend(body(
        0,
        &[
            &[0x20, 0x01, 0x04, 0x7f][..],
            &CAST_A,
            &FIELD,
            &[0x05],
            &CAST_A,
            &FIELD,
            &[0x41, 0xe8, 0x07, 0x6a, 0x0b],
        ]
        .concat(),
    ));
    // cast_after_test
    code.extend(body(
        0,
        &[
            &[0x20, 0x00, 0xfb, 0x14, 0x00, 0x04, 0x7f][..],
            &CAST_A,
            &FIELD,
            &[0x05, 0x41, 0x7f, 0x0b, 0x20, 0x01, 0x04, 0x40],
            &CAST_A,
            &[0x1a, 0x0b],
        ]
        .concat(),
    ));
    // cast_after_loop
    code.extend(body(
        0,
        &[
            &[0x02, 0x40, 0x03, 0x40, 0x20, 0x01, 0x45, 0x0d, 0x01][..],
            &CAST_A,
            &[
                0x1a, 0x20, 0x01, 0x41, 0x01, 0x6b, 0x21, 0x01, 0x0c, 0x00, 0x0b, 0x0b,
            ],
            &CAST_A,
            &FIELD,
        ]
        .concat(),
    ));
    // non_null_after_branch
    code.extend(body(
        0,
        &[
            &[
                0x20, 0x01, 0x04, 0x40, 0x02, 0x40, 0x20, 0x00, 0xd5, 0x00, 0x1a, 0x0b, 0x0b, 0x20,
                0x00, 0xd4,
            ][..],
            &FIELD,
        ]
        .concat(),
    ));
    // read_around_calls
    code.extend(body(
        2,
        &[
            &[0x20, 0x00][..],
            &FIELD,
            &[0x21, 0x02, 0x20, 0x00, 0xd4, 0x41, 0x0a, 0x10, 0x01],
            &ADD_FIELD,
            &[
                0x20, 0x01, 0x04, 0x40, 0x20, 0x00, 0xd4, 0x41, 0xe4, 0x00, 0x10, 0x01, 0x0b,
            ],
            &ADD_FIELD,
            &[0x41, 0x03, 0x21, 0x03, 0x03, 0x40],
            &[
                0x20, 0x00, 0xd4, 0x20, 0x03, 0x41, 0xe8, 0x07, 0x6c, 0x10, 0x01,
            ],
            &ADD_FIELD,
            &[
                0x20, 0x03, 0x41, 0x01, 0x6b, 0x22, 0x03, 0x0d, 0x00, 0x0b, 0x20, 0x02,
            ],
        ]
        .concat(),
    ));
    // classify
    code.extend(body(
        0,
        &[
            0x20, 0x00, 0xd1, 0x04, 0x7f, 0x41, 0x03, 0x05, 0x20, 0x00, 0xfb, 0x14, 0x00, 0x04,
            0x7f, 0x41, 0x01, 0x05, 0x20, 0x00, 0xfb, 0x15, 0x01, 0x04, 0x7f, 0x41, 0x02, 0x05,
            0x41, 0x04, 0x0b, 0x0b, 0x0b,
        ],
    ));
    // field_or_zero
    code.extend(body(
        0,
        &[
            &[
                0x02, 0x64, 0x00, 0x20, 0x00, 0xfb, 0x18, 0x01, 0x00, 0x6d, 0x00, 0x1a, 0x41, 0x00,
                0x0f, 0x0b,
            ][..],
            &FIELD,
        ]
        .concat(),
    ));
    // one_of
    code.extend(body(
        0,
        &[
            0x41, 0x00, 0x20, 0x00, 0x41, 0x06, 0x46, 0x72, 0x20, 0x00, 0x41, 0x07, 0x46, 0x72,
            0x20, 0x00, 0x41, 0x08, 0x46, 0x72, 0x20, 0x00, 0x41, 0x09, 0x46, 0x72,
        ],
    ));
    // none_of
    code.extend(body(
        0,
        &[
            0x20, 0x00, 0x41, 0x02, 0x47, 0x20, 0x00, 0x41, 0x04, 0x47, 0x71, 0x20, 0x00, 0x41,
            0x03, 0x47, 0x71,
        ],
    ));
    // zero_or_one
    code.extend(body(
        0,
        &[0x20, 0x00, 0x45, 0x20, 0x00, 0x41, 0x01, 0x46, 0x72],
    ));
    // top_two
    code.extend(body(
        0,
        &[
            0x20, 0x00, 0x41, 0x7e, 0x46, 0x20, 0x00, 0x41, 0x7f, 0x46, 0x72,
        ],
    ));
    // around_zero
    code.extend(body(
        0,
        &[
            0x20, 0x00, 0x41, 0x7f, 0x46, 0x20, 0x00, 0x41, 0x00, 0x46, 0x72, 0x20, 0x00, 0x41,
            0x01, 0x46, 0x72,
        ],
    ));
    section(&mut module, 10, 17, &code);
    module
}

struct Probe {
    store: Store<Watch>,
    instance: Instance,
}

impl Probe {
    fn new() -> Self {
        let mut config = Config::new();
        configure_wasm_runtime(&mut config);
        let engine = wasmtime::Engine::new(&config).unwrap();
        let module = Module::new(&engine, probe_bytes()).unwrap();
        let mut store = Store::new(&engine, Watch::default());
        let gc = Func::wrap(
            &mut store,
            |mut caller: Caller<'_, Watch>| -> wasmtime::Result<()> {
                let watched = caller.data().watched;
                let before = match watched {
                    Some(reference) => Some(reference.to_raw(&mut caller)?),
                    None => None,
                };
                caller.gc(None)?;
                caller.data_mut().collections += 1;
                if let (Some(reference), Some(before)) = (watched, before) {
                    if reference.to_raw(&mut caller)? != before {
                        caller.data_mut().moves += 1;
                    }
                }
                Ok(())
            },
        );
        let instance = Instance::new(&mut store, &module, &[gc.into()]).unwrap();
        Self { store, instance }
    }

    fn call(&mut self, name: &str, arguments: &[Val]) -> wasmtime::Result<Val> {
        let function = self.instance.get_func(&mut self.store, name).unwrap();
        let mut output = [Val::I32(0)];
        function.call(&mut self.store, arguments, &mut output)?;
        Ok(output[0])
    }

    fn reference(&mut self, name: &str, arguments: &[Val]) -> Rooted<AnyRef> {
        match self.call(name, arguments).unwrap() {
            Val::AnyRef(Some(reference)) => reference,
            other => panic!("{name} returned {other:?}"),
        }
    }

    fn number(&mut self, name: &str, reference: Option<Rooted<AnyRef>>, flag: i32) -> i32 {
        self.call(name, &[Val::AnyRef(reference), Val::I32(flag)])
            .unwrap()
            .unwrap_i32()
    }

    fn trap(&mut self, name: &str, reference: Option<Rooted<AnyRef>>, flag: i32) -> Trap {
        let error = self
            .call(name, &[Val::AnyRef(reference), Val::I32(flag)])
            .unwrap_err();
        *error
            .downcast_ref::<Trap>()
            .unwrap_or_else(|| panic!("{name}: {error:?}"))
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn a_check_that_one_path_made_does_not_cover_the_others() {
    let mut probe = Probe::new();
    let a = Some(probe.reference("new_a", &[Val::I32(5)]));
    let b = Some(probe.reference("new_b", &[]));
    let i31 = Some(probe.reference("new_i31", &[]));

    for flag in [0, 1] {
        assert_eq!(probe.number("cast_after_then", a, flag), 5);
        for other in [b, i31, None] {
            assert_eq!(
                probe.trap("cast_after_then", other, flag),
                Trap::CastFailure
            );
        }
    }

    assert_eq!(probe.number("cast_in_else", a, 1), 5);
    assert_eq!(probe.number("cast_in_else", a, 0), 1005);
    for flag in [0, 1] {
        for other in [b, i31, None] {
            assert_eq!(probe.trap("cast_in_else", other, flag), Trap::CastFailure);
        }
    }

    for flag in [0, 1] {
        assert_eq!(probe.number("cast_after_test", a, flag), 5);
    }
    for other in [b, i31, None] {
        assert_eq!(probe.number("cast_after_test", other, 0), -1);
        assert_eq!(probe.trap("cast_after_test", other, 1), Trap::CastFailure);
    }

    for turns in [0, 1, 3] {
        assert_eq!(probe.number("cast_after_loop", a, turns), 5);
        for other in [b, i31, None] {
            assert_eq!(
                probe.trap("cast_after_loop", other, turns),
                Trap::CastFailure
            );
        }
    }

    for flag in [0, 1] {
        assert_eq!(probe.number("non_null_after_branch", a, flag), 5);
        assert_eq!(
            probe.trap("non_null_after_branch", None, flag),
            Trap::NullReference
        );
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn a_branch_on_a_fresh_truth_value_goes_where_the_value_says() {
    let mut probe = Probe::new();
    let a = Some(probe.reference("new_a", &[Val::I32(5)]));
    let b = Some(probe.reference("new_b", &[]));
    let i31 = Some(probe.reference("new_i31", &[]));
    for (reference, class, field) in [(a, 1, 5), (b, 2, 0), (None, 3, 0), (i31, 4, 0)] {
        assert_eq!(probe.number("classify", reference, 0), class);
        assert_eq!(probe.number("field_or_zero", reference, 0), field);
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn a_reference_is_read_again_after_a_call_that_moved_its_object() {
    for flag in [0, 1] {
        let mut probe = Probe::new();
        let a = probe.reference("new_a", &[Val::I32(5)]);
        probe.store.data_mut().watched = Some(a);
        // Each call of `$mutate` collects and then stores through the
        // reference it was passed: a caller that kept reading through the
        // reference it had before the call would see the old object.
        let expected = 5 + 10 + if flag == 1 { 100 } else { 10 } + 3000 + 2000 + 1000;
        assert_eq!(probe.number("read_around_calls", Some(a), flag), expected);
        assert_eq!(probe.number("cast_after_then", Some(a), 0), 1000);
        let watch = probe.store.data();
        assert_eq!(watch.collections, 4 + flag as u32);
        // The probe is only worth its name if the collector moved the object.
        assert!(watch.moves > 0, "no collection moved the watched object");
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn a_chain_of_tests_against_constants_agrees_with_each_test() {
    let mut probe = Probe::new();
    let mut inputs: Vec<i32> = (-4..=12).collect();
    inputs.extend([
        i32::MIN,
        i32::MIN + 6,
        i32::MAX,
        i32::MAX - 1,
        6 + (1 << 16),
        i32::MIN + 9,
    ]);
    for input in inputs {
        let mut call = |name: &str| probe.call(name, &[Val::I32(input)]).unwrap().unwrap_i32();
        assert_eq!(
            call("one_of"),
            i32::from((6..=9).contains(&input)),
            "{input}"
        );
        assert_eq!(
            call("none_of"),
            i32::from(!(2..=4).contains(&input)),
            "{input}"
        );
        assert_eq!(
            call("zero_or_one"),
            i32::from(input == 0 || input == 1),
            "{input}"
        );
        assert_eq!(
            call("top_two"),
            i32::from(input == -2 || input == -1),
            "{input}"
        );
        assert_eq!(
            call("around_zero"),
            i32::from((-1..=1).contains(&input)),
            "{input}"
        );
    }
}

/// `(type $s (struct (field nullfuncref) (field i32)))` with two readers of
/// the uninhabited field, each followed by another use of the reference.
fn bottom_field_bytes() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        3,
        &[
            0x5f, 2, 0x73, 0, 0x7f, 0, // 0: $s
            0x60, 1, 0x63, 0, 1, 0x7f, // 1: ((ref null $s)) -> i32
            0x60, 0, 1, 0x63, 0, // 2: () -> (ref null $s)
        ],
    );
    section(&mut module, 3, 3, &[2, 1, 1]);
    let mut exports = Vec::new();
    for (index, name) in ["new", "read_then_non_null", "read_then_field"]
        .into_iter()
        .enumerate()
    {
        leb(name.len() as u32, &mut exports);
        exports.extend_from_slice(name.as_bytes());
        exports.extend_from_slice(&[0, index as u8]);
    }
    section(&mut module, 7, 3, &exports);
    let mut code = Vec::new();
    // `ref.null nofunc; i32.const 7; struct.new $s`
    code.extend(body(0, &[0xd0, 0x73, 0x41, 0x07, 0xfb, 0x00, 0x00]));
    // `local.get 0; struct.get $s 0; drop; local.get 0; ref.as_non_null; ref.is_null`
    code.extend(body(
        0,
        &[
            0x20, 0x00, 0xfb, 0x02, 0x00, 0x00, 0x1a, 0x20, 0x00, 0xd4, 0xd1,
        ],
    ));
    // `local.get 0; struct.get $s 0; drop; local.get 0; struct.get $s 1`
    code.extend(body(
        0,
        &[
            0x20, 0x00, 0xfb, 0x02, 0x00, 0x00, 0x1a, 0x20, 0x00, 0xfb, 0x02, 0x00, 0x01,
        ],
    ));
    section(&mut module, 10, 3, &code);
    module
}

/// A field of an uninhabited type is known to be null without a load, but
/// reading it through a null reference must trap all the same, and may not
/// stand as the proof that the reference is not null.
#[cfg(target_pointer_width = "64")]
#[test]
fn a_read_of_an_uninhabited_field_still_checks_its_reference() {
    let mut config = Config::new();
    configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let module = Module::new(&engine, bottom_field_bytes()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let mut call = |name: &str, arguments: &[Val]| -> wasmtime::Result<Val> {
        let function = instance.get_func(&mut store, name).unwrap();
        let mut output = [Val::I32(0)];
        function.call(&mut store, arguments, &mut output)?;
        Ok(output[0])
    };
    let object = call("new", &[]).unwrap();
    assert!(matches!(object, Val::AnyRef(Some(_))));
    assert_eq!(
        call("read_then_non_null", &[object]).unwrap().unwrap_i32(),
        0
    );
    assert_eq!(call("read_then_field", &[object]).unwrap().unwrap_i32(), 7);
    for name in ["read_then_non_null", "read_then_field"] {
        let error = call(name, &[Val::AnyRef(None)]).unwrap_err();
        assert_eq!(
            error.downcast_ref::<Trap>().copied(),
            Some(Trap::NullReference),
            "{name}: {error:?}"
        );
    }
}

/// How many functions the funcref probe defines: more than twice the
/// runtime's intern cache, so that addresses share its entries.
const FUNCREF_PROBE_FUNCTIONS: u32 = 600;

// (module
//   (type $f (func (result i32)))
//   (type $s (struct (field (mut (ref null $f))) (field (mut (ref null $f)))))
//   (func $k (type $f) i32.const k) ...                ;; one for each k
//   (table (N + 1) funcref)                            ;; entry N stays null
//   (elem (i32.const 0) func 0 1 ...)
//   (func (export "new") (param i32 i32) (result (ref null $s))
//     local.get 0 table.get 0 ref.cast (ref null $f)
//     local.get 1 table.get 0 ref.cast (ref null $f)
//     struct.new $s)
//   ;; The value the function in field `which` returns, or -1 for null.
//   (func (export "call") (param (ref null $s) i32) (result i32) (local (ref null $f))
//     local.get 1
//     if (result (ref null $f)) local.get 0 struct.get $s 1
//     else local.get 0 struct.get $s 0 end
//     local.set 2
//     local.get 2 ref.is_null
//     if (result i32) i32.const -1 else local.get 2 call_ref $f end)
//   (func (export "set") (param (ref null $s) i32 i32)
//     local.get 1
//     if local.get 0 local.get 2 table.get 0 ref.cast (ref null $f) struct.set $s 1
//     else local.get 0 local.get 2 table.get 0 ref.cast (ref null $f) struct.set $s 0 end))
fn funcref_probe_bytes() -> Vec<u8> {
    let count = FUNCREF_PROBE_FUNCTIONS;
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        5,
        &[
            0x60, 0, 1, 0x7f, // 0: $f
            0x5f, 2, 0x63, 0, 1, 0x63, 0, 1, // 1: $s
            0x60, 2, 0x7f, 0x7f, 1, 0x63, 1, // 2: (i32 i32) -> (ref null $s)
            0x60, 2, 0x63, 1, 0x7f, 1, 0x7f, // 3: ((ref null $s) i32) -> i32
            0x60, 3, 0x63, 1, 0x7f, 0x7f, 0, // 4: ((ref null $s) i32 i32)
        ],
    );
    let mut functions = vec![0u8; count as usize];
    functions.extend_from_slice(&[2, 3, 4]);
    section(&mut module, 3, functions.len(), &functions);
    let mut table = vec![0x70, 0x00];
    leb(count + 1, &mut table);
    section(&mut module, 4, 1, &table);
    let mut exports = Vec::new();
    for (index, name) in ["new", "call", "set"].into_iter().enumerate() {
        leb(name.len() as u32, &mut exports);
        exports.extend_from_slice(name.as_bytes());
        exports.push(0);
        leb(count + index as u32, &mut exports);
    }
    section(&mut module, 7, 3, &exports);
    let mut elements = vec![0x00, 0x41, 0x00, 0x0b];
    leb(count, &mut elements);
    for index in 0..count {
        leb(index, &mut elements);
    }
    section(&mut module, 9, 1, &elements);
    let mut code = Vec::new();
    for index in 0..count {
        // `i32.const index`, as a signed LEB128 of two bytes.
        code.extend(body(
            0,
            &[0x41, (index & 0x7f) as u8 | 0x80, (index >> 7) as u8],
        ));
    }
    // `table.get 0; ref.cast (ref null $f)`
    const ENTRY: [u8; 5] = [0x25, 0x00, 0xfb, 0x17, 0x00];
    let mut new = vec![0x20, 0x00];
    new.extend_from_slice(&ENTRY);
    new.extend_from_slice(&[0x20, 0x01]);
    new.extend_from_slice(&ENTRY);
    new.extend_from_slice(&[0xfb, 0x00, 0x01]);
    code.extend(body(0, &new));
    // One local of type `(ref null $f)`.
    let call = [
        0x01, 0x01, 0x63, 0x00, // locals
        0x20, 0x01, 0x04, 0x63, 0x00, // local.get 1; if (result (ref null $f))
        0x20, 0x00, 0xfb, 0x02, 0x01, 0x01, // local.get 0; struct.get $s 1
        0x05, 0x20, 0x00, 0xfb, 0x02, 0x01, 0x00, 0x0b, // else ... struct.get $s 0; end
        0x21, 0x02, 0x20, 0x02, 0xd1, // local.set 2; local.get 2; ref.is_null
        0x04, 0x7f, 0x41, 0x7f, 0x05, 0x20, 0x02, 0x14, 0x00, 0x0b, // if .. -1 else call_ref $f end
        0x0b,
    ];
    leb(call.len() as u32, &mut code);
    code.extend_from_slice(&call);
    let mut set = vec![0x20, 0x01, 0x04, 0x40, 0x20, 0x00, 0x20, 0x02];
    set.extend_from_slice(&ENTRY);
    set.extend_from_slice(&[0xfb, 0x05, 0x01, 0x01, 0x05, 0x20, 0x00, 0x20, 0x02]);
    set.extend_from_slice(&ENTRY);
    set.extend_from_slice(&[0xfb, 0x05, 0x01, 0x00, 0x0b]);
    code.extend(body(0, &set));
    section(&mut module, 10, count as usize + 3, &code);
    module
}

/// A funcref stored into the GC heap is an id of the Store's table. The
/// translation takes the id of the null funcref to be zero and reads every
/// other id from the runtime's cache of addresses it has interned, calling
/// the runtime only when the cache holds another address. Six hundred
/// functions share the cache's 256 entries, in two Stores and in two
/// instances of one Store, across collections; every field must still call
/// the function that was stored in it.
#[cfg(target_pointer_width = "64")]
#[test]
fn a_stored_funcref_is_the_function_that_was_stored() {
    let mut config = Config::new();
    configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let module = Module::new(&engine, funcref_probe_bytes()).unwrap();
    let count = FUNCREF_PROBE_FUNCTIONS as i32;
    // Index `count` is the table's null entry.
    let expected = |index: i32| if index == count { -1 } else { index };
    let second = |index: i32| (index * 7 + 3) % (count + 1);

    let mut stores = [Store::new(&engine, ()), Store::new(&engine, ())];
    let instances = [
        Instance::new(&mut stores[0], &module, &[]).unwrap(),
        Instance::new(&mut stores[1], &module, &[]).unwrap(),
        // A second instance in the first Store: other addresses for the
        // same functions, in the same table and cache.
        Instance::new(&mut stores[0], &module, &[]).unwrap(),
    ];
    let store_of = [0usize, 1, 0];
    fn call(store: &mut Store<()>, instance: &Instance, name: &str, arguments: &[Val]) -> Val {
        let function = instance.get_func(&mut *store, name).unwrap();
        let mut output = [Val::I32(0)];
        let results = if name == "set" { 0 } else { 1 };
        function
            .call(&mut *store, arguments, &mut output[..results])
            .unwrap();
        output[0]
    }

    for round in 0..3 {
        let mut made = Vec::new();
        // Interleave the instances so that each Store's cache is consulted
        // right after the other Store filled its own.
        for index in 0..=count {
            for (which, instance) in instances.iter().enumerate() {
                let store = &mut stores[store_of[which]];
                let object = call(
                    store,
                    instance,
                    "new",
                    &[Val::I32(index), Val::I32(second(index))],
                );
                assert!(matches!(object, Val::AnyRef(Some(_))));
                made.push((which, index, object));
            }
        }
        for store in &mut stores {
            store.gc(None).unwrap();
        }
        for (which, index, object) in &made {
            let store = &mut stores[store_of[*which]];
            let instance = &instances[*which];
            assert_eq!(
                call(store, instance, "call", &[*object, Val::I32(0)]).unwrap_i32(),
                expected(*index),
                "round {round}, instance {which}, first field of {index}"
            );
            assert_eq!(
                call(store, instance, "call", &[*object, Val::I32(1)]).unwrap_i32(),
                expected(second(*index)),
                "round {round}, instance {which}, second field of {index}"
            );
        }
        // A struct made by one instance is read by the other instance of
        // its Store: the ids are the Store's, not the instance's.
        for (which, index, object) in made.iter().filter(|(which, ..)| *which != 1).step_by(37) {
            let other = &instances[2 - *which];
            assert_eq!(
                call(&mut stores[0], other, "call", &[*object, Val::I32(0)]).unwrap_i32(),
                expected(*index)
            );
        }
        // Stores into existing structs: to another function, to null and
        // back, each read at once and again after a collection.
        for (which, index, object) in made.iter().step_by(5) {
            let store = &mut stores[store_of[*which]];
            let instance = &instances[*which];
            let replacement = (index * 11 + round) % (count + 1);
            for (field, value) in [(0, replacement), (1, count), (1, *index)] {
                call(
                    store,
                    instance,
                    "set",
                    &[*object, Val::I32(field), Val::I32(value)],
                );
                assert_eq!(
                    call(store, instance, "call", &[*object, Val::I32(field)]).unwrap_i32(),
                    expected(value)
                );
            }
        }
        for store in &mut stores {
            store.gc(None).unwrap();
        }
        for (which, index, object) in made.iter().step_by(5) {
            let store = &mut stores[store_of[*which]];
            let instance = &instances[*which];
            assert_eq!(
                call(store, instance, "call", &[*object, Val::I32(0)]).unwrap_i32(),
                expected((index * 11 + round) % (count + 1))
            );
            assert_eq!(
                call(store, instance, "call", &[*object, Val::I32(1)]).unwrap_i32(),
                expected(*index)
            );
        }
    }
}
