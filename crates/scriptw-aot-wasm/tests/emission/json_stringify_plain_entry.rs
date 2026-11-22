//! JSON.stringify in a plain context (no replacer function, no property
//! list, no gap) serializes through a small hot runtime entry that recurses
//! into itself; everything it cannot prove plain leaves it only by one tail
//! call to the general SerializeJSONProperty helper, so the plain path never
//! calls (and never returns into) the general helper.
use crate::property_read_plans::{calls, Module};
use scriptw_aot_wasm::{emit_with_rooted_snapshot, PromiseRejectionPolicy};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

fn direct_calls(body: &[Operator<'_>], target: u32) -> usize {
    body.iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == target))
        .count()
}

fn tail_calls(body: &[Operator<'_>], target: u32) -> usize {
    body.iter()
        .filter(
            |op| matches!(op, Operator::ReturnCall { function_index } if *function_index == target),
        )
        .count()
}

#[test]
fn plain_stringify_has_a_hot_entry_that_reaches_the_general_helper_only_by_tail_call() {
    std::thread::Builder::new()
        .name("json-stringify-plain-entry".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let ir = lower(
                &parse(
                    "export function stringify(value, replacer, space) { return JSON.stringify(value, replacer, space); }",
                    ParseOptions::module(),
                )
                .unwrap(),
            );
            let artifact = emit_with_rooted_snapshot(
                &ir,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            )
            .unwrap();
            let runtime_bytes = artifact.runtime().unwrap().bytes();
            Validator::new_with_features(WasmFeatures::all())
                .validate_all(runtime_bytes)
                .unwrap();
            let runtime = Module::read(runtime_bytes, true);
            let (hot, hot_body) = runtime.named("helper::json_stringify_plain_value");
            let (general, general_body) = runtime.named("helper::json_stringify_value");
            let (_, entry) = runtime.named("builtin::JSON.stringify");

            // The plain path never calls the general helper: the hot entry's
            // only route there is a single tail call, taken before anything
            // is observed.
            assert_eq!(direct_calls(hot_body, general), 0);
            assert_eq!(tail_calls(hot_body, general), 1);
            // Members recurse through the hot entry itself, an Array's and an
            // ordinary object's each.
            assert!(direct_calls(hot_body, hot) >= 2);
            assert_eq!(tail_calls(hot_body, hot), 0);
            // The general walk hands members of a plain context back to the
            // hot entry, and keeps its own recursion for every other context.
            assert!(calls(general_body, hot));
            assert!(calls(general_body, general));
            // The entry selects exactly one of the two for the root.
            assert!(calls(entry, hot) && calls(entry, general));
            // The hot entry owns no replacer, toJSON, unboxing or gap work.
            assert!(
                hot_body.len() < general_body.len(),
                "hot entry {} operators, general helper {}",
                hot_body.len(),
                general_body.len()
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
