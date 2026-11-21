//! Boa parser implementation.

mod cursor;
mod expression;
mod statement;

pub(crate) mod function;

#[cfg(test)]
mod tests;

use crate::{
    Error, Source,
    error::ParseResult,
    lexer::{Error as LexError, InputElement},
    parser::{
        cursor::Cursor,
        function::{FormalParameters, FunctionStatementList},
    },
    source::ReadChar,
};
use boa_ast::{
    Declaration, Position, Spanned, StatementList, StatementListItem,
    declaration::LexicalDeclaration,
    function::{FormalParameterList, FunctionBody},
    operations::{
        ContainsSymbol, all_private_identifiers_valid, check_labels, contains,
        contains_invalid_object_literal, lexically_declared_names, var_declared_names,
    },
    scope::Scope,
};
use boa_interner::{Interner, Sym};
use rustc_hash::FxHashSet;
use std::path::Path;

use self::statement::ModuleItemList;

/// Scriptw: the deepest nesting of recursive grammar productions a source may
/// have; deeper source is a SyntaxError. Each level is one enclosing
/// AssignmentExpression, Statement, function body, unary or prefix update
/// operand, `await` operand, right-associative `**` operand, `new` callee,
/// class heritage or binding pattern. V8 reports a RangeError for comparable
/// depths (about 2,000 to 4,000 nested parentheses or blocks).
///
/// The parser recurses once per level, but loops build some trees deeper than
/// the parser recursed: binary, logical, `??` and comma chains, and member,
/// call, tagged template and optional chains add one tree level per operator.
/// So the bound is on the height of the tree, in units: each [`NestingCharge`]
/// costs its [`NestingCharge::units`], and no path from the root may exceed
/// [`MAX_NESTING_UNITS`]. Every phase after the parser recurses once per
/// level except through binary, logical, `??` and comma chains, which Boa's
/// visitors, the tree's `Drop` and Scriptw's lowering walk iteratively.
/// Template literals are flat too: the parser loops over their parts, and
/// lowering folds them like a chain's operands. Lowering bounds the IR those
/// chains and templates become: it binds the value so far to a temporary
/// every 256 links and after an operand whose own chains are deep, and
/// arranges the temporaries as a tree, so a chain is at most about 256 IR
/// levels above its operands (two above one much deeper operand, about
/// log2(k) + 1 above k comparably deep ones). Every other construct costs
/// about one IR level per tree level, so the IR's depth follows the tree's
/// height, which this bound limits. The deepest compiler stack measured at
/// this bound is about 35 KB per level (a ladder of precedence levels, two
/// operators each, per parenthesis: 33.7 MB at 950 parentheses), about half
/// of the 64 MiB stack the compiler runs on; nested array literals take about
/// 18 KB per level. A chain may have about two million operators (V8 handles
/// chains of a million terms).
pub const MAX_NESTING_DEPTH: u32 = 1000;

/// Scriptw: tree-height units of one counted production.
pub const NESTING_UNITS_PER_LEVEL: u32 = 2048;

/// Scriptw: tree-height units of one `else if`, which the parser recurses
/// through directly rather than as a whole Statement: two thirds of a level,
/// so up to 1,500 `else if` branches compile (V8: about 3,000 run; 10,000
/// throw a RangeError). Native compilation of the nested branches still grows
/// faster than their number (2,000 took 2.2 GB with backtracking allocation).
pub const ELSE_IF_UNITS: u32 = NESTING_UNITS_PER_LEVEL * 2 / 3;

/// Scriptw: tree-height units of one link of a member, call, tagged template
/// or optional chain (`a.b`, `a[b]`, `a(b)`, ``a`b` ``, `a?.b` and each item
/// after it) that is not part of a method call: two levels, so such a chain
/// has at most about 500 links. Native compilation of a property access chain
/// grows quadratically with its length (450 `.a` links took 0.5 GB, 1,000
/// about 2.2 GB), and a call of a call result superlinearly.
pub const MEMBER_LINK_UNITS: u32 = 2 * NESTING_UNITS_PER_LEVEL;

