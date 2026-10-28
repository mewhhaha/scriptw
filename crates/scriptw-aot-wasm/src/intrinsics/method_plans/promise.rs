use super::*;

pub(super) const PROMISE_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::native(StandardBuiltinId::PromisePrototypeThen),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromisePrototypeCatch),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromisePrototypeFinally),
];

pub(super) const PROMISE_CONSTRUCTOR: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseResolve),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseReject),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseAll),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseAllSettled),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseAllKeyed),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseAllSettledKeyed),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseAny),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseRace),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseWithResolvers),
    IntrinsicMethodSpec::native(StandardBuiltinId::PromiseTry),
];

// The planner's existing publication arrays remain authoritative.
const _: () = {
    let pairs: [(&[IntrinsicMethodSpec], &[StandardBuiltinId]); 2] = [
        (
            PROMISE_PROTOTYPE,
            &crate::intrinsics::promise::PROMISE_PROTOTYPE_METHOD_PUBLICATIONS,
        ),
        (
            PROMISE_CONSTRUCTOR,
            &crate::intrinsics::promise::PROMISE_STATIC_METHOD_PUBLICATIONS,
        ),
    ];
    let mut plan = 0;
    while plan < pairs.len() {
        let (rows, builtins) = pairs[plan];
        assert!(rows.len() == builtins.len());
        let mut row = 0;
        while row < rows.len() {
            assert!(rows[row].builtin as u32 == builtins[row] as u32);
            row += 1;
        }
        plan += 1;
    }
};
