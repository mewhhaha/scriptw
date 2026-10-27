//! Read linked Cargo storage without faulting its entire executable mapping.
//! No caller path or native cache is an input. Linux binds the linked symbol's
//! range to the actual executable inode; other cases retain the static source.
use std::io::{self, Read};
#[cfg(target_os = "linux")]
#[path = "native_input/elf_origin.rs"]
mod elf_origin;

fn trace_source(kind: &str) {
    if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
        eprintln!("scriptw wasm trace: runtime-native packed source: {kind}");
    }
}

pub(super) enum PackedReader {
    Embedded(&'static [u8]),
    #[cfg(target_os = "linux")]
    Executable(std::io::Take<std::fs::File>),
}

impl PackedReader {
    /// The caller selects only immutable Cargo-defined arrays through Image.
    pub(super) fn new(encoded: &'static [u8]) -> Self {
        #[cfg(target_os = "linux")]
        if let Some(reader) = executable_range(encoded) {
            return Self::Executable(reader);
        }
        trace_source("embedded");
        Self::Embedded(encoded)
    }
}

impl Read for PackedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Embedded(bytes) => bytes.read(buffer),
            #[cfg(target_os = "linux")]
            Self::Executable(reader) => reader.read(buffer),
        }
    }
}

#[cfg(target_os = "linux")]
struct ExecutableIdentity {
    device: u64,
    inode: u64,
    length: u64,
}

#[cfg(target_os = "linux")]
impl ExecutableIdentity {
    fn from_file(file: &std::fs::File) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata().ok()?;
        metadata.is_file().then_some(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
        })
    }

    fn range_offset(&self, line: &str, address: usize, length: usize) -> Option<u64> {
        let mut fields = line.split_whitespace();
        let (start, end) = fields.next()?.split_once('-')?;
        let start = usize::from_str_radix(start, 16).ok()?;
        let end = usize::from_str_radix(end, 16).ok()?;
        // Writable/private copies cannot establish that file bytes equal the
        // immutable linked symbol. Paths (including "deleted") are ignored.
        if fields.next()? != "r--p"
            || start >= end
            || length == 0
            || address < start
            || address.checked_add(length)? > end
        {
            return None;
        }
        let offset = u64::from_str_radix(fields.next()?, 16).ok()?;
        let (major, minor) = fields.next()?.split_once(':')?;
        let major = u64::from_str_radix(major, 16).ok()?;
        let minor = u64::from_str_radix(minor, 16).ok()?;
        let inode: u64 = fields.next()?.parse().ok()?;
        // Linux dev_t encoding (gnu_dev_major/minor), not path resolution.
        let expected_major = (self.device >> 8 & 0xfff) | (self.device >> 32 & 0xfffff000);
        let expected_minor = (self.device & 0xff) | (self.device >> 12 & 0xffffff00);
        if inode == 0 || inode != self.inode || major != expected_major || minor != expected_minor {
            return None;
        }
        let offset = offset.checked_add(u64::try_from(address.checked_sub(start)?).ok()?)?;
        (offset.checked_add(u64::try_from(length).ok()?)? <= self.length).then_some(offset)
    }
}

