use scriptw_engine::{
    CompileOptions, Engine, ExecutionBackend, HostOutputEvent, HostSurfacePolicy,
    ObservedCompletion, ObservedJsValue, ObservedNumber, RealmBuilder, RunOptions,
};

fn assert_modes(source: &str, expected: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one compilation worker");
    for directive in ["", "'use strict';\n"] {
        let observed = Engine::new(RealmBuilder::new().build())
            .observe_script(
                &format!("{directive}{source}"),
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
                RunOptions {
                    backend: ExecutionBackend::WasmAot,
                    timeout_ms: Some(30_000),
                    ..RunOptions::default()
                },
            )
            .expect("primitive property source compiles and executes through Wasm AOT");
        assert_eq!(observed.backend_used, ExecutionBackend::WasmAot);
        assert_eq!(
            observed.completion,
            ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(262.0))),
            "{observed:?}"
        );
        assert_eq!(
            observed.output_events,
            vec![HostOutputEvent::PrintLine(expected.into())],
        );
    }
}

#[test]
fn literal_getv_keys_preserve_receiver_abrupt_and_argument_order() {
    assert_modes(
        r#"
        function check(value, label) { if (!value) throw label; }
        let log = '', gets = 0;
        const receiver = {};
        Object.defineProperty(receiver, 'opaque', {
            get() {
                check(this === receiver, 'object-getter-this');
                gets++;
                log += 'get;';
                return function (value) {
                    check(this === receiver, 'method-this');
                    log += 'call;';
                    return value;
                };
            }
        });
        function argument() { log += 'arg;'; return 17; }
        check(receiver.opaque(argument()) === 17, 'method-result');
        check(gets === 1 && log === 'get;arg;call;', 'get-before-argument-once');

        Object.defineProperty(Number.prototype, 'opaque', {
            configurable: true,
            get() { 'use strict'; check(this === 7, 'primitive-getter-this'); return 19; }
        });
        check((7).opaque === 19, 'primitive-lookup');
        check('abc'.length === 3 && ''.length === 0, 'string-lookup');

        let argumentsRun = 0, failures = 0;
        function forbidden() { argumentsRun++; return 1; }
        for (const value of [null, undefined]) {
            try { value.opaque(forbidden()); }
            catch (error) { check(error instanceof TypeError, 'nullish-error'); failures++; }
        }
        const marker = {};
        Object.defineProperty(receiver, 'fail', { get() { throw marker; } });
        try { receiver.fail(forbidden()); }
        catch (error) { check(error === marker, 'getter-error'); failures++; }
        check(failures === 3 && argumentsRun === 0, 'abrupt-prevents-arguments');

        let keyGets = 0;
        const unusual = {};
        Object.defineProperty(unusual, '\u0000\uD800', {
            get() { check(this === unusual, 'code-unit-getter-this'); keyGets++; return 23; }
        });
        unusual['\u0000\uDC00'] = 29;
        check(unusual['\u0000\uD800'] === 23 && unusual['\u0000\uDC00'] === 29,
              'exact-code-unit-keys');
        check(keyGets === 1, 'code-unit-getter-once');

        const symbol = Symbol('key');
        let conversions = 0;
        const key = {
            [Symbol.toPrimitive](hint) {
                check(hint === 'string', 'key-hint');
                conversions++;
                return symbol;
            }
        };
        Object.defineProperty(unusual, symbol, {
            get() { check(this === unusual, 'symbol-getter-this'); return 31; }
        });
        check(unusual[key] === 31 && conversions === 1, 'dynamic-symbol-key-once');
        print('literal-getv:ok');
        262;
        "#,
        "literal-getv:ok",
    );
}

