//! Host-declared Realm requests are distinct from requests by source records.

use crate::*;

pub(super) const REALM_IMPORT_DISPATCHER: &str = "$scriptw$module$realmImport";

pub fn scan_script_realm_module_requests(
    source: &scriptw_front::ParsedScript,
) -> Vec<ModuleRequestKeyIr> {
    source.with_compiler_session(literal_realm_requests)
}

impl ModuleGraphIr {
    pub(super) fn realm_import_dispatcher_source(&self) -> String {
        let mut text =
            format!("async function {REALM_IMPORT_DISPATCHER}(specifier) {{ await void 0;");
        for (request, resolution) in &self.realm_requests {
            if !request.attributes().is_empty() {
                continue;
            }
            text.push_str(" if (specifier === ");
            super::namespace::push_js_string_literal(&mut text, request.specifier());
            text.push_str(") {");
            match resolution {
                RealmModuleResolutionIr::Loaded(module) => {
                    text.push_str(" await ");
                    text.push_str(&super::dynamic::module_evaluator_name(*module));
                    // Namespace is captured by importValue's intrinsic export
                    // continuation. Resolving it here would observe export `then`.
                    text.push_str("; return; }");
                }
                RealmModuleResolutionIr::Rejected(message) => {
                    text.push_str(" throw new $scriptw$module$SyntaxError(");
                    super::namespace::push_js_string_literal(&mut text, message);
                    text.push_str("); }");
                }
            }
        }
        text.push_str(
            " throw new $scriptw$module$TypeError(\"Cannot find module \" + specifier); }",
        );
        text
    }
}

/// Original host/load/link rejection remains data in the compiled catalog;
/// importValue's outer promise decides how it crosses the Realm boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealmModuleResolutionIr {
    Loaded(ModuleUnitId),
    Rejected(String),
}

pub(super) fn literal_realm_requests<'ast>(
    source: &'ast impl VisitWith,
    interner: &'ast Interner,
) -> Vec<ModuleRequestKeyIr> {
    let Some(mut requests) = RealmRequestScan::new(interner) else {
        return Vec::new();
    };
    let _ = source.visit_with(&mut requests);
    requests.into_requests()
}

pub(super) struct RealmRequestScan<'a> {
    interner: &'a Interner,
    import_value: boa_interner::Sym,
    requests: BTreeSet<ModuleRequestKeyIr>,
}

impl<'a> RealmRequestScan<'a> {
    pub(super) fn new(interner: &'a Interner) -> Option<Self> {
        Some(Self {
            interner,
            import_value: interner.get("importValue")?,
            requests: BTreeSet::new(),
        })
    }

    pub(super) fn observe(&mut self, expression: &Expression) {
        let Expression::Call(call) = expression else {
            return;
        };
        let Expression::PropertyAccess(PropertyAccess::Simple(access)) = call.function().flatten()
        else {
            return;
        };
        let name = match access.field() {
            PropertyAccessField::Const(name) => name.sym(),
            PropertyAccessField::Expr(expression) => {
                let Expression::Literal(literal) = expression.flatten() else {
                    return;
                };
                let LiteralKind::String(name) = literal.kind() else {
                    return;
                };
                *name
            }
        };
        if name == self.import_value {
            if let Some(Expression::Literal(literal)) = call.args().first().map(Expression::flatten)
            {
                if let LiteralKind::String(specifier) = literal.kind() {
                    self.requests.insert(ModuleRequestKeyIr::plain(
                        self.interner.resolve_expect(*specifier).to_string(),
                    ));
                }
            }
        }
    }

    pub(super) fn into_requests(self) -> Vec<ModuleRequestKeyIr> {
        self.requests.into_iter().collect()
    }
}

impl<'ast> Visitor<'ast> for RealmRequestScan<'_> {
    type BreakTy = core::convert::Infallible;

    fn visit_expression(&mut self, expression: &'ast Expression) -> ControlFlow<Self::BreakTy> {
        self.observe(expression);
        expression.visit_with(self)
    }
}
