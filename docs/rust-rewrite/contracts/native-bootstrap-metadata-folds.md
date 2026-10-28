# Native bootstrap metadata and accessor folds

Native headers already carry their compact name and length before their
Context, FunctionObject and ACTIVE_FUNCTION cycle are completed. The later
metadata publication has no effects when the actual entry is a standard or
host builtin, the function is strict, no automatic prototype is required and
name/length remain configurable. The compiler omits only that empty tail.
The repeated name lookup would hit the same strongly rooted pooled String;
the other omitted operands are constants or null locals.

These conditions exclude JavaScript functions, non-strict natives, automatic
prototype creation and the nonconfigurable ThrowTypeError function. Their
existing publication stays intact. Header allocation, source/Context creation,
the active-function cycle and Realm ownership keep their existing order.

The fifteen Intl getters use the existing checked accessor allocator: twelve
Locale getters and the DateTimeFormat.format, NumberFormat.format and
Collator.compare getters. Its plan validates their actual native entry,
strictness, capture/class flags, source ordinal and typed Wasm code. The
callable and its metadata remain independent in each defining Realm.

This changes emitted code within runtime layout 30. The [verified checkpoint](../memory-bootstrap-metadata-folds-20261028.md)
records 4,906 fresh native tests across 19 full targets and all 17 matched
workload checksums. It introduces no helper ABI or object representation.
No overall causal memory or speed saving is established.
