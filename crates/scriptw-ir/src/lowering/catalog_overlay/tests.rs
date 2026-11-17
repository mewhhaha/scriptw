use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Debug)]
struct Counted {
    value: usize,
    clones: Arc<AtomicUsize>,
}

impl Clone for Counted {
    fn clone(&self) -> Self {
        self.clones.fetch_add(1, Ordering::SeqCst);
        Self {
            value: self.value,
            clones: self.clones.clone(),
        }
    }
}

fn counted_baseline() -> (&'static BTreeMap<String, Counted>, Arc<AtomicUsize>) {
    let clones = Arc::new(AtomicUsize::new(0));
    let baseline = ["a", "c", "e"]
        .into_iter()
        .enumerate()
        .map(|(value, key)| {
            (
                key.to_string(),
                Counted {
                    value,
                    clones: clones.clone(),
                },
            )
        })
        .collect();
    (Box::leak(Box::new(baseline)), clones)
}

#[test]
fn untouched_catalog_and_prepass_forks_never_clone_recipe_values() {
    let (baseline, clones) = counted_baseline();
    let mut facts = CatalogOverlay::new(baseline);
    let fork = facts.clone();
    assert_eq!(facts.get("a").unwrap().value, 0);
    assert!(std::ptr::eq(
        facts.get("c").unwrap(),
        baseline.get("c").unwrap()
    ));
    assert!(facts.same_baseline(&fork));
    assert_eq!(clones.load(Ordering::SeqCst), 0);
    facts.get_mut("a").unwrap().value = 7;
    facts.get_mut("a").unwrap().value = 8;
    assert_eq!(clones.load(Ordering::SeqCst), 1);
    assert_eq!(fork.get("a").unwrap().value, 0);
    let mut second = facts.clone();
    assert_eq!(clones.load(Ordering::SeqCst), 1);
    assert!(std::ptr::eq(
        facts.get("a").unwrap(),
        second.get("a").unwrap()
    ));
    second.get_mut("a").unwrap().value = 9;
    assert_eq!(clones.load(Ordering::SeqCst), 2);
    assert_eq!(facts.get("a").unwrap().value, 8);
    assert_eq!(baseline.get("a").unwrap().value, 0);
}

#[test]
fn merged_iteration_preserves_order_overrides_and_deleted_baseline_entries() {
    let baseline = Box::leak(Box::new(BTreeMap::from([
        ("a".to_string(), 1),
        ("c".to_string(), 3),
        ("e".to_string(), 5),
    ])));
    let mut facts = CatalogOverlay::new(baseline);
    assert_eq!(facts.remove("a"), Some(1));
    assert_eq!(facts.get("c"), Some(&3));
    facts.insert("c".to_string(), 30);
    assert_eq!(facts.get("c"), Some(&30));
    facts.insert("b".to_string(), 2);
    facts.insert("f".to_string(), 6);
    assert_eq!(
        facts
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect::<Vec<_>>(),
        [("b", 2), ("c", 30), ("e", 5), ("f", 6)]
    );
    assert!(facts.get_mut("a").is_none());
    assert_eq!(facts.remove("b"), Some(2));
    assert!(!facts.contains_key("b"));
    assert_eq!(baseline.get("a"), Some(&1));
    facts.set_effective("c".to_string(), Some(3));
    assert!(!facts.changed_keys().any(|key| key == "c"));
}

#[test]
fn fixpoint_equality_compares_effective_facts_without_materializing_recipes() {
    let first = Box::leak(Box::new(BTreeMap::from([("x".to_string(), 1)])));
    let other = Box::leak(Box::new(first.clone()));
    let facts = CatalogOverlay::new(first);
    let mut fork = facts.clone();
    assert_eq!(fork.get_mut("x").map(|value| *value), Some(1));
    assert_eq!(facts, fork);
    assert!(!facts.same_baseline(&CatalogOverlay::new(other)));
    assert_eq!(facts, CatalogOverlay::new(other));
    fork.remove("x");
    assert_ne!(facts, fork);
    fork.set_effective("x".to_string(), Some(1));
    assert_eq!(facts, fork);
    assert_eq!(fork.changed_keys().count(), 0);
}

#[test]
fn targeted_bulk_edits_clone_only_selected_rows_in_effective_key_order() {
    let (baseline, clones) = counted_baseline();
    let mut facts = CatalogOverlay::new(baseline);
    facts.edit_matching(|_, _| false, |_, _| panic!("no row selected"));
    assert_eq!(clones.load(Ordering::SeqCst), 0);
    let mut order = Vec::new();
    facts.edit_matching(
        |key, _| key != "c",
        |key, value| {
            order.push(key.clone());
            value.value += 10;
        },
    );
    assert_eq!(order, ["a", "e"]);
    assert_eq!(clones.load(Ordering::SeqCst), 2);
    assert_eq!(facts.changed_keys().count(), 2);
    assert!(std::ptr::eq(
        facts.get("c").unwrap(),
        baseline.get("c").unwrap()
    ));
}

#[test]
fn settled_idempotent_edit_revisits_exactly_the_keys_mutated_since() {
    let baseline = Box::leak(Box::new(BTreeMap::from([
        ("a".to_string(), 1),
        ("b".to_string(), 2),
        ("z".to_string(), 26),
    ])));
    let edit = IdempotentEdit::UnknownPropertyHook;
    let keys = Arc::new(BTreeSet::from(["a".to_string(), "b".to_string()]));
    let mut facts = CatalogOverlay::new(baseline);
    assert_eq!(facts.unsettled_keys(edit, &keys), ["a", "b"]);
    facts.settle(edit, keys.clone());
    assert!(facts.unsettled_keys(edit, &keys).is_empty());
    // A different key set with equal contents is not the settled one.
    let other = Arc::new((*keys).clone());
    assert_eq!(facts.unsettled_keys(edit, &other), ["a", "b"]);
    // Every mutable access unsettles its key, even one that writes nothing,
    // and keys outside the settled set are not tracked.
    let _ = facts.get_mut("b");
    facts.insert("z".to_string(), 0);
    assert_eq!(facts.unsettled_keys(edit, &keys), ["b"]);
    assert!(facts.is_settled(edit, "a"));
    assert!(!facts.is_settled(edit, "b"));
    assert!(!facts.is_settled(edit, "z"));
    facts.set_effective("a".to_string(), Some(5));
    assert_eq!(facts.unsettled_keys(edit, &keys), ["a", "b"]);
    let mut fork = facts.clone();
    fork.settle(edit, keys.clone());
    assert_eq!(facts.unsettled_keys(edit, &keys), ["a", "b"]);
    fork.edit_matching(|key, _| key == "a", |_, value| *value += 1);
    assert_eq!(fork.unsettled_keys(edit, &keys), ["a"]);
    assert!(fork.empty_changes().unsettled_keys(edit, &keys).len() == 2);
}

#[path = "tests/changed_facts.rs"]
mod changed_facts;