#[test]
fn bare_primitive_gets_observe_prototype_getter_effects() {
    assert_modes(
        r#"
        const symbol = Symbol('original');
        let state;
        Object.defineProperty(Number.prototype, 'toFixed', {
            configurable: true,
            get() { state.value = 'number'; return 11; }
        });
        state = { value: 1 };
        const numberRead = (1).toFixed;
        if (numberRead !== 11 || typeof state.value !== 'string') throw 'number-get';

        Object.defineProperty(Boolean.prototype, 'valueOf', {
            configurable: true,
            get() { state.value = 'boolean'; return 12; }
        });
        state = { value: 1 };
        const booleanRead = true.valueOf;
        if (booleanRead !== 12 || typeof state.value !== 'string') throw 'boolean-get';

        Object.defineProperty(BigInt.prototype, 'toString', {
            configurable: true,
            get() { state.value = 'bigint'; return 13; }
        });
        state = { value: 1 };
        const bigintRead = (1n).toString;
        if (bigintRead !== 13 || typeof state.value !== 'string') throw 'bigint-get';

        Object.defineProperty(String.prototype, 'charAt', {
            configurable: true,
            get() { state.value = 'string'; return 14; }
        });
        state = { value: 1 };
        const stringRead = 'abc'.charAt;
        if (stringRead !== 14 || typeof state.value !== 'string') throw 'string-get';

        Object.defineProperty(Symbol.prototype, 'valueOf', {
            configurable: true,
            get() { state.value = 'symbol'; return 15; }
        });
        state = { value: 1 };
        const symbolRead = symbol.valueOf;
        if (symbolRead !== 15 || typeof state.value !== 'string') throw 'symbol-get';
        print('primitive-gets:ok');
        262;
        "#,
        "primitive-gets:ok",
    );
}

#[test]
fn fresh_instances_consult_live_builtin_prototypes_before_arguments() {
    assert_modes(
        r#"
        let log = '', state;
        Object.defineProperty(Date.prototype, 'getTime', {
            configurable: true,
            get() {
                log += 'get;'; state.value = 'date';
                return function (value) { if (this !== date) throw 'date-this'; return value; };
            }
        });
        const date = new Date(0);
        state = { value: 1 };
        const actual = date.getTime((log += 'arg;', typeof state.value));
        if (actual !== 'string' || log !== 'get;arg;') throw 'date-order';
        Map.prototype.has = function () { return 'map'; };
        Set.prototype.has = function () { return 'set'; };
        if (new Map().has(1) !== 'map' || new Set().has(1) !== 'set') throw 'collection-result';
        Temporal.PlainDate.prototype.add = function () { return 'date-added'; };
        if (new Temporal.PlainDate(2024, 1, 1).add({ days: 1 }) !== 'date-added') throw 'temporal-result';
        Object.defineProperty(Intl.Locale.prototype, 'baseName', {
            configurable: true, get() { return 42; }
        });
        if (new Intl.Locale('en').baseName !== 42) throw 'intl-result';
        Object.defineProperty(Uint8Array.prototype, 'constructor', {
            configurable: true, get() { return 43; }
        });
        if (new Uint8Array(1).constructor !== 43) throw 'typed-array-constructor';
        print('fresh-prototypes:ok');
        262;
        "#,
        "fresh-prototypes:ok",
    );
}

#[test]
fn array_reads_observe_holes_indices_and_replaced_named_methods() {
    assert_modes(
        r#"
        let state, log = '';
        Object.defineProperty(Array.prototype, '7', {
            configurable: true, get() { state.value = 'index'; return 'inherited'; }
        });
        state = { value: 1 };
        const missing = [0][7];
        if (missing !== 'inherited' || typeof state.value !== 'string') throw 'absent-index';
        state = { value: 1 };
        const hole = [,,,,,,,,][7];
        if (hole !== 'inherited' || typeof state.value !== 'string') throw 'hole-index';
        function at(array, index) { return array[index]; }
        state = { value: 1 };
        if (at([1], 7) !== 'inherited' || typeof state.value !== 'string') throw 'dynamic-index';
        Object.defineProperty(Array.prototype, 'join', {
            configurable: true,
            get() { log += 'get;'; return function () { return 17; }; }
        });
        const joined = [1].join((log += 'arg;', ','));
        if (joined !== 17 || log !== 'get;arg;') throw 'array-method';
        print('array-property-gets:ok');
        262;
        "#,
        "array-property-gets:ok",
    );
}

