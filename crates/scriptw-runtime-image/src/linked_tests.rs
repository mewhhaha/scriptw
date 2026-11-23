use super::*;
use sha2::{Digest, Sha256};

/// The load path trusts three build facts instead of re-deriving them at
/// every start: the attested digest, the link's alignment and the image's
/// place in the executable file. Check each against the actual linked bytes.
#[test]
fn linked_images_match_their_build_digest_and_map_from_the_executable() {
    for image in [
        native().expect("supported Cargo image"),
        portable_native().expect("supported Cargo image"),
    ] {
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(image.bytes())),
            *image.build_digest(),
            "the packaging step attested exactly the bytes it linked"
        );
        assert_eq!(image.bytes().as_ptr() as usize % (64 * 1024), 0);
        assert_eq!(&image.bytes()[..4], b"\x7fELF", "a raw Wasmtime image");
        if !cfg!(target_os = "linux") {
            assert!(image.executable_range().is_none());
            continue;
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::FileExt;
            let range = image
                .executable_range()
                .expect("a linked image is a read-only range of this test executable");
            assert_eq!(range.length(), image.bytes().len());
            assert_eq!(range.offset() % 4096, 0, "mappable at its file offset");
            let file = range.try_clone_file().unwrap();
            let mut read = vec![0; range.length()];
            file.read_exact_at(&mut read, range.offset()).unwrap();
            assert!(
                read == image.bytes(),
                "the file range holds the linked image"
            );
        }
    }
    let (host, portable) = (native().unwrap(), portable_native().unwrap());
    assert_ne!(host.build_digest(), portable.build_digest());
}
