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
//!
//! The last probe checks placement instead: the product configuration reads
//! the branch hinting section, and what a hinted branch rarely reaches is
//! laid out behind the function's other code.
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

fn signed_leb(mut value: i64, bytes: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            return;
        }
    }
}

/// `local.get 1; i32.const marker; i32.xor`: code that holds `marker` as an
/// immediate, so that its place in the native code can be found.
fn marked(marker: i32) -> Vec<u8> {
    let mut code = vec![0x20, 0x01, 0x41];
    signed_leb(i64::from(marker), &mut code);
    code.push(0x73);
    code
}

/// A two-way branch of each hinted kind, each with a marker on both sides:
/// with the branch hinted unlikely taken (`likely` false) the first marker
/// of its pair is in the code the hint calls rare, with the opposite hint
/// the second is. The last function's `if` is always hinted unlikely and
/// holds a nested `if`, whose arms are rare with it.
const HINTED_PAIRS: [(i32, i32); 6] = [
    (0x5a10_3701, 0x5a10_3702), // if: consequent, alternative
    (0x5a10_3703, 0x5a10_3704), // br_if: its label, the fallthrough
    (0x5a10_3705, 0x5a10_3706), // br_on_non_null
    (0x5a10_3707, 0x5a10_3708), // br_on_null
    (0x5a10_3709, 0x5a10_370a), // br_on_cast
    (0x5a10_370b, 0x5a10_370c), // br_on_cast_fail
];
const RARE_NESTED: [i32; 2] = [0x5a10_370d, 0x5a10_370e];
const AFTER_RARE: i32 = 0x5a10_370f;

fn hinted_bytes(likely: bool) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        3,
        &[
            0x60, 2, 0x7f, 0x7f, 1, 0x7f, // 0: (i32, i32) -> i32
            0x60, 2, 0x6e, 0x7f, 1, 0x7f, // 1: (anyref, i32) -> i32
            0x5f, 1, 0x7f, 0, // 2: $s
        ],
    );
    section(&mut module, 3, 7, &[0, 0, 1, 1, 1, 1, 0]);

    // Each body declares one further `i32` local, local 2; `body` writes
    // that declaration in three bytes, which precede the code.
    const DECLARATION: usize = 3;
    let mut code = Vec::new();
    let mut hints = Vec::new();
    let mut function = |instructions: &[&[u8]], hinted: usize, likely: bool| {
        let offset = DECLARATION
            + instructions[..hinted]
                .iter()
                .map(|i| i.len())
                .sum::<usize>();
        hints.push((offset as u32, likely));
        code.extend(body(1, &instructions.concat()));
    };
    let [(then, otherwise), (label, fallthrough), (non_null, null), (is_null, not_null), (cast, not_cast), (failed, passed)] =
        HINTED_PAIRS.map(|(rare, usual)| (marked(rare), marked(usual)));
    const RETURN: &[u8] = &[0x0f];
    const END: &[u8] = &[0x0b];
    const DROP: &[u8] = &[0x1a];
    const GET_0: &[u8] = &[0x20, 0x00];
    const SET_2: &[u8] = &[0x21, 0x02];
    // `if (result i32) .. else .. end`
    function(
        &[GET_0, &[0x04, 0x7f], &then, &[0x05], &otherwise, END],
        1,
        likely,
    );
    // `block $done block $slow .. br_if $slow .. br $done end .. end local.get 2`
    function(
        &[
            &[0x02, 0x40, 0x02, 0x40],
            GET_0,
            &[0x0d, 0x00],
            &fallthrough,
            SET_2,
            &[0x0c, 0x01],
            END,
            &label,
            SET_2,
            END,
            &[0x20, 0x02],
        ],
        2,
        likely,
    );
    // `block (result (ref any)) local.get 0 br_on_non_null 0 .. return end drop ..`
    function(
        &[
            &[0x02, 0x64, 0x6e],
            GET_0,
            &[0xd6, 0x00],
            &null,
            RETURN,
            END,
            DROP,
            &non_null,
        ],
        2,
        likely,
    );
    // `block local.get 0 br_on_null 0 drop .. return end ..`
    function(
        &[
            &[0x02, 0x40],
            GET_0,
            &[0xd5, 0x00],
            DROP,
            &not_null,
            RETURN,
            END,
            &is_null,
        ],
        2,
        likely,
    );
    // `block (result (ref $s)) local.get 0 br_on_cast 0 anyref (ref $s) drop .. return end drop ..`
    function(
        &[
            &[0x02, 0x64, 0x02],
            GET_0,
            &[0xfb, 0x18, 0x01, 0x00, 0x6e, 0x02],
            DROP,
            &not_cast,
            RETURN,
            END,
            DROP,
            &cast,
        ],
        2,
        likely,
    );
    // `block (result anyref) local.get 0 br_on_cast_fail 0 anyref (ref $s) drop .. return end drop ..`
    function(
        &[
            &[0x02, 0x6e],
            GET_0,
            &[0xfb, 0x19, 0x01, 0x00, 0x6e, 0x02],
            DROP,
            &passed,
            RETURN,
            END,
            DROP,
            &failed,
        ],
        2,
        likely,
    );
    // `local.get 0 if local.get 1 if (result i32) .. else .. end local.set 2 end local.get 2 ..`
    function(
        &[
            GET_0,
            &[0x04, 0x40, 0x20, 0x01, 0x04, 0x7f],
            &marked(RARE_NESTED[0]),
            &[0x05],
            &marked(RARE_NESTED[1]),
            END,
            SET_2,
            END,
            &[0x20, 0x02, 0x1a],
            &marked(AFTER_RARE),
        ],
        1,
        false,
    );

    // `metadata.code.branch_hint`: per function its index and its hints,
    // each a body offset, the size 1 and whether the branch is likely taken.
    let name = b"metadata.code.branch_hint";
    let mut custom = vec![name.len() as u8];
    custom.extend_from_slice(name);
    leb(hints.len() as u32, &mut custom);
    for (index, (offset, likely)) in hints.iter().enumerate() {
        leb(index as u32, &mut custom);
        custom.push(1);
        leb(*offset, &mut custom);
        custom.extend_from_slice(&[1, u8::from(*likely)]);
    }
    module.push(0);
    leb(custom.len() as u32, &mut module);
    module.extend_from_slice(&custom);
    section(&mut module, 10, 7, &code);
    module
}