#[test]
fn property_reads_keep_the_receiver_and_actual_global_object() {
    assert_modes(
        r#"
        let Math;
        if (!delete globalThis.Math || Math !== undefined || Object.hasOwn(globalThis, 'Math')) {
            throw 'global-property-delete-versus-lexical';
        }
        const deleteLexicalMath = Function('return delete Math;');
        if (deleteLexicalMath()) throw 'global-lexical-delete';

        let calls = 0;
        function make() { calls++; return function () {}; }
        const inheritedCall = make().call;
        if (calls !== 1 || inheritedCall !== Function.prototype.call) throw 'receiver-effects';
        function shadow(globalThis) { return globalThis.NaN; }
        if (shadow({ NaN: 'local' }) !== 'local') throw 'globalThis-shadow';
        function capture(globalThis) { return () => globalThis.NaN; }
        if (capture({ NaN: 'captured' })() !== 'captured') throw 'globalThis-capture';
        function numericNames(Infinity, NaN, undefined, print) {
            return Infinity + ':' + NaN + ':' + undefined + ':' + print;
        }
        if (numericNames('I', 'N', 'U', 'P') !== 'I:N:U:P') throw 'shadowed-global-names';
        globalThis.prototypeReadGlobal = 1;
        __scriptwRealmEvalScript('let prototypeReadGlobal = 2;');
        if (globalThis.prototypeReadGlobal !== 1 || prototypeReadGlobal !== 2) throw 'global-object';
        const chars = [...new String('a\u{1D11E}')];
        if (chars.length !== 2 || chars[1] !== '\u{1D11E}') throw 'boxed-string-iterator';

        const entryGlobal = globalThis;
        const originalGlobalThis = Object.getOwnPropertyDescriptor(entryGlobal, 'globalThis');
        const replacement = { NaN: 'replacement', victim: 'replacement', receiverCounter: 99 };
        function readGlobalThis() { return globalThis; }
        function readGlobalType() { return typeof globalThis; }
        var receiverCounter = 1;
        function updateRealGlobal(globalThis) {
            receiverCounter += 1;
            receiverCounter++;
            return globalThis.NaN;
        }
        const deleteGlobalThis = Function('return delete globalThis;');
        const deleteHasShadow = Function('return delete referenceDeleteShadow;');
        function readAbruptType() { return typeof referenceTypeofAbrupt; }
        var parseInt;
        function readDeclaredType() { return typeof parseInt; }
        const originalParseInt = Object.getOwnPropertyDescriptor(entryGlobal, 'parseInt');
        const originalPrototype = Object.getPrototypeOf(entryGlobal);
        let getterReads = 0, getterState;
        try {
            globalThis = Function;
            if (globalThis('return 41')() !== 41) throw 'callable-public-globalThis';
            entryGlobal.globalThis = entryGlobal;
            const hook = Object.create({ get replace() { entryGlobal.globalThis = replacement; } });
            hook.replace;
            if (readGlobalThis() !== replacement || globalThis !== replacement) throw 'replaced-globalThis';
            entryGlobal.victim = 'entry';
            Object.defineProperty(entryGlobal, 'globalThis', {
                configurable: true,
                get() { getterReads++; getterState.value = 'changed'; return replacement; }
            });
            getterState = { value: 1 };
            const selectedGlobal = globalThis;
            if (selectedGlobal !== replacement || typeof getterState.value !== 'string') throw 'globalThis-getter-effects';
            if (readGlobalThis() !== replacement || globalThis.NaN !== 'replacement') throw 'globalThis-getter';
            if (typeof globalThis !== 'object' || readGlobalType() !== 'object' || getterReads !== 5) throw 'globalThis-typeof-getter';
            if (!delete globalThis.victim || getterReads !== 6 ||
                Object.hasOwn(replacement, 'victim') || entryGlobal.victim !== 'entry') throw 'globalThis-delete-receiver';
            if (updateRealGlobal({ NaN: 'parameter' }) !== 'parameter' ||
                entryGlobal.receiverCounter !== 3 || replacement.receiverCounter !== 99 ||
                getterReads !== 6) throw 'hidden-global-reference';
            delete entryGlobal.globalThis;
            let missing;
            try { readGlobalThis(); } catch (error) { missing = error; }
            if (!(missing instanceof ReferenceError) || typeof globalThis !== 'undefined' ||
                readGlobalType() !== 'undefined') throw 'deleted-globalThis';
            entryGlobal.globalThis = replacement;
            if (!deleteGlobalThis() || typeof globalThis !== 'undefined') throw 'delete-globalThis-binding';

            delete entryGlobal.parseInt;
            let globalHas = 0, globalGets = 0, declaredHas = 0, declaredGets = 0;
            let shadowHas = 0, shadowGets = 0;
            const marker = {};
            const prototype = new Proxy(Object.create(null), {
                has(target, key) {
                    if (key === 'globalThis') { globalHas++; return false; }
                    if (key === 'parseInt') { declaredHas++; return false; }
                    if (key === 'referenceTypeofAbrupt') throw marker;
                    if (key === 'referenceDeleteShadow') {
                        shadowHas++;
                        __scriptwRealmEvalScript('let referenceDeleteShadow = 23;');
                        return true;
                    }
                    return Reflect.has(target, key);
                },
                get(target, key, receiver) {
                    if (key === 'globalThis') { globalGets++; return replacement; }
                    if (key === 'parseInt') { declaredGets++; return 17; }
                    if (key === 'referenceDeleteShadow') shadowGets++;
                    return Reflect.get(target, key, receiver);
                }
            });
            Object.setPrototypeOf(entryGlobal, prototype);
            if (typeof globalThis !== 'undefined' || readGlobalType() !== 'undefined' ||
                globalHas !== 2 || globalGets !== 0) throw 'typeof-global-has-before-get';
            if (!deleteGlobalThis() || globalHas !== 3 || globalGets !== 0) throw 'delete-global-has';
            if (readDeclaredType() !== 'undefined' || declaredHas !== 1 || declaredGets !== 0) {
                throw 'typeof-declared-global-has';
            }
            let abrupt;
            try { readAbruptType(); } catch (error) { abrupt = error; }
            if (abrupt !== marker) throw 'typeof-global-has-abrupt';
            if (deleteHasShadow() || shadowHas !== 1 || shadowGets !== 0 ||
                __scriptwRealmEvalScript('referenceDeleteShadow;') !== 23) throw 'delete-global-lexical-refresh';
        } finally {
            Object.setPrototypeOf(entryGlobal, originalPrototype);
            Object.defineProperty(entryGlobal, 'globalThis', originalGlobalThis);
            Object.defineProperty(entryGlobal, 'parseInt', originalParseInt);
            delete entryGlobal.victim;
        }
        print('property-receivers:ok');
        262;
        "#,
        "property-receivers:ok",
    );
}

