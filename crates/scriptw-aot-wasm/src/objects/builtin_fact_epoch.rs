//! Where the builtin fact epoch advances.
//!
//! A builtin entry guard proves its facts from live state and keeps the
//! whole proof for one epoch (`functions/builtin_entry_guard.rs`). The proof
//! is read from the state below, so each writer of that state ends the
//! epoch; an entry in the same epoch then knows that nothing it read has
//! changed. Three of the states are compared at the entry itself: the
//! Global Environment Record, the current Realm, and the Environment's
//! declarative name table, which is replaced whenever a name joins it.
//!
//! - **A global binding's value and kind.** A fact reads the global
//!   object's dictionary entry for its name: the entry's descriptor, that
//!   descriptor's flags and its value cell. A value cell is immutable, an
//!   entry is its property's one record for as long as the property lives,
//!   and a stored descriptor belongs to one entry (every publication
//!   allocates the descriptor it stores), so the binding changes only when
//!   that descriptor is written or the entry's descriptor is replaced. A
//!   proof marks each entry it read WATCHED before it is kept, and every
//!   writer that reaches a stored descriptor through its entry calls
//!   [`FunctionBuilder::emit_watched_entry_edit`] for that entry:
//!   - the in-place Set of an existing writable data property
//!     (`emit_dictionary_existing_data_write`),
//!   - a redefinition copied onto the stored descriptor
//!     (`emit_publish_ordinary_definition`),
//!   - a publication that replaces an entry's descriptor
//!     (`emit_ordinary_append_property_entry_inner`),
//!   - a deletion, which replaces it with a dead accessor descriptor
//!     (`emit_object_delete_ordinary`).
//!
//!   The one writer that holds a stored descriptor without its entry is the
//!   resolver that replaces a deferred recipe by the value it made
//!   (`intrinsics/deferred_methods.rs`); it advances the epoch for every
//!   descriptor, at most once per recipe. The remaining writers of
//!   descriptor fields write descriptors no entry holds yet (a merged
//!   descriptor, an Array's synthesized `length`) or a function object's own
//!   `caller` and `arguments` (`functions/legacy_activation/property.rs`),
//!   whose entries are never a global object's.
//! - **An intrinsic object's rows.** A namespace, a constructor with a
//!   complete layout and the protected prototypes are proved by the
//!   intrinsic storage their headers hold. Rows are immutable and
//!   `emit_promote_intrinsic_properties` is the only writer of a header
//!   that holds them; it advances the epoch.
//! - **A Realm's global object and `globalThis`.** Written once, when the
//!   Realm's global object is made; that write advances the epoch.
//!
//! Everything else a fact reads is immutable: a function object's code and
//! context, a context's Realm, a site record's Environment and entry, an
//! Environment's defining Realm, and the protector record.
use super::*;
use crate::gc_types::GcFieldNullability;

impl FunctionBuilder<'_> {
    /// Ends the builtin fact epoch when some builtin entry proof was read
    /// from the descriptor the non-null `entry` holds. Called by each
    /// writer of that descriptor, and of the entry's descriptor field,
    /// before or after its write: no source code runs in between.
    pub(super) fn emit_watched_entry_edit<N: GcFieldNullability>(
        &mut self,
        entry: &GcLocal<PropertyEntry, N>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .field(PropertyEntrySchema::WATCHED)
            .read(entry, schema, function);
        self.open_frame(ControlFrameKind::If, function);
        // Only a write to a global binding some guard proved from.
        function.rarely_reached();
        schema.emit_advance_builtin_fact_epoch(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
}
