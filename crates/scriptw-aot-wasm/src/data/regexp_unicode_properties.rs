use super::*;
use scriptw_ir::{regexp_unicode_property_catalog, RegExpUnicodePropertyCatalogValue};

#[derive(Clone, Copy)]
#[repr(u64)]
pub(crate) enum RegExpUnicodePropertyImageKind {
    CodePoints = 1,
    Strings = 2,
}

#[derive(Clone, Copy)]
#[repr(u64)]
pub(crate) enum RegExpUnicodePropertyImageWord {
    NamePointer = 0,
    NameLength = 8,
    PayloadPointer = 16,
    PayloadCount = 24,
    Kind = 32,
}

impl RegExpUnicodePropertyImageWord {
    pub(crate) const ROW_BYTES: u64 = 40;
}

/// A Strings property payload is a table of these descriptors. The backing
/// code-point keys are immutable u32 little-endian words, never UTF-16 halves.
#[derive(Clone, Copy)]
#[repr(u64)]
pub(crate) enum RegExpUnicodePropertyImageStringWord {
    CodePointsPointer = 0,
    CodePointCount = 8,
}

impl RegExpUnicodePropertyImageStringWord {
    pub(crate) const ROW_BYTES: u64 = 16;
}

/// Created only while serializing the complete validated native catalog.
/// Private fields retain one checked immutable image; emitted consumers cannot
/// choose an arbitrary table pointer or independent property domain.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RegExpUnicodePropertyImage {
    rows: u32,
    count: u32,
    region: RegExpUnicodePropertyRegion,
}

/// Sealed contiguous image with unchanged embedded virtual addresses.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RegExpUnicodePropertyRegion {
    start: u32,
    end: u32,
}
impl RegExpUnicodePropertyRegion {
    pub(crate) fn start(self) -> u32 {
        self.start
    }
    pub(crate) fn end(self) -> u32 {
        self.end
    }
    pub(crate) fn length(self) -> u32 {
        self.end - self.start
    }
    pub(crate) const fn segment(self) -> PassiveDataSegment {
        PassiveDataSegment::RegExpUnicodeProperties
    }
    pub(super) fn offsets(self) -> std::ops::Range<usize> {
        (self.start - STATIC_DATA_OFFSET) as usize..(self.end - STATIC_DATA_OFFSET) as usize
    }
}
impl RegExpUnicodePropertyImage {
    pub(crate) const fn rows(self) -> u32 {
        self.rows
    }
    pub(crate) const fn count(self) -> u32 {
        self.count
    }
}

impl StringPool {
    fn property_image_pointer(&self) -> u32 {
        STATIC_DATA_OFFSET
            .checked_add(u32::try_from(self.bytes.len()).expect("Unicode property image size"))
            .expect("Unicode property image address space")
    }

    pub(super) fn append_regexp_unicode_property_image(&mut self) {
        if self.regexp_unicode_property_image.is_some() {
            return;
        }
        let start = self.property_image_pointer();
        let mut ranges: BTreeMap<Vec<(u32, u32)>, (u32, u32)> = BTreeMap::new();
        let mut string_keys: BTreeMap<Vec<u32>, (u32, u32)> = BTreeMap::new();
        let mut rows = Vec::new();
        for entry in regexp_unicode_property_catalog() {
            let name = entry.name().as_bytes();
            assert!(
                name.is_ascii() && !name.is_empty(),
                "validated property image name"
            );
            let name_pointer = self.property_image_pointer();
            self.bytes.extend_from_slice(name);
            let name_length = u32::try_from(name.len()).expect("property image name length");
            let (kind, pointer, count) = match entry.value() {
                RegExpUnicodePropertyCatalogValue::CodePoints(values) => {
                    assert!(values
                        .iter()
                        .all(|(start, end)| start <= end && *end <= 0x10ffff));
                    assert!(values.windows(2).all(|pair| pair[0].1 < pair[1].0));
                    let (pointer, count) = if let Some(pair) = ranges.get(values) {
                        *pair
                    } else {
                        self.align_bytes(8);
                        let pointer = self.property_image_pointer();
                        let count =
                            u32::try_from(values.len()).expect("property image range count");
                        for (start, end) in values {
                            self.bytes.extend_from_slice(&start.to_le_bytes());
                            self.bytes.extend_from_slice(&end.to_le_bytes());
                        }
                        ranges.insert(values.to_vec(), (pointer, count));
                        (pointer, count)
                    };
                    (RegExpUnicodePropertyImageKind::CodePoints, pointer, count)
                }
                RegExpUnicodePropertyCatalogValue::Strings(sequences) => {
                    let mut descriptors = Vec::with_capacity(sequences.len());
                    for sequence in sequences {
                        let (pointer, count) = if let Some(pair) = string_keys.get(sequence) {
                            *pair
                        } else {
                            self.align_bytes(4);
                            let pointer = self.property_image_pointer();
                            let count = u32::try_from(sequence.len())
                                .expect("property image string code-point count");
                            for code_point in sequence {
                                self.bytes.extend_from_slice(&code_point.to_le_bytes());
                            }
                            string_keys.insert(sequence.clone(), (pointer, count));
                            (pointer, count)
                        };
                        descriptors.push([u64::from(pointer), u64::from(count)]);
                    }
                    self.align_bytes(8);
                    let pointer = self.property_image_pointer();
                    let count = u32::try_from(descriptors.len())
                        .expect("property image string sequence count");
                    for descriptor in descriptors {
                        for word in descriptor {
                            self.bytes.extend_from_slice(&word.to_le_bytes());
                        }
                    }
                    (RegExpUnicodePropertyImageKind::Strings, pointer, count)
                }
            };
            rows.push([
                u64::from(name_pointer),
                u64::from(name_length),
                u64::from(pointer),
                u64::from(count),
                kind as u64,
            ]);
        }
        self.align_bytes(8);
        let pointer = self.property_image_pointer();
        let count = u32::try_from(rows.len()).expect("property image catalog count");
        for row in rows {
            for word in row {
                self.bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        // Check the complete image end before exposing its only pointer owner.
        let end = self.property_image_pointer();
        self.regexp_unicode_property_image = Some(RegExpUnicodePropertyImage {
            rows: pointer,
            count,
            region: RegExpUnicodePropertyRegion { start, end },
        });
    }

    pub(crate) fn regexp_unicode_property_region(&self) -> RegExpUnicodePropertyRegion {
        self.regexp_unicode_property_image
            .or_else(|| {
                self.runtime_prefix()
                    .and_then(|prefix| prefix.regexp_unicode_property_image)
            })
            .map(|image| image.region)
            .unwrap_or(RegExpUnicodePropertyRegion {
                start: STATIC_DATA_OFFSET,
                end: STATIC_DATA_OFFSET,
            })
    }
    pub(crate) fn regexp_unicode_property_image(&self) -> RegExpUnicodePropertyImage {
        self.regexp_unicode_property_image
            .or_else(|| {
                self.runtime_prefix()
                    .and_then(|prefix| prefix.regexp_unicode_property_image)
            })
            .expect("computed RegExp compiler requires complete Unicode property image")
    }
}