/// Scriptw: tree-height units of each of the two links of a method call
/// (`a.m(…)`, `a[k](…)` and their optional forms): the property access whose
/// next token is the call's `(`, and the call whose callee is a property
/// access. Half a level each, so a chain of about 1,000 method calls fits, four
/// times the other links (builder and `Map#set` chains in generated code reach
/// hundreds; Node runs 3,000). Each costs the compiler about 4 KB of stack.
/// With the Speed policy, the embedding default, and one compilation job
/// (`SCRIPTW_JOBS=1`; more jobs compile separate functions concurrently), the
/// longest admitted chain compiles in about 0.7 GB (950 `.m()` calls at a Script's top level:
/// 647 MB and 10 s; 950 `Map#set` calls: 680 MB and 51 s). Native
/// compilation memory follows the size of the whole function body, not the
/// chain: three chains of 300 calls in one body cost what one of 900 does
/// (about 510 MB), three of 900 took 1.9 GB, and 2,700 unchained `o.m();`
/// statements 1.5 GB, which no nesting weight bounds (see the compile memory
/// item of the inventory). With the Fast policy 1,000 calls took 261 MB and
/// 14 s, and 1,500 calls 1.25 GB.
pub const METHOD_LINK_UNITS: u32 = NESTING_UNITS_PER_LEVEL / 2;

/// Scriptw: tree-height units of one binary, logical, `??` or comma operator;
/// the compiler walks these chains without recursing per operator. In a
/// generator or async function body, from the first operand of a chain that
/// contains an `await` or `yield` on, the compiler's resumable lowering
/// recurses per operator, so there one costs [`RESUMABLE_BINARY_LINK_UNITS`].
pub const BINARY_LINK_UNITS: u32 = 1;

/// Scriptw: tree-height units of one binary operator after an `await` or
/// `yield` in its chain, in a generator or async function body: half a level,
/// so about 2,000 fit.
pub const RESUMABLE_BINARY_LINK_UNITS: u32 = NESTING_UNITS_PER_LEVEL / 2;

/// Scriptw: the greatest tree height, in units, a source may have.
pub const MAX_NESTING_UNITS: u32 = MAX_NESTING_DEPTH * NESTING_UNITS_PER_LEVEL;

/// Scriptw: the nesting levels, of [`MAX_NESTING_DEPTH`], that Scriptw's
/// module scaffolding adds above an ES module's body. The compiler parses
/// every module's body again inside that scaffolding (an async arrow function
/// in an array in an arrow function called at a Script's top level), so the
/// Module parse starts this deep, and source that parses as a Module never
/// exceeds the limit only in the reparse, where the error would point into
/// generated text. A module's body may nest eight levels less than a
/// Script's.
///
/// The reparse also rewrites three forms, which the Module parse charges as
/// rewritten: `export var` is reached through a Statement once `export` is
/// deleted (one level); an anonymous `export default function` (or generator,
/// async or async generator function) becomes `var <name> = function …`, a
/// Statement and an AssignmentExpression (two levels), and an anonymous
/// `export default class` becomes `let <name> = class …` (one level); and
/// `import(…)`, in a Script too, becomes a call of a generated function,
/// charged that call's link (`NestingCharge::MemberLink`).
pub const MODULE_BODY_ENCLOSING_LEVELS: u32 = 8;

/// Scriptw: the most formal parameters one function may have; more is a
/// SyntaxError. ECMA-262 sets no limit. Node 24's V8 accepts at most 65,525,
/// although its SyntaxError says 65,534 are allowed, so code that runs there
/// fits.
pub const MAX_FORMAL_PARAMETERS: usize = 65_535;

/// Scriptw: what one charge against [`MAX_NESTING_UNITS`] is for. The limit
/// error names the construct that exceeded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NestingCharge {
    /// A counted production ([`NESTING_UNITS_PER_LEVEL`]).
    Level,
    /// An `else if` ([`ELSE_IF_UNITS`]).
    ElseIf,
    /// A member, call, tagged template or optional chain link ([`MEMBER_LINK_UNITS`]).
    MemberLink,
    /// A link of a method call ([`METHOD_LINK_UNITS`]).
    MethodLink,
    /// A binary, logical, `??` or comma operator ([`BINARY_LINK_UNITS`]).
    Operator,
    /// Such an operator of a chain that has an `await` or `yield` operand
    /// ([`RESUMABLE_BINARY_LINK_UNITS`]); the operators before that operand
    /// are charged it too.
    ResumableOperator,
}

impl NestingCharge {
    /// The tree-height units of this charge.
    pub(crate) const fn units(self) -> u32 {
        match self {
            Self::Level => NESTING_UNITS_PER_LEVEL,
            Self::ElseIf => ELSE_IF_UNITS,
            Self::MemberLink => MEMBER_LINK_UNITS,
            Self::MethodLink => METHOD_LINK_UNITS,
            Self::Operator => BINARY_LINK_UNITS,
            Self::ResumableOperator => RESUMABLE_BINARY_LINK_UNITS,
        }
    }

