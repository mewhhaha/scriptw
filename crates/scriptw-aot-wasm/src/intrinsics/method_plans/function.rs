use super::*;

pub(super) const FUNCTION_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("call", StandardBuiltinId::FunctionPrototypeCall),
    IntrinsicMethodSpec::named("apply", StandardBuiltinId::FunctionPrototypeApply),
    IntrinsicMethodSpec::named("bind", StandardBuiltinId::FunctionPrototypeBind),
    IntrinsicMethodSpec::named("toString", StandardBuiltinId::FunctionPrototypeToString),
];
