# Reflect descriptor objects retain the executing Realm

Reflect.getOwnPropertyDescriptor invokes the typed `ObjectGetOwnProperty`
runtime helper with its actual native method Realm. The helper returns a whole
Completion and a private descriptor snapshot. The public wrapper alone creates
the fresh observable object, using that same Realm's Object prototype. Neither
public method replacement nor a caller's lexical Environment changes this Realm.
Proxy defineProperty descriptors retain their existing internal-method owner.

The helper has no callable context. Revocation and generated descriptor errors
use its explicit execution Realm; trap calls select their actual callable Realm
and preserve original Throw values. Recursive descriptor operations retain the
originating execution Realm across nested user calls.

Existing `reflect_descriptor_object_realm`, `aot_reflect_completed_descriptors`
and `aot_gc_reflect_entries` controls cover identity, admission, errors and both
borrowed-Realm directions. The encoded runtime/program signature and private
transport boundary are checked by `emission::own_property_helper`.
