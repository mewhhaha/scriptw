use super::*;

pub(super) const REG_EXP_PROTOTYPE_BEFORE_ACCESSORS: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("compile", StandardBuiltinId::RegExpPrototypeCompile),
    IntrinsicMethodSpec::named("toString", StandardBuiltinId::RegExpPrototypeToString),
    IntrinsicMethodSpec::named("exec", StandardBuiltinId::RegExpPrototypeExec),
    IntrinsicMethodSpec::named("test", StandardBuiltinId::RegExpPrototypeTest),
];

pub(super) const REG_EXP_SYMBOLS: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::symbol(
        scriptw_ir::WellKnownSymbol::Match,
        StandardBuiltinId::RegExpPrototypeSymbolMatch,
        true,
    ),
    IntrinsicMethodSpec::symbol(
        scriptw_ir::WellKnownSymbol::MatchAll,
        StandardBuiltinId::RegExpPrototypeSymbolMatchAll,
        true,
    ),
    IntrinsicMethodSpec::symbol(
        scriptw_ir::WellKnownSymbol::Replace,
        StandardBuiltinId::RegExpPrototypeSymbolReplace,
        true,
    ),
    IntrinsicMethodSpec::symbol(
        scriptw_ir::WellKnownSymbol::Search,
        StandardBuiltinId::RegExpPrototypeSymbolSearch,
        true,
    ),
    IntrinsicMethodSpec::symbol(
        scriptw_ir::WellKnownSymbol::Split,
        StandardBuiltinId::RegExpPrototypeSymbolSplit,
        true,
    ),
];