    /// What the limit error says the charge was for.
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Level => "",
            Self::ElseIf => " at this `else if`, which counts as two thirds of a level",
            Self::MemberLink => {
                " at this link of a property access, call, tagged template or optional chain, \
                 which counts as two levels"
            }
            Self::MethodLink => {
                " at this method call, each link of which counts as half a level"
            }
            Self::Operator => " at this operator, which counts as 1/2048 of a level",
            Self::ResumableOperator => {
                " at this operator of a chain with an `await` or `yield` operand, each operator \
                 of which counts as half a level"
            }
        }
    }
}

type ScriptParseOutput = (boa_ast::Script, boa_ast::SourceText);
type ModuleParseOutput = (boa_ast::Module, boa_ast::SourceText);

fn contains_top_level_using_declaration(body: &StatementList) -> bool {
    body.statements().iter().any(|item| {
        matches!(
            item,
            StatementListItem::Declaration(decl)
                if matches!(
                    decl.as_ref(),
                    Declaration::Lexical(
                        LexicalDeclaration::Using(_) | LexicalDeclaration::AwaitUsing(_)
                    )
                )
        )
    })
}

/// Trait implemented by parsers.
///
/// This makes it possible to abstract over the underlying implementation of a parser.
trait TokenParser<R>: Sized
where
    R: ReadChar,
{
    /// Output type for the parser.
    type Output; // = Node; waiting for https://github.com/rust-lang/rust/issues/29661

    /// Parses the token stream using the current parser.
    ///
    /// This method needs to be provided by the implementor type.
    ///
    /// # Errors
    ///
    /// It will fail if the cursor is not placed at the beginning of the expected non-terminal.
    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output>;
}

/// Boolean representing if the parser should allow a `yield` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowYield(bool);

impl From<bool> for AllowYield {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Boolean representing if the parser should allow a `await` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowAwait(bool);

impl From<bool> for AllowAwait {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Boolean representing if the parser should allow a `in` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowIn(bool);

impl From<bool> for AllowIn {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Boolean representing if the parser should allow a `return` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowReturn(bool);

impl From<bool> for AllowReturn {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Boolean representing if the parser should allow a `default` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllowDefault(bool);

impl From<bool> for AllowDefault {
    fn from(allow: bool) -> Self {
        Self(allow)
    }
}

/// Parser for the ECMAScript language.
///
/// This parser implementation tries to be conformant to the most recent
/// [ECMAScript language specification], and it also implements some legacy features like
/// [labelled functions][label] or [duplicated block-level function definitions][block].
///
/// [spec]: https://tc39.es/ecma262/#sec-ecmascript-language-source-code
/// [label]: https://tc39.es/ecma262/#sec-labelled-function-declarations
/// [block]: https://tc39.es/ecma262/#sec-block-duplicates-allowed-static-semantics
#[derive(Debug)]
pub struct Parser<'a, R> {
    /// Path to the source being parsed.
    #[allow(unused)] // Good to have for future improvements.
    path: Option<&'a Path>,
    /// Cursor of the parser, pointing to the lexer and used to get tokens for the parser.
    cursor: Cursor<R>,
}

impl<'a, R: ReadChar> Parser<'a, R> {
    /// Create a new `Parser` with a `Source` as the input to parse.
    pub fn new(source: Source<'a, R>) -> Self {
        Self {
            path: source.path,
            cursor: Cursor::new(source.reader),
        }
    }

    /// Parse the full input as a [ECMAScript Script][spec] into the boa AST representation without source text.
    /// The resulting `Script` can be compiled into boa bytecode and executed in the boa vm.
    ///
    /// # Errors
    ///
    /// Will return `Err` on any parsing error, including invalid reads of the bytes being parsed.
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-Script
    pub fn parse_script(
        &mut self,
        scope: &Scope,
        interner: &mut Interner,
    ) -> ParseResult<boa_ast::Script> {
        self.parse_script_with_source(scope, interner).map(|x| x.0)
    }

