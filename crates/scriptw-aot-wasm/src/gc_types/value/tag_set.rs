//! Membership tests over small closed i32 domains.
//!
//! A set is emitted as its maximal runs of consecutive wire values. A run of
//! one value is an `i32.eq`; a longer run is one unsigned range test
//! (`value - first < length`, or `value < length` from zero). Runs are joined
//! with `i32.or`, so the test is never seeded with a constant zero. Each test
//! reads the local again and leaves a 0/1 `i32`.
use super::*;

/// A set of runtime value tags, held as a bit set over their wire values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TagSet(u32);

impl TagSet {
    /// Every tag must fit the bit set and appear once; the set is non-empty.
    pub(crate) const fn of(tags: &[WasmRuntimeValueTag]) -> Self {
        let mut bits = 0u32;
        let mut index = 0;
        while index < tags.len() {
            let wire = tags[index] as i32;
            assert!(
                0 <= wire && wire < u32::BITS as i32,
                "tag wire value outside the set"
            );
            let bit = 1u32 << wire;
            assert!(bits & bit == 0, "a tag appears twice in a tag set");
            bits |= bit;
            index += 1;
        }
        assert!(bits != 0, "an empty tag set has no test");
        Self(bits)
    }

    /// The number of maximal runs of consecutive wire values: each is one test.
    pub(crate) const fn run_count(self) -> u32 {
        (self.0 & !(self.0 << 1)).count_ones()
    }

    pub(crate) const fn contains(self, tag: WasmRuntimeValueTag) -> bool {
        self.0 & (1 << tag as i32) != 0
    }

    fn runs(self) -> impl Iterator<Item = ValueRun> {
        let mut rest = self.0;
        std::iter::from_fn(move || {
            if rest == 0 {
                return None;
            }
            let first = rest.trailing_zeros();
            let length = (rest >> first).trailing_ones();
            rest &= !(((1u64 << length) - 1) << first) as u32;
            Some(ValueRun {
                first: first as i32,
                length,
            })
        })
    }
}

/// Object, Array, Function and Arguments: every Object value.
pub(crate) const OBJECT_TAGS: TagSet = TagSet::of(&[
    WasmRuntimeValueTag::Object,
    WasmRuntimeValueTag::Array,
    WasmRuntimeValueTag::Function,
    WasmRuntimeValueTag::Arguments,
]);
/// A [[Prototype]] value: null or an Object.
pub(crate) const OBJECT_OR_NULL_TAGS: TagSet = TagSet::of(&[
    WasmRuntimeValueTag::Null,
    WasmRuntimeValueTag::Object,
    WasmRuntimeValueTag::Array,
    WasmRuntimeValueTag::Function,
    WasmRuntimeValueTag::Arguments,
]);
/// Every ECMAScript language type except Object.
pub(crate) const PRIMITIVE_TAGS: TagSet = TagSet::of(&[
    WasmRuntimeValueTag::Undefined,
    WasmRuntimeValueTag::Null,
    WasmRuntimeValueTag::Boolean,
    WasmRuntimeValueTag::Number,
    WasmRuntimeValueTag::String,
    WasmRuntimeValueTag::Symbol,
    WasmRuntimeValueTag::BigInt,
]);
// The wire numbering keeps these sets to the run counts their tests are sized
// for: one range test for every Object, null plus that range for a prototype,
// and Undefined..=Symbol plus BigInt for the primitives. A renumbering that
// splits a run fails here rather than lengthening every emitted test.
const _: () = {
    assert!(OBJECT_TAGS.run_count() == 1);
    assert!(OBJECT_OR_NULL_TAGS.run_count() == 2);
    assert!(PRIMITIVE_TAGS.run_count() == 2);
    assert!(PRIMITIVE_TAGS.0 & OBJECT_TAGS.0 == 0);
    assert!(
        PRIMITIVE_TAGS.0 | OBJECT_TAGS.0 == (1 << (WasmRuntimeValueTag::BigInt as i32 + 1)) - 1
    );
};