/// The branch hinting section reaches the native code of the product
/// configuration: a branch hinted either way puts the side the hint calls
/// rare behind the other, for `if`, `br_if` and the four `br_on_*` forms,
/// under both compilation policies and when precompiled, and the blocks
/// inside a rare arm go with it.
#[cfg(target_pointer_width = "64")]
#[test]
fn a_hinted_branch_lays_its_rare_side_out_behind_the_other() {
    for speed in [false, true] {
        let mut config = Config::new();
        configure_wasm_runtime(&mut config);
        if speed {
            config.cranelift_opt_level(wasmtime::OptLevel::Speed);
            config.cranelift_regalloc_algorithm(wasmtime::RegallocAlgorithm::Backtracking);
        }
        let engine = wasmtime::Engine::new(&config).unwrap();
        let place = |module: &Module, marker: i32| -> usize {
            let text = module.text();
            let found: Vec<usize> = text
                .windows(4)
                .enumerate()
                .filter(|(_, window)| *window == marker.to_le_bytes())
                .map(|(at, _)| at)
                .collect();
            assert_eq!(found.len(), 1, "marker {marker:#x} in the native code");
            found[0]
        };
        let unlikely = Module::new(&engine, hinted_bytes(false)).unwrap();
        let likely = Module::new(&engine, hinted_bytes(true)).unwrap();
        // A precompiled module is the same code.
        let precompiled = engine.precompile_module(&hinted_bytes(false)).unwrap();
        // SAFETY: the bytes are this engine's own output, unmodified.
        let loaded = unsafe { Module::deserialize(&engine, &precompiled) }.unwrap();
        assert_eq!(loaded.text(), unlikely.text(), "speed {speed}");
        for (rare_when_unlikely, rare_when_likely) in HINTED_PAIRS {
            assert!(
                place(&unlikely, rare_when_unlikely) > place(&unlikely, rare_when_likely),
                "speed {speed}: {rare_when_unlikely:#x} is behind when its branch is unlikely"
            );
            assert!(
                place(&likely, rare_when_likely) > place(&likely, rare_when_unlikely),
                "speed {speed}: {rare_when_likely:#x} is behind when the branch is likely"
            );
        }
        for module in [&unlikely, &likely] {
            for nested in RARE_NESTED {
                assert!(
                    place(module, nested) > place(module, AFTER_RARE),
                    "speed {speed}: {nested:#x} is inside a rare arm"
                );
            }
        }
    }
}