    /// Parse the full input as a [ECMAScript Script][spec] into the boa AST representation with source text.
    /// The resulting `Script` can be compiled into boa bytecode and executed in the boa vm.
    ///
    /// # Errors
    ///
    /// Will return `Err` on any parsing error, including invalid reads of the bytes being parsed.
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-Script
    pub fn parse_script_with_source(
        &mut self,
        scope: &Scope,
        interner: &mut Interner,
    ) -> ParseResult<ScriptParseOutput> {
        self.cursor.set_goal(InputElement::HashbangOrRegExp);
        let (mut ast, source) = ScriptParser::new(false).parse(&mut self.cursor, interner)?;
        if let Err(reason) = ast.analyze_scope(scope, interner) {
            return Err(Error::general(
                format!("invalid scope analysis: {reason}"),
                Position::new(1, 1),
            ));
        }
        Ok((ast, source))
    }

    /// Parse the full input as an [ECMAScript Module][spec] into the boa AST representation without source text.
    /// The resulting `ModuleItemList` can be compiled into boa bytecode and executed in the boa vm.
    ///
    /// # Errors
    ///
    /// Will return `Err` on any parsing error, including invalid reads of the bytes being parsed.
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-Module
    pub fn parse_module(
        &mut self,
        scope: &Scope,
        interner: &mut Interner,
    ) -> ParseResult<boa_ast::Module>
    where
        R: ReadChar,
    {
        self.parse_module_with_source(scope, interner).map(|x| x.0)
    }

    /// Parse the full input as an [ECMAScript Module][spec] into the boa AST representation with source text.
    /// The resulting `ModuleItemList` can be compiled into boa bytecode and executed in the boa vm.
    ///
    /// # Errors
    ///
    /// Will return `Err` on any parsing error, including invalid reads of the bytes being parsed.
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-Module
    pub fn parse_module_with_source(
        &mut self,
        scope: &Scope,
        interner: &mut Interner,
    ) -> ParseResult<ModuleParseOutput>
    where
        R: ReadChar,
    {
        self.cursor.set_goal(InputElement::HashbangOrRegExp);
        let (mut module, source) = ModuleParser.parse(&mut self.cursor, interner)?;
        if let Err(reason) = module.analyze_scope(scope, interner) {
            return Err(Error::general(
                format!("invalid scope analysis: {reason}"),
                Position::new(1, 1),
            ));
        }
        Ok((module, source))
    }

    /// [`19.2.1.1 PerformEval ( x, strictCaller, direct )`][spec]
    ///
    /// Parses the source text input of an `eval` call.
    ///
    /// # Errors
    ///
    /// Will return `Err` on any parsing error, including invalid reads of the bytes being parsed.
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-performeval
    pub fn parse_eval(
        &mut self,
        direct: bool,
        interner: &mut Interner,
    ) -> ParseResult<ScriptParseOutput> {
        self.cursor.set_goal(InputElement::HashbangOrRegExp);
        ScriptParser::new(direct).parse(&mut self.cursor, interner)
    }

    /// Parses the full input as an [ECMAScript `FunctionBody`][spec] into the boa AST representation.
    ///
    /// # Errors
    ///
    /// Will return `Err` on any parsing error, including invalid reads of the bytes being parsed.
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-FunctionBody
    pub fn parse_function_body(
        &mut self,
        interner: &mut Interner,
        allow_yield: bool,
        allow_await: bool,
    ) -> ParseResult<FunctionBody> {
        let mut parser = FunctionStatementList::new(allow_yield, allow_await, "function body");
        parser.parse_full_input(true);
        parser.parse(&mut self.cursor, interner)
    }

    /// Parses the full input as an [ECMAScript `FormalParameterList`][spec] into the boa AST representation.
    ///
    /// # Errors
    ///
    /// Will return `Err` on any parsing error, including invalid reads of the bytes being parsed.
    ///
    /// [spec]: https://tc39.es/ecma262/#prod-FormalParameterList
    pub fn parse_formal_parameters(
        &mut self,
        interner: &mut Interner,
        allow_yield: bool,
        allow_await: bool,
    ) -> ParseResult<FormalParameterList> {
        let params =
            FormalParameters::new(allow_yield, allow_await).parse(&mut self.cursor, interner)?;

        if let Some(next) = self.cursor.peek(0, interner)? {
            return Err(Error::unexpected(
                next.to_string(interner),
                next.span(),
                "formal parameters",
            ));
        }

        Ok(params)
    }
}

impl<R> Parser<'_, R> {
    /// Set the parser strict mode to true.
    pub fn set_strict(&mut self)
    where
        R: ReadChar,
    {
        self.cursor.set_strict(true);
    }

    /// Set the parser JSON mode to true.
    pub fn set_json_parse(&mut self)
    where
        R: ReadChar,
    {
        self.cursor.set_json_parse(true);
    }