#[test]
fn foreign_global_constructors_do_not_change_primitive_boxing_realm() {
    assert_modes(
        r#"
        const other = __scriptwCreateRealm().global;
        Number.prototype.toString = function () { return 17; };
        Boolean.prototype.toString = function () { return 18; };
        Number = other.Number;
        Boolean = other.Boolean;
        if ((1).toString() !== 17 || true.toString() !== 18) throw 'local-boxing-realm';
        print('primitive-boxing-realm:ok');
        262;
        "#,
        "primitive-boxing-realm:ok",
    );
}

#[test]
fn symbol_properties_and_bigint_custom_calls_use_live_prototypes() {
    assert_modes(
        r#"
        const bare = Symbol(), described = Symbol('text'), known = Symbol.iterator;
        const primitiveKey = Symbol.toPrimitive;
        if (bare.description !== undefined || described.description !== 'text') throw 'description';
        if (known.description !== 'Symbol.iterator') throw 'known-description';
        Object.defineProperty(Symbol.prototype, 'description', {
            configurable: true, get() { 'use strict'; return this === bare ? 41 : 42; }
        });
        if (bare.description !== 41 || described.description !== 42) throw 'description-getter';
        Symbol.prototype.constructor = 17;
        if (bare.constructor !== 17) throw 'constructor';
        Object.defineProperty(Symbol.prototype, primitiveKey, {
            configurable: true, value: function (hint) { 'use strict'; return this === bare && hint; }
        });
        if (bare[primitiveKey]('changed') !== 'changed') throw 'symbol-method';
        BigInt.prototype.custom = function (value) { 'use strict'; return this + value; };
        if ((7n).custom(2n) !== 9n) throw 'bigint-own-method';
        Object.prototype.inherited = function () { 'use strict'; return this; };
        if ((8n).inherited() !== 8n) throw 'bigint-inherited-method';
        print('primitive-prototypes:ok');
        262;
        "#,
        "primitive-prototypes:ok",
    );
}

