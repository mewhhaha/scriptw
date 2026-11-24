use super::*;
use std::sync::{Arc, Barrier};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "scriptw-cache-atomic-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn distinct_cache_instances_publish_complete_entries() {
    let directory = Directory::new();
    let caches = (0..8)
        .map(|_| FunctionCache::new(directory.0.clone(), 1024 * 1024).unwrap())
        .collect::<Vec<_>>();
    let barrier = Arc::new(Barrier::new(caches.len()));
    let threads = caches
        .into_iter()
        .enumerate()
        .map(|(value, cache)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                assert!(cache.insert(b"same-key", vec![value as u8; 65_536]));
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    let cache = FunctionCache::new(directory.0.clone(), 1024 * 1024).unwrap();
    let bytes = cache.get(b"same-key").unwrap();
    assert_eq!(bytes.len(), 65_536);
    assert!(bytes.iter().all(|byte| *byte == bytes[0]));
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn preexisting_temporary_files_are_not_truncated() {
    let directory = Directory::new();
    let path = directory.0.join("entry");
    let stale = directory
        .0
        .join(format!("entry.tmp-{}-0", std::process::id()));
    fs::write(&stale, b"do not touch").unwrap();
    insert_atomic(
        &path,
        b"new bytes",
        EntryPublication::SyncedBeforeRename,
        &AtomicU64::new(0),
    )
    .unwrap();
    assert_eq!(fs::read(&stale).unwrap(), b"do not touch");
    assert_eq!(fs::read(&path).unwrap(), b"new bytes");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn failed_publication_removes_its_temporary_file() {
    let directory = Directory::new();
    let path = directory.0.join("entry");
    fs::create_dir(&path).unwrap();
    assert!(insert_atomic(
        &path,
        b"bytes",
        EntryPublication::Unsynced,
        &AtomicU64::new(0)
    )
    .is_err());
    assert!(path.is_dir());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn exhausted_temporary_identities_fail_without_wrapping() {
    let directory = Directory::new();
    let next = AtomicU64::new(u64::MAX);
    assert!(insert_atomic(
        &directory.0.join("entry"),
        b"bytes",
        EntryPublication::Unsynced,
        &next,
    )
    .is_err());
    assert_eq!(next.load(Ordering::Relaxed), u64::MAX);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn preexisting_temporary_symlinks_are_not_followed() {
    let directory = Directory::new();
    let outside = Directory::new();
    let sentinel = outside.0.join("sentinel");
    fs::write(&sentinel, b"unchanged").unwrap();
    let link = directory
        .0
        .join(format!("entry.tmp-{}-0", std::process::id()));
    std::os::unix::fs::symlink(&sentinel, &link).unwrap();
    insert_atomic(
        &directory.0.join("entry"),
        b"new",
        EntryPublication::Unsynced,
        &AtomicU64::new(0),
    )
    .unwrap();
    assert_eq!(fs::read(&sentinel).unwrap(), b"unchanged");
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
}
