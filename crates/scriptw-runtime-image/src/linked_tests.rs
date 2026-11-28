use super::*;
use sha2::{Digest, Sha256};

/// The load path trusts three build facts instead of re-deriving them at
/// every start: the attested digest, the link's alignment and the image's
/// place in the executable file. Check each against the actual linked bytes.
#[test]
fn linked_images_match_their_build_digest_and_map_from_the_executable() {
    #[allow(unused_mut)]
    let mut images = vec![native().expect("supported Cargo image")];
    #[cfg(not(feature = "native-host-only"))]
    images.push(portable_native().expect("supported Cargo image"));
    for image in images {
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(image.bytes())),
            *image.build_digest(),
            "the packaging step attested exactly the bytes it linked"
        );
        // Older kernels only page-align a PIE's load bias, so the address is
        // checked against the page; the file offset is checked below.
        assert_eq!(image.bytes().as_ptr() as usize % 4096, 0);
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
    #[cfg(not(feature = "native-host-only"))]
    {
        let (host, portable) = (native().unwrap(), portable_native().unwrap());
        assert_ne!(host.build_digest(), portable.build_digest());
    }
    #[cfg(feature = "native-host-only")]
    assert!(portable_native().is_none());
}
