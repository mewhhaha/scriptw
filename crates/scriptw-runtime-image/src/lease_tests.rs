use super::*;
#[test]
fn concurrent_loads_share_bytes_and_the_last_lease_releases_them() {
    if !cfg!(all(
        target_endian = "little",
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        )
    )) {
        assert!(portable_native().is_none());
        assert!(native().is_none());
        return;
    }
    use std::sync::{Arc, Barrier};
    for acquire in [
        native as fn() -> Option<Arc<NativeRuntimeBytes>>,
        portable_native,
    ] {
        let barrier = Arc::new(Barrier::new(4));
        let threads = (0..4)
            .map(|_| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let bytes = acquire().expect("supported Cargo image");
                    barrier.wait();
                    bytes
                })
            })
            .collect::<Vec<_>>();
        let leases = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect::<Vec<_>>();
        assert!(leases.iter().all(|image| Arc::ptr_eq(image, &leases[0])));
        let digest = *leases[0].digest();
        let weak = Arc::downgrade(&leases[0]);
        drop(leases);
        assert!(
            weak.upgrade().is_none(),
            "the cache must not retain a strong decoded-byte owner"
        );
        let later = acquire().expect("a new actual load may decode again");
        assert_eq!(later.digest(), &digest);
    }
}