    /// Set the unique identifier for the parser.
    pub fn set_identifier(&mut self, identifier: u32)
    where
        R: ReadChar,
    {
        self.cursor.set_identifier(identifier);
    }
}

/// Parses a full script.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-Script
#[derive(Debug, Clone, Copy)]
pub struct ScriptParser {
    direct_eval: bool,
}

impl ScriptParser {
    /// Create a new `Script` parser.
    #[inline]
    const fn new(direct_eval: bool) -> Self {
        Self { direct_eval }
    }
}

impl<R> TokenParser<R> for ScriptParser
where
    R: ReadChar,
{
    type Output = ScriptParseOutput;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let stmts =
            ScriptBody::new(true, cursor.strict(), self.direct_eval).parse(cursor, interner)?;
        let script = boa_ast::Script::new(stmts);

        // It is a Syntax Error if the LexicallyDeclaredNames of ScriptBody contains any duplicate entries.
        let mut lexical_names = FxHashSet::default();
        for name in lexically_declared_names(&script) {
            if !lexical_names.insert(name) {
                return Err(Error::general(
                    "lexical name declared multiple times",
                    Position::new(1, 1),
                ));
            }
        }

        // It is a Syntax Error if any element of the LexicallyDeclaredNames of ScriptBody also occurs in the VarDeclaredNames of ScriptBody.
        for name in var_declared_names(&script) {
            if lexical_names.contains(&name) {
                return Err(Error::general(
                    "lexical name declared multiple times",
                    Position::new(1, 1),
                ));
            }
        }

        let source = cursor.take_source();
        Ok((script, source))
    }
}

/// Parses a script body.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-ScriptBody
#[derive(Debug, Clone, Copy)]
pub struct ScriptBody {
    directive_prologues: bool,
    strict: bool,
    direct_eval: bool,
}

impl ScriptBody {
    /// Create a new `ScriptBody` parser.
    #[inline]
    const fn new(directive_prologues: bool, strict: bool, direct_eval: bool) -> Self {
        Self {
            directive_prologues,
            strict,
            direct_eval,
        }
    }
}

impl<R> TokenParser<R> for ScriptBody
where
    R: ReadChar,
{
    type Output = StatementList;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        let (body, _end) = statement::StatementList::new(
            false,
            false,
            false,
            &[],
            self.directive_prologues,
            self.strict,
        )
        .parse(cursor, interner)?;

        if contains_top_level_using_declaration(&body) {
            return Err(Error::general(
                if self.direct_eval {
                    "`using` declarations are not allowed at the top level of direct eval"
                } else {
                    "`using` declarations are not allowed at the top level of scripts"
                },
                Position::new(1, 1),
            ));
        }

        if !self.direct_eval {
            // It is a Syntax Error if StatementList Contains super unless the source text containing super is eval
            // code that is being processed by a direct eval.
            // Additional early error rules for super within direct eval are defined in 19.2.1.1.
            if contains(&body, ContainsSymbol::Super) {
                return Err(Error::general("invalid super usage", Position::new(1, 1)));
            }
            // It is a Syntax Error if StatementList Contains NewTarget unless the source text containing NewTarget
            // is eval code that is being processed by a direct eval.
            // Additional early error rules for NewTarget in direct eval are defined in 19.2.1.1.
            if contains(&body, ContainsSymbol::NewTarget) {
                return Err(Error::general(
                    "invalid new.target usage",
                    Position::new(1, 1),
                ));
            }

            // It is a Syntax Error if AllPrivateIdentifiersValid of StatementList with
            // argument « » is false unless the source text containing ScriptBody is
            // eval code that is being processed by a direct eval.
            if !all_private_identifiers_valid(&body, Vec::new()) {
                return Err(Error::general(
                    "invalid private identifier usage",
                    Position::new(1, 1),
                ));
            }
        }

        if let Err(error) = check_labels(&body) {
            return Err(Error::lex(LexError::Syntax(
                error.message(interner).into(),
                Position::new(1, 1),
            )));
        }

        if contains_invalid_object_literal(&body) {
            return Err(Error::lex(LexError::Syntax(
                "invalid object literal in script statement list".into(),
                Position::new(1, 1),
            )));
        }

        Ok(body)
    }
}

/// Parses a full module.
///
/// More information:
///  - [ECMAScript specification][spec]
///
/// [spec]: https://tc39.es/ecma262/#prod-Module
#[derive(Debug, Clone, Copy)]
struct ModuleParser;

