use super::*;

pub(super) const DISPOSABLE_STACK_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("use", StandardBuiltinId::DisposableStackPrototypeUse),
    IntrinsicMethodSpec::named("adopt", StandardBuiltinId::DisposableStackPrototypeAdopt),
    IntrinsicMethodSpec::named("defer", StandardBuiltinId::DisposableStackPrototypeDefer),
    IntrinsicMethodSpec::named("move", StandardBuiltinId::DisposableStackPrototypeMove),
];
