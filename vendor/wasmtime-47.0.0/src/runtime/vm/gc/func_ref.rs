//! Implementation of the side table for `funcref`s in the GC heap.
//!
//! The actual `VMFuncRef`s are kept in a side table, rather than inside the GC
//! heap, for the same reasons that an `externref`'s host data is kept in a side
//! table. We cannot trust any data coming from the GC heap, but `VMFuncRef`s
//! contain raw pointers, so if we stored `VMFuncRef`s inside the GC heap, we
//! wouldn't be able to use the raw pointers from any `VMFuncRef` we got out of
//! the heap. And that means we wouldn't be able to, for example, call a
//! `funcref` we got from inside the GC heap.

use crate::{
    Result, bail_bug,
    hash_map::HashMap,
    type_registry::TypeRegistry,
    vm::{SendSyncPtr, VMFuncRef},
};
use wasmtime_core::alloc::{PanicOnOom, TryVec};
use core::hash::{BuildHasherDefault, Hasher};
use wasmtime_environ::VMSharedTypeIndex;

/// An identifier into the `FuncRefTable`: the index of its entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct FuncRefTableId(u32);

impl FuncRefTableId {
    /// Convert this `FuncRefTableId` into its raw `u32` ID.
    pub fn into_raw(self) -> u32 {
        self.0
    }

    /// Create a `FuncRefTableId` from a raw `u32` ID.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// Hasher for interned `VMFuncRef` addresses. They are host pointers, not
/// guest-chosen bytes, so the keyed SipHash default only cost each
/// `struct.new` or `struct.set` of a `funcref`; the murmur3 finalizer spreads
/// the aligned addresses over the table's bucket bits.
#[derive(Default)]
struct FuncRefAddressHasher(u64);

impl Hasher for FuncRefAddressHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(byte);
        }
    }

    fn write_u64(&mut self, word: u64) {
        self.0 = self.0.rotate_left(32) ^ word;
    }

    fn write_usize(&mut self, word: usize) {
        self.write_u64(word as u64);
    }

    fn write_isize(&mut self, word: isize) {
        self.write_u64(word as u64);
    }

    fn finish(&self) -> u64 {
        let mut hash = self.0;
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        hash ^ (hash >> 33)
    }
}

/// Side table mapping `FuncRefTableId`s that can be stored in the GC heap to
/// raw `VMFuncRef`s.
///
/// Entries are never removed, so ids are dense indices into `entries`. Each
/// entry is a nullable `VMFuncRef` pointer (`Option<SendSyncPtr<_>>` has the
/// layout of `*mut VMFuncRef`, null for `None`). A collector may publish
/// [`FuncRefTable::view`] to compiled code, which then reads an entry inline
/// and calls `get_interned_func_ref` for everything it does not accept.
#[derive(Default)]
pub struct FuncRefTable {
    interned: HashMap<
        Option<SendSyncPtr<VMFuncRef>>,
        FuncRefTableId,
        BuildHasherDefault<FuncRefAddressHasher>,
    >,
    entries: TryVec<Option<SendSyncPtr<VMFuncRef>>>,
}

/// The entries of a [`FuncRefTable`] as compiled code reads them: `len`
/// pointer-sized entries starting at `base`. Valid until the table next
/// grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuncRefTableView {
    /// Address of the first entry.
    pub base: usize,
    /// Number of entries.
    pub len: u32,
}

impl FuncRefTable {
    /// Intern a `VMFuncRef` in the side table, returning an ID that can be
    /// stored in the GC heap.
    ///
    /// # Safety
    ///
    /// The given `func_ref` must point to a valid `VMFuncRef` and must remain
    /// valid for the duration of this table's lifetime.
    pub unsafe fn intern(&mut self, func_ref: Option<SendSyncPtr<VMFuncRef>>) -> FuncRefTableId {
        *self.interned.entry(func_ref).or_insert_with(|| {
            let id = u32::try_from(self.entries.len()).expect("funcref table index space exhausted");
            // TODO(#12069): handle allocation failure here
            self.entries.push(func_ref).panic_on_oom();
            FuncRefTableId(id)
        })
    }

    /// The table's entries for compiled code. Every interned id is below `len`.
    pub fn view(&self) -> FuncRefTableView {
        FuncRefTableView {
            base: self.entries.as_ptr() as usize,
            // `intern` keeps the length within `u32`.
            len: self.entries.len() as u32,
        }
    }

    fn entry(&self, id: FuncRefTableId) -> Option<Option<SendSyncPtr<VMFuncRef>>> {
        self.entries.get(usize::try_from(id.0).ok()?).copied()
    }

    /// Get the `VMFuncRef` associated with the given ID.
    ///
    /// Checks that the `VMFuncRef` is a subtype of the expected type.
    pub fn get_typed(
        &self,
        types: &TypeRegistry,
        id: FuncRefTableId,
        expected_ty: VMSharedTypeIndex,
    ) -> Result<Option<SendSyncPtr<VMFuncRef>>> {
        let Some(f) = self.entry(id) else {
            bail_bug!("bad FuncRefTableId")
        };

        if let Some(f) = f {
            // The safety contract for `intern` ensures that deref'ing `f` is safe.
            let actual_ty = unsafe { f.as_ref().type_index };

            // Ensure that the funcref actually is a subtype of the expected
            // type. This protects against GC heap corruption being leveraged in
            // attacks: if the attacker has a write gadget inside the GC heap,
            // they can overwrite a funcref ID to point to a different funcref,
            // but this check ensures that any calls to that wrong funcref at
            // least remain well-typed, which reduces the attack surface and
            // maintains memory safety.
            if !types.is_subtype(actual_ty, expected_ty) {
                bail_bug!("funcref table type mismatch")
            }
        }

        Ok(f)
    }

    /// Get the `VMFuncRef` associated with the given ID, without checking the
    /// type.
    ///
    /// Prefer `get_typed`. This method is only suitable for getting a
    /// `VMFuncRef` as an untyped `funcref` function reference, and never as a
    /// typed `(ref $some_func_type)` function reference.
    pub fn get_untyped(&self, id: FuncRefTableId) -> Result<Option<SendSyncPtr<VMFuncRef>>> {
        match self.entry(id) {
            Some(f) => Ok(f),
            None => bail_bug!("bad FuncRefTableId"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_have_the_layout_compiled_code_reads() {
        assert_eq!(
            core::mem::size_of::<Option<SendSyncPtr<VMFuncRef>>>(),
            core::mem::size_of::<*mut VMFuncRef>(),
        );
        let mut table = FuncRefTable::default();
        let mut func_ref = core::mem::MaybeUninit::<VMFuncRef>::uninit();
        let ptr = SendSyncPtr::new(core::ptr::NonNull::new(func_ref.as_mut_ptr()).unwrap());
        // SAFETY: the entries are only read back as addresses below.
        let (null, some) = unsafe { (table.intern(None), table.intern(Some(ptr))) };
        assert_eq!((null.into_raw(), some.into_raw()), (0, 1));
        assert_eq!(unsafe { table.intern(Some(ptr)) }, some);
        let view = table.view();
        assert_eq!(view.len, 2);
        // SAFETY: `view` describes two initialized pointer-sized entries.
        let words = unsafe { core::slice::from_raw_parts(view.base as *const usize, 2) };
        assert_eq!(words, [0, ptr.as_ptr() as usize]);
    }
}