/// Pushes whether `tag` holds one of `tags`, as a 0/1 i32.
pub(crate) fn emit_tag_in(tag: I32Local, tags: TagSet, function: &mut Function) {
    emit_runs(tag, tags.runs(), function);
}

/// Pushes whether `value` is one of `values`, as a 0/1 i32. For closed
/// domains whose encodings are not a const tag set (enum wire values).
pub(crate) fn emit_i32_in(
    value: I32Local,
    values: impl IntoIterator<Item = i32>,
    function: &mut Function,
) {
    let mut sorted = values.into_iter().collect::<Vec<_>>();
    sorted.sort_unstable();
    sorted.dedup();
    assert!(!sorted.is_empty(), "an empty value set has no test");
    let mut runs = Vec::new();
    for value in sorted {
        match runs.last_mut() {
            Some(ValueRun { first, length })
                if i64::from(*first) + i64::from(*length) == i64::from(value) =>
            {
                *length += 1;
            }
            _ => runs.push(ValueRun {
                first: value,
                length: 1,
            }),
        }
    }
    emit_runs(value, runs, function);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ValueRun {
    first: i32,
    length: u32,
}

fn emit_runs(value: I32Local, runs: impl IntoIterator<Item = ValueRun>, function: &mut Function) {
    let mut joined = false;
    for ValueRun { first, length } in runs {
        value.load(function);
        if length == 1 {
            function.instruction(&Instruction::I32Const(first));
            function.instruction(&Instruction::I32Eq);
        } else {
            // A run never wraps: it ends at or before i32::MAX + 1, so the
            // wrapped difference is below `length` exactly inside the run.
            if first != 0 {
                function.instruction(&Instruction::I32Const(first));
                function.instruction(&Instruction::I32Sub);
            }
            function.instruction(&Instruction::I32Const(length as i32));
            function.instruction(&Instruction::I32LtU);
        }
        if joined {
            function.instruction(&Instruction::I32Or);
        }
        joined = true;
    }
    assert!(joined, "an empty value set has no test");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_split_at_gaps() {
        assert_eq!(
            OBJECT_OR_NULL_TAGS.runs().collect::<Vec<_>>(),
            [
                ValueRun {
                    first: 1,
                    length: 1
                },
                ValueRun {
                    first: 6,
                    length: 4
                },
            ]
        );
        assert_eq!(
            PRIMITIVE_TAGS.runs().collect::<Vec<_>>(),
            [
                ValueRun {
                    first: 0,
                    length: 6
                },
                ValueRun {
                    first: 10,
                    length: 1
                },
            ]
        );
        let all = TagSet(u32::MAX);
        assert_eq!(
            all.runs().collect::<Vec<_>>(),
            [ValueRun {
                first: 0,
                length: 32
            }]
        );
        assert!(OBJECT_TAGS.contains(WasmRuntimeValueTag::Arguments));
        assert!(!OBJECT_TAGS.contains(WasmRuntimeValueTag::Null));
    }

    #[test]
    fn range_tests_match_membership_for_every_i32_edge() {
        // Evaluate the emitted run tests in Rust over each wire value and the
        // wrapping edges of the subtraction.
        fn member(runs: &[ValueRun], value: i32) -> bool {
            runs.iter().any(|run| {
                if run.length == 1 {
                    value == run.first
                } else {
                    (value.wrapping_sub(run.first) as u32) < run.length
                }
            })
        }
        for set in [OBJECT_TAGS, OBJECT_OR_NULL_TAGS, PRIMITIVE_TAGS] {
            let runs = set.runs().collect::<Vec<_>>();
            for value in (-40..40).chain([i32::MIN, i32::MIN + 1, i32::MAX, i32::MAX - 1]) {
                let expected = (0..32).contains(&value) && set.0 & (1u32 << value) != 0;
                assert_eq!(member(&runs, value), expected, "{set:?} {value}");
            }
        }
    }
}
