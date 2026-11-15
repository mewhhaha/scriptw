use super::*;

#[test]
fn stale_slot_generations_and_foreign_stores_cannot_alias_new_entries() {
    let (mut store, pre) = heap();
    let owner = object(&mut store, &pre, 1);
    let a = object(&mut store, &pre, 10);
    let b = object(&mut store, &pre, 20);
    let old = WeakAnyRef::new(&mut store, &owner, &a).unwrap();
    old.remove(&mut store).unwrap();
    let current = WeakAnyRef::new(&mut store, &owner, &b).unwrap();
    old.remove(&mut store).unwrap();
    assert!(old.upgrade(&mut store).unwrap().is_none());
    let upgraded = current.upgrade(&mut store).unwrap().unwrap();
    assert!(Rooted::ref_eq(&store, &upgraded, &b).unwrap());
    let (mut foreign, _) = heap();
    assert!(current.upgrade(&mut foreign).is_err());
    assert!(WeakAnyRef::new(&mut foreign, &owner, &a).is_err());
    let immediate = AnyRef::from_i31(&mut store, I31::wrapping_u32(1));
    assert!(WeakAnyRef::new(&mut store, &owner, &immediate).is_err());
    assert!(GcEphemeron::new(&mut store, &immediate, &a, Some(&b)).is_err());
}

#[test]
fn job_held_roots_survive_until_the_keep_set_is_cleared() {
    let (mut store, pre) = heap();
    let owner = object(&mut store, &pre, 1);
    let (weak, mut kept) = {
        let mut roots = RootScope::new(&mut store);
        let target = object(&mut roots, &pre, 10);
        let weak = WeakAnyRef::new(&mut roots, &owner, &target).unwrap();
        let upgraded = weak.upgrade(&mut roots).unwrap().unwrap();
        (weak, vec![upgraded.to_owned_rooted(&mut roots).unwrap()])
    };
    for _ in 0..4 {
        store.gc(None).unwrap();
        let mut roots = RootScope::new(&mut store);
        assert!(weak.upgrade(&mut roots).unwrap().is_some());
    }
    kept.clear();
    store.gc(None).unwrap();
    assert!(weak.upgrade(&mut store).unwrap().is_none());
}

struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn ephemeron_marking_precedes_externref_host_data_sweeping() {
    let (mut store, pre) = heap();
    let owner = object(&mut store, &pre, 1);
    let drops = Arc::new(AtomicUsize::new(0));
    let (key, entry, weak) = {
        let mut roots = RootScope::new(&mut store);
        let key = object(&mut roots, &pre, 10);
        let native = ExternRef::new(&mut roots, Dropped(Arc::clone(&drops))).unwrap();
        let value = AnyRef::convert_extern(&mut roots, native).unwrap();
        let entry = GcEphemeron::new(&mut roots, &owner, &key, Some(&value)).unwrap();
        let weak = WeakAnyRef::new(&mut roots, &owner, &value).unwrap();
        (key.to_owned_rooted(&mut roots).unwrap(), entry, weak)
    };
    store.gc(None).unwrap();
    assert!(entry.is_live(&mut store).unwrap());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(key);
    store.gc(None).unwrap();
    assert!(weak.upgrade(&mut store).unwrap().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn membership_and_nullable_values_have_distinct_lifetimes() {
    let (mut store, pre) = heap();
    let owner = object(&mut store, &pre, 1);
    let key = object(&mut store, &pre, 10);
    let entry = GcEphemeron::new(&mut store, &owner, &key, None).unwrap();
    store.gc(None).unwrap();
    assert!(entry.matches_key(&mut store, &key).unwrap());
    assert!(entry.value(&mut store).unwrap().is_none());
    let weak = {
        let mut roots = RootScope::new(&mut store);
        let value = object(&mut roots, &pre, 20);
        entry.set_value(&mut roots, Some(&value)).unwrap();
        WeakAnyRef::new(&mut roots, &owner, &value).unwrap()
    };
    store.gc(None).unwrap();
    {
        let mut roots = RootScope::new(&mut store);
        let value = weak.upgrade(&mut roots).unwrap().unwrap();
        assert_eq!(marker(&mut roots, value), 20);
    }
    entry.set_value(&mut store, None).unwrap();
    store.gc(None).unwrap();
    assert!(entry.matches_key(&mut store, &key).unwrap());
    assert!(weak.upgrade(&mut store).unwrap().is_none());
}

#[test]
fn raw_operands_name_the_same_entries_as_rooted_ones_until_the_next_collection() {
    let (mut store, pre) = heap();
    let owner = object(&mut store, &pre, 1);
    let key = {
        let mut roots = RootScope::new(&mut store);
        object(&mut roots, &pre, 10)
            .to_owned_rooted(&mut roots)
            .unwrap()
    };
    let other = object(&mut store, &pre, 11);
    fn raw(store: &mut Store<()>, reference: &AnyRef) -> u32 {
        reference.to_raw(store).unwrap()
    }
    let (entry, watched) = {
        let mut roots = RootScope::new(&mut store);
        let value = object(&mut roots, &pre, 20);
        let (owner_raw, key_raw, value_raw) = (
            owner.to_raw(&mut roots).unwrap(),
            key.to_raw(&mut roots).unwrap(),
            value.to_raw(&mut roots).unwrap(),
        );
        // SAFETY: the three references are rooted and nothing has collected
        // since their raw values were taken.
        let entry =
            unsafe { GcEphemeron::new_raw(&mut roots, owner_raw, key_raw, value_raw) }.unwrap();
        assert!(entry.matches_key(&roots, &key).unwrap());
        assert!(entry.matches_raw(&roots, owner_raw, key_raw).unwrap());
        assert_eq!(entry.value_raw(&mut roots).unwrap(), value_raw);
        let stored = entry.value(&mut roots).unwrap().unwrap();
        assert!(Rooted::ref_eq(&roots, &stored, &value).unwrap());
        (entry, WeakAnyRef::new(&mut roots, &owner, &value).unwrap())
    };
    // Owner and key are both part of an entry's identity.
    let (owner_raw, key_raw, other_raw) = (
        raw(&mut store, &owner),
        raw(&mut store, &key),
        raw(&mut store, &other),
    );
    assert!(!entry.matches_raw(&store, key_raw, owner_raw).unwrap());
    assert!(!entry.matches_raw(&store, owner_raw, other_raw).unwrap());
    assert!(!entry.matches_raw(&store, other_raw, key_raw).unwrap());
    assert!(!entry.matches_raw(&store, owner_raw, 0).unwrap());
    // A collection moves every object. The entry follows its owner, key and
    // value; raw values taken before it name nothing any more.
    store.gc(None).unwrap();
    let (moved_owner, moved_key) = (raw(&mut store, &owner), raw(&mut store, &key));
    assert_ne!((moved_owner, moved_key), (owner_raw, key_raw));
    assert!(entry.matches_raw(&store, moved_owner, moved_key).unwrap());
    assert!(!entry.matches_raw(&store, owner_raw, key_raw).unwrap());
    {
        let mut roots = RootScope::new(&mut store);
        let value = watched.upgrade(&mut roots).unwrap().unwrap();
        assert_eq!(marker(&mut roots, value), 20);
        let value_raw = value.to_raw(&mut roots).unwrap();
        assert_eq!(entry.value_raw(&mut roots).unwrap(), value_raw);
        // Replacing and clearing the value through raw operands.
        let replacement = object(&mut roots, &pre, 30);
        let replacement_raw = replacement.to_raw(&mut roots).unwrap();
        // SAFETY: rooted, and nothing has collected since.
        unsafe { entry.set_value_raw(&mut roots, replacement_raw) }.unwrap();
        assert_eq!(entry.value_raw(&mut roots).unwrap(), replacement_raw);
    }
    store.gc(None).unwrap();
    assert!(watched.upgrade(&mut store).unwrap().is_none());
    {
        let mut roots = RootScope::new(&mut store);
        let replacement = entry.value(&mut roots).unwrap().unwrap();
        assert_eq!(marker(&mut roots, replacement), 30);
    }
    // SAFETY: zero is no reference.
    unsafe { entry.set_value_raw(&mut store, 0) }.unwrap();
    assert_eq!(entry.value_raw(&mut store).unwrap(), 0);
    assert!(entry.value(&mut store).unwrap().is_none());
    assert!(entry.is_live(&store).unwrap());
    // Null and i31 owners and keys are rejected; a null or i31 value is not.
    let (owner_raw, key_raw) = (raw(&mut store, &owner), raw(&mut store, &key));
    let immediate = AnyRef::from_i31(&mut store, I31::wrapping_u32(7))
        .to_raw(&mut store)
        .unwrap();
    // SAFETY: rooted references, null and an i31.
    unsafe {
        assert!(GcEphemeron::new_raw(&mut store, 0, key_raw, 0).is_err());
        assert!(GcEphemeron::new_raw(&mut store, owner_raw, 0, 0).is_err());
        assert!(GcEphemeron::new_raw(&mut store, immediate, key_raw, 0).is_err());
        assert!(GcEphemeron::new_raw(&mut store, owner_raw, immediate, 0).is_err());
        let small = GcEphemeron::new_raw(&mut store, owner_raw, key_raw, immediate).unwrap();
        assert_eq!(small.value_raw(&mut store).unwrap(), immediate);
        small.remove(&mut store).unwrap();
        assert!(!small.matches_raw(&store, owner_raw, key_raw).unwrap());
        assert_eq!(small.value_raw(&mut store).unwrap(), 0);
    }
    // A foreign Store's handle is an error, not a miss.
    let (mut foreign, _) = heap();
    assert!(entry.matches_raw(&foreign, owner_raw, key_raw).is_err());
    assert!(entry.value_raw(&mut foreign).is_err());
    // The entry retires with its key, whatever raw operands then say.
    drop(key);
    store.gc(None).unwrap();
    assert!(!entry.is_live(&store).unwrap());
    let owner_raw = raw(&mut store, &owner);
    assert!(!entry.matches_raw(&store, owner_raw, key_raw).unwrap());
    assert_eq!(entry.value_raw(&mut store).unwrap(), 0);
    // SAFETY: zero is no reference.
    assert!(unsafe { entry.set_value_raw(&mut store, 0) }.is_err());
}