#[test]
fn primitive_getters_precede_arguments_and_preserve_abrupt_values() {
    assert_modes(
        r#"
        let state, log = '';
        const marker = {};
        Object.defineProperty(Number.prototype, 'toFixed', {
            configurable: true,
            get() {
                log += 'get;'; state.value = 'changed';
                return function (argument) { 'use strict';
                    log += 'call;';
                    if (this !== 1 || argument !== 'string') throw 'receiver-or-argument';
                    return 31;
                };
            }
        });
        state = { value: 1 };
        if ((1).toFixed((log += 'argument;', typeof state.value)) !== 31) throw 'call';
        if (log !== 'get;argument;call;') throw 'order';
        Object.defineProperty(String.prototype, 'custom', {
            configurable: true, get() { throw marker; }
        });
        let reached = false;
        try { 'abc'.custom(reached = true); throw 'missing-throw'; }
        catch (error) { if (error !== marker || reached) throw 'abrupt'; }
        Object.defineProperty(String.prototype, '9', {
            configurable: true, get() { 'use strict'; state.value = this; return 32; }
        });
        state = { value: 1 };
        const inheritedIndex = 'a'[9];
        if (inheritedIndex !== 32 || typeof state.value !== 'string' || state.value !== 'a') {
            throw 'out-of-range-index';
        }
        Object.defineProperty(String.prototype, '0', { configurable: true, get() { throw 'own-index'; } });
        if ('a'[0] !== 'a') throw 'string-own-index';
        Object.defineProperty(String.prototype, Symbol.iterator, {
            configurable: true, get() {
                state.value = 'iterator';
                return function (argument) { 'use strict';
                    if (this !== 'x') throw 'iterator-receiver';
                    return argument;
                };
            }
        });
        state = { value: 1 };
        if ('x'[Symbol.iterator](typeof state.value) !== 'string') throw 'iterator-getter';
        print('primitive-order:ok');
        262;
        "#,
        "primitive-order:ok",
    );
}

#[test]
fn array_and_regexp_protocol_getters_precede_arguments() {
    assert_modes(
        r#"
        let state;
        Object.defineProperty(Array.prototype, Symbol.iterator, {
            configurable: true, get() {
                state.value = 'array';
                return function (argument) { return argument; };
            }
        });
        state = { value: 1 };
        if ([1][Symbol.iterator](typeof state.value) !== 'string') throw 'array-iterator-get';

        Object.defineProperty(RegExp.prototype, Symbol.match, {
            configurable: true, get() {
                state.value = 'match';
                return function (argument) { return argument; };
            }
        });
        state = { value: 1 };
        if (/x/[Symbol.match](typeof state.value) !== 'string') throw 'regexp-match-get';

        Object.defineProperty(RegExp.prototype, 'test', {
            configurable: true, get() {
                state.value = 'test';
                return function (argument) { return argument; };
            }
        });
        state = { value: 1 };
        if (/x/.test(typeof state.value) !== 'string') throw 'regexp-test-get';

        const own = { [Symbol.match]() { return 47; } };
        if (own[Symbol.match]('x') !== 47) throw 'own-protocol';
        const absent = Object.create(null);
        let argumentRan = false;
        try { absent[Symbol.match](argumentRan = true); throw 'missing-type-error'; }
        catch (error) {
            if (!(error instanceof TypeError) || !argumentRan) throw 'absent-protocol';
        }
        print('protocol-gets:ok');
        262;
        "#,
        "protocol-gets:ok",
    );
}

#[test]
fn source_helper_names_do_not_replace_property_or_argument_execution() {
    assert_modes(
        r#"
        let log = '';
        const ASCII_IDENTIFIER = {
            get test() {
                log += 'get;';
                return function (value) {
                    log += 'call;';
                    if (this !== ASCII_IDENTIFIER || value !== 23) throw 'helper-receiver';
                    return false;
                };
            }
        };
        if (ASCII_IDENTIFIER.test((log += 'argument;', 23)) !== false) throw 'helper-name';
        if (log !== 'get;argument;call;') throw 'helper-effects';
        {
            const ASCII_IDENTIFIER = /^[$_a-zA-Z][$_a-zA-Z0-9]*$/u;
            if (ASCII_IDENTIFIER.test('1invalid') || !ASCII_IDENTIFIER.test('next')) throw 'regexp-result';
        }
        print('source-helper-names:ok');
        262;
        "#,
        "source-helper-names:ok",
    );
}