#[cfg(target_os = "linux")]
fn executable_range(encoded: &'static [u8]) -> Option<std::io::Take<std::fs::File>> {
    use std::io::{Seek, SeekFrom};
    const MAX_MAPS_BYTES: u64 = 1024 * 1024;
    // This kernel handle remains the running executable after rename/unlink.
    // A maps pathname or a current_exe() pathname is never opened as authority.
    let mut file = std::fs::File::open("/proc/self/exe").ok()?;
    let identity = ExecutableIdentity::from_file(&file)?;
    let mut maps = String::new();
    std::fs::File::open("/proc/self/maps")
        .ok()?
        .take(MAX_MAPS_BYTES + 1)
        .read_to_string(&mut maps)
        .ok()?;
    if maps.len() as u64 > MAX_MAPS_BYTES {
        return None;
    }
    // as_ptr/len do not read the packed payload through its mapped address.
    let mut matches = maps
        .lines()
        .filter_map(|line| identity.range_offset(line, encoded.as_ptr() as usize, encoded.len()));
    if let Some(offset) = matches.next() {
        if matches.next().is_some() {
            return None;
        }
        file.seek(SeekFrom::Start(offset)).ok()?;
        trace_source("executable-inode");
        return Some(file.take(u64::try_from(encoded.len()).ok()?));
    }
    // Some Linux filesystem views expose different device numbers through
    // stat and maps. Retain the strict identity path above; the alternative
    // proves origin independently from the kernel's main executable headers.
    let reader = elf_origin::KernelMainElf::open()?.read_symbol(encoded, &maps)?;
    trace_source("kernel-main-elf");
    Some(reader)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_input_and_embedded_source_have_identical_bounded_bytes() {
        static INPUT: &[u8] = b"Cargo storage range\0with a strict end";
        let mut reader = PackedReader::new(INPUT);
        let mut read = Vec::new();
        reader.read_to_end(&mut read).unwrap();
        assert_eq!(read, INPUT);
        assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
        let mut fallback = PackedReader::Embedded(INPUT);
        let mut read = Vec::new();
        fallback.read_to_end(&mut read).unwrap();
        assert_eq!(read, INPUT);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn range_admission_rejects_aliases_mutability_wrong_inode_and_overflows() {
        let identity = ExecutableIdentity {
            device: 0x801,
            inode: 42,
            length: 0x5000,
        };
        let line = "1000-2000 r--p 00001000 08:01 42 /ignored path (deleted)";
        assert_eq!(identity.range_offset(line, 0x1800, 0x20), Some(0x1800));
        for changed in [
            line.replace("r--p", "rw-p"),
            line.replace("r--p", "r--s"),
            line.replace("08:01", "08:02"),
            line.replace(" 42 ", " 43 "),
            line.replace(" 42 ", " 0 "),
            line.replace("00001000", "ffffffffffffffff"),
        ] {
            assert!(identity.range_offset(&changed, 0x1800, 0x20).is_none());
        }
        for (address, length) in [(0xfff, 1), (0x1fff, 2), (0x1000, 0), (usize::MAX, 2)] {
            assert!(identity.range_offset(line, address, length).is_none());
        }
        let short = ExecutableIdentity {
            length: 0x1801,
            ..identity
        };
        assert!(short.range_offset(line, 0x1800, 2).is_none());
        // Upper device bits must not alias the common small-device case.
        let wide = ExecutableIdentity {
            device: (0x123_u64 << 32) | (0x456_u64 << 12) | 0x801,
            ..identity
        };
        assert!(wide.range_offset(line, 0x1800, 1).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inode_bound_reader_survives_rename_and_unlink_and_stops_at_its_range() {
        use std::io::{Seek, SeekFrom, Write};
        use std::os::unix::fs::MetadataExt;
        let directory = std::env::temp_dir().join(format!(
            "scriptw-image-range-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let original = directory.join("original");
        let renamed = directory.join("renamed");
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&original)
            .unwrap();
        file.write_all(b"before:payload:after").unwrap();
        let metadata = file.metadata().unwrap();
        let identity = ExecutableIdentity::from_file(&file).unwrap();
        let major = (metadata.dev() >> 8 & 0xfff) | (metadata.dev() >> 32 & 0xfffff000);
        let minor = (metadata.dev() & 0xff) | (metadata.dev() >> 12 & 0xffffff00);
        let line = format!(
            "1000-1014 r--p 00000000 {major:x}:{minor:x} {} /not-opened",
            metadata.ino()
        );
        let offset = identity.range_offset(&line, 0x1007, 7).unwrap();
        std::fs::rename(&original, &renamed).unwrap();
        std::fs::remove_file(&renamed).unwrap();
        assert_eq!(file.metadata().unwrap().nlink(), 0);
        let still_owned = ExecutableIdentity::from_file(&file).unwrap();
        assert_eq!(still_owned.range_offset(&line, 0x1007, 7), Some(offset));
        file.seek(SeekFrom::Start(offset)).unwrap();
        let mut bounded = file.take(7);
        let mut result = Vec::new();
        bounded.read_to_end(&mut result).unwrap();
        assert_eq!(result, b"payload");
        assert_eq!(bounded.read(&mut [0; 1]).unwrap(), 0);
        drop(bounded);
        std::fs::remove_dir(directory).unwrap();
    }
}
