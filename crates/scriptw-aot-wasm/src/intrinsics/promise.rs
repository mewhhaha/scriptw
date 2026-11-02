//! Complete Promise prototype and shared publication catalogues.

use super::super::*;

pub(crate) const PROMISE_PROTOTYPE_METHOD_PUBLICATIONS: [StandardBuiltinId; 3] = [
    StandardBuiltinId::PromisePrototypeThen,
    StandardBuiltinId::PromisePrototypeCatch,
    StandardBuiltinId::PromisePrototypeFinally,
];

pub(crate) const PROMISE_STATIC_METHOD_PUBLICATIONS: [StandardBuiltinId; 10] = [
    StandardBuiltinId::PromiseResolve,
    StandardBuiltinId::PromiseReject,
    StandardBuiltinId::PromiseAll,
    StandardBuiltinId::PromiseAllSettled,
    StandardBuiltinId::PromiseAllKeyed,
    StandardBuiltinId::PromiseAllSettledKeyed,
    StandardBuiltinId::PromiseAny,
    StandardBuiltinId::PromiseRace,
    StandardBuiltinId::PromiseWithResolvers,
    StandardBuiltinId::PromiseTry,
];
