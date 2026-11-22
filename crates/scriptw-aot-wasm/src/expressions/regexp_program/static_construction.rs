//! Constant `RegExp(literal)` / `new RegExp(literal)` sites bound at compile time.
//!
//! The site shape only *nominates* the binding. The actual once-evaluated callee
//! decides at run time: only the running Realm's own native %RegExp% takes the
//! bound route, and every other callee (a replaced global, a subclass, a Proxy,
//! a foreign Realm's %RegExp%) keeps the original Call/Construct dispatch with
//! the same operands. Spec reasoning for the bound route, for a String pattern
//! P and String flags F (22.2.4.1 RegExp ( pattern, flags )):
//!
//! - IsRegExp(P) is false without observable steps: P is not an Object.
//! - NewTarget is %RegExp% itself (Call: step 4a; `new RegExp`: the callee),
//!   so the P-is-RegExp `constructor` read and GetPrototypeFromConstructor's
//!   `prototype` Get are unobservable: `RegExp.prototype` is a non-writable,
//!   non-configurable data property whose value is this Realm's
//!   %RegExp.prototype%.
//! - ToString(P) and ToString(F) are identities.
//! - RegExpInitialize's parse already succeeded at compile time (an
//!   `InvalidSyntax` or unsupported pattern never forms a site, so its
//!   SyntaxError still comes from the native constructor at the same point).
//! - [[LegacyFeaturesEnabled]] is true: NewTarget is thisRealm.%RegExp%.
//!
//! The bound route therefore allocates exactly what RegExpCreate allocates for
//! a literal: the literal allocation helper, with this Realm's prototype and
//! Realm, `lastIndex` 0, the source/flags Strings and the collected program.

use super::*;
use crate::functions::NonArrayRealmIntrinsicSlot;
use scriptw_ir::{EnvironmentIdentifierOperationIr, StandardBuiltinId, BUILTIN_REGEXP_FUNCTION_ID};
use std::borrow::Cow;

/// A Call/Construct site whose operands are constant RegExp source text and
/// flags with a compile-time program. Only [`Self::of`] constructs one, so the
/// collector (which interns the strings and queues the program) and the
/// emitter (which binds them) agree on every site by construction.
pub(crate) struct ConstantRegExpConstructionSite<'a> {
    source: &'a str,
    flags: &'a str,
    program: Cow<'a, RegExpProgram>,
}

impl<'a> ConstantRegExpConstructionSite<'a> {
    /// `static_compilation` is the lowering's own verdict for a resolved
    /// %RegExp% callee. Without one, a callee that can only nominate %RegExp%
    /// (a global read spelled `RegExp`, or possible-target knowledge) is
    /// compiled here. A third argument is never admitted: it would be an
    /// arbitrary expression with its own effects.
    pub(crate) fn of(
        callee: &TypedExpr,
        args: &'a [TypedExpr],
        static_compilation: Option<&'a StaticRegExpCompilation>,
    ) -> Option<Self> {
        let (source, flags) = match args {
            []
            | [TypedExpr {
                expr: ExprIr::Undefined,
                ..
            }] => ("", ""),
            [TypedExpr {
                expr: ExprIr::String(pattern),
                ..
            }] => (pattern.as_str(), ""),
            [TypedExpr {
                expr: ExprIr::String(pattern),
                ..
            }, TypedExpr {
                expr: ExprIr::String(flags),
                ..
            }] => (pattern.as_str(), flags.as_str()),
            _ => return None,
        };
        let program = match static_compilation {
            Some(StaticRegExpCompilation::Program(program)) => Cow::Borrowed(program),
            // The native constructor reports this after its own steps.
            Some(StaticRegExpCompilation::InvalidSyntax { .. }) => return None,
            None if Self::callee_nominates_regexp(callee) => Cow::Owned(
                RegExpProgram::compile_utf16(&scriptw_ir::decode_js_string_utf16(source), flags)
                    .ok()?,
            ),
            None => return None,
        };
        Some(Self {
            source,
            flags,
            program,
        })
    }

    fn callee_nominates_regexp(callee: &TypedExpr) -> bool {
        const REGEXP: &str = "RegExp";
        match &callee.expr {
            ExprIr::GlobalPropertyRead { name } | ExprIr::GlobalIdentifierRead { name }
                if name == REGEXP =>
            {
                true
            }
            ExprIr::EnvironmentIdentifier(identifier)
                if identifier.name == REGEXP
                    && matches!(identifier.operation, EnvironmentIdentifierOperationIr::Read) =>
            {
                true
            }
            _ => callee
                .function_targets
                .known_targets()
                .contains(BUILTIN_REGEXP_FUNCTION_ID),
        }
    }

    pub(crate) fn source(&self) -> &'a str {
        self.source
    }

    pub(crate) fn flags(&self) -> &'a str {
        self.flags
    }

    pub(crate) fn program(&self) -> &RegExpProgram {
        &self.program
    }
}

impl FunctionBuilder<'_> {
    /// Writes the bound RegExp object to `output` when the evaluated `callee`
    /// is this Realm's native %RegExp%. The returned flag is 1 exactly then;
    /// the caller emits the original dispatch, with the same operands, under
    /// its zero arm.
    pub(crate) fn emit_constant_regexp_construction_if_intrinsic(
        &mut self,
        callee: &ValueLocals,
        site: &ConstantRegExpConstructionSite<'_>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let handled = self.runtime_schema().reserve_i32_local(function);
        handled.set_constant(0, function);
        self.emit_with_current_native_builtin(
            callee,
            StandardBuiltinId::RegExpConstructor,
            function,
            |builder, function| {
                builder.emit_constant_regexp_object(site, output, function)?;
                handled.set_constant(1, function);
                Ok(())
            },
        )?;
        Ok(handled)
    }

    /// RegExpCreate in the current Realm, which the guard proved is the
    /// callee's own Realm.
    fn emit_constant_regexp_object(
        &mut self,
        site: &ConstantRegExpConstructionSite<'_>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.load_current_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::RegExpPrototype,
            &prototype,
            function,
        );
        let source = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(site.source(), function)?,
            function,
        );
        let flags = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(site.flags(), function)?,
            function,
        );
        let program = schema.reserve_gc_local(function).initialize(
            self.emit_collected_regexp_program(site.program(), function)?,
            function,
        );
        let object = schema.reserve_gc_local(function).initialize(
            schema.helper_reference_on_stack(schema.call_helper(
                crate::runtime_helpers::RegExpLiteralAllocateArguments::new(
                    &prototype, &source, &flags, &program, &realm,
                ),
                self.runtime_helper_base()?,
                function,
            )),
            function,
        );
        output.set_reference(&object, schema, function);
        object.clear(function);
        program.clear(function);
        flags.clear(function);
        source.clear(function);
        prototype.clear(function);
        realm.clear(function);
        Ok(())
    }
}