impl<R> TokenParser<R> for ModuleParser
where
    R: ReadChar,
{
    type Output = ModuleParseOutput;

    fn parse(self, cursor: &mut Cursor<R>, interner: &mut Interner) -> ParseResult<Self::Output> {
        cursor.set_module();

        let module = boa_ast::Module::new(cursor.module_body(interner, |cursor, interner| {
            ModuleItemList.parse(cursor, interner)
        })?);

        // It is a Syntax Error if the LexicallyDeclaredNames of ModuleItemList contains any duplicate entries.
        let mut bindings = FxHashSet::default();
        for name in lexically_declared_names(&module) {
            if !bindings.insert(name) {
                return Err(Error::general(
                    format!(
                        "lexical name `{}` declared multiple times",
                        interner.resolve_expect(name)
                    ),
                    Position::new(1, 1),
                ));
            }
        }

        // It is a Syntax Error if any element of the LexicallyDeclaredNames of ModuleItemList also occurs in the
        // VarDeclaredNames of ModuleItemList.
        for name in var_declared_names(&module) {
            if !bindings.insert(name) {
                return Err(Error::general(
                    format!(
                        "lexical name `{}` declared multiple times",
                        interner.resolve_expect(name)
                    ),
                    Position::new(1, 1),
                ));
            }
        }

        // It is a Syntax Error if the ExportedNames of ModuleItemList contains any duplicate entries.
        {
            let mut exported_names = FxHashSet::default();
            for name in module.items().exported_names() {
                if !exported_names.insert(name) {
                    return Err(Error::general(
                        format!(
                            "exported name `{}` declared multiple times",
                            interner.resolve_expect(name)
                        ),
                        Position::new(1, 1),
                    ));
                }
            }
        }

        // It is a Syntax Error if any element of the ExportedBindings of ModuleItemList does not also occur in either
        // the VarDeclaredNames of ModuleItemList, or the LexicallyDeclaredNames of ModuleItemList.
        for name in module.items().exported_bindings() {
            if !bindings.contains(&name) {
                return Err(Error::general(
                    format!(
                        "could not find the exported binding `{}` in the declared names of the module",
                        interner.resolve_expect(name)
                    ),
                    Position::new(1, 1),
                ));
            }
        }

        // It is a Syntax Error if ModuleItemList Contains super.
        if contains(&module, ContainsSymbol::Super) {
            return Err(Error::general(
                "module cannot contain `super` on the top-level",
                Position::new(1, 1),
            ));
        }

        // It is a Syntax Error if ModuleItemList Contains NewTarget.
        if contains(&module, ContainsSymbol::NewTarget) {
            return Err(Error::general(
                "module cannot contain `new.target` on the top-level",
                Position::new(1, 1),
            ));
        }

        // It is a Syntax Error if ContainsDuplicateLabels of ModuleItemList with argument « » is true.
        // It is a Syntax Error if ContainsUndefinedBreakTarget of ModuleItemList with argument « » is true.
        // It is a Syntax Error if ContainsUndefinedContinueTarget of ModuleItemList with arguments « » and « » is true.
        check_labels(&module).map_err(|error| {
            Error::lex(LexError::Syntax(
                error.message(interner).into(),
                Position::new(1, 1),
            ))
        })?;

        // It is a Syntax Error if AllPrivateIdentifiersValid of ModuleItemList with argument « » is false.
        if !all_private_identifiers_valid(&module, Vec::new()) {
            return Err(Error::general(
                "invalid private identifier usage",
                Position::new(1, 1),
            ));
        }

        let source = cursor.take_source();
        Ok((module, source))
    }
}

/// Helper to check if any parameter names are declared in the given list.
fn name_in_lexically_declared_names(
    bound_names: &[Sym],
    lexical_names: &[Sym],
    position: Position,
    interner: &Interner,
) -> ParseResult<()> {
    for name in bound_names {
        if lexical_names.contains(name) {
            return Err(Error::general(
                format!(
                    "formal parameter `{}` declared in lexically declared names",
                    interner.resolve_expect(*name)
                ),
                position,
            ));
        }
    }
    Ok(())
}

/// Trait to reduce boilerplate in the parser.
trait OrAbrupt<T> {
    /// Will convert an `Ok(None)` to an [`Error::AbruptEnd`] or return the inner type if not.
    fn or_abrupt(self) -> ParseResult<T>;
}

impl<T> OrAbrupt<T> for ParseResult<Option<T>> {
    fn or_abrupt(self) -> ParseResult<T> {
        self?.ok_or(Error::AbruptEnd)
    }
}
