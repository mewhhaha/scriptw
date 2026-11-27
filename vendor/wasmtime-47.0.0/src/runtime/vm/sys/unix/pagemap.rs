//! Module for Linux pagemap based tracking of dirty pages.
//!
//! For other platforms, a no-op implementation is provided.

#[cfg(feature = "pooling-allocator")]
use crate::prelude::*;

use self::ioctl::{Categories, PageMapScanBuilder};
use crate::runtime::vm::{HostAlignedByteCount, host_page_size};
use rustix::ioctl::ioctl;
use std::fs::File;
use std::mem::MaybeUninit;
use std::ptr;

/// A static file-per-process which represents this process's page map file.
///
/// Note that this is required to be updated on a fork because otherwise this'll
/// refer to the parent process's page map instead of the child process's page
/// map. Thus when first initializing this file the `pthread_atfork` function is
/// used to hook the child process to update this.
///
/// Also note that updating this is not done via mutation but rather it's done
/// with `dup2` to replace the file descriptor that `File` points to in-place.
/// The local copy of of `File` is then closed in the atfork handler.
#[cfg(feature = "pooling-allocator")]
static PROCESS_PAGEMAP: std::sync::LazyLock<Option<File>> = std::sync::LazyLock::new(|| {
    use rustix::fd::AsRawFd;

    let pagemap = File::open("/proc/self/pagemap").ok()?;

    // SAFETY: all libc functions are unsafe by default, and we're basically
    // going to do our damndest to make sure this invocation of `pthread_atfork`
    // is safe, namely the handler registered here is intentionally quite
    // minimal and only accesses the `PROCESS_PAGEMAP`.
    let rc = unsafe { libc::pthread_atfork(None, None, Some(after_fork_in_child)) };
    if rc != 0 {
        return None;
    }

    return Some(pagemap);

    /// Hook executed as part of `pthread_atfork` in the child process after a
    /// fork.
    ///
    /// # Safety
    ///
    /// This function is not safe to call in general and additionally has its
    /// own stringent safety requirements. This is after a fork but before exec
    /// so all the safety requirements of `Command::pre_exec` in the standard
    /// library apply here. Effectively the standard library primitives are
    /// avoided here as they aren't necessarily safe to execute in this context.
    unsafe extern "C" fn after_fork_in_child() {
        let Some(parent_pagemap) = PROCESS_PAGEMAP.as_ref() else {
            // This should not be reachable, but to avoid panic infrastructure
            // here this is just skipped instead.
            return;
        };

        // SAFETY: see function documentation.
        //
        // Here `/proc/self/pagemap` is opened in the child. If that fails for
        // whatever reason then the pagemap is replaced with `/dev/null` which
        // means that all future ioctls for `PAGEMAP_SCAN` will fail. If that
        // fails then that's left to abort the process for now. If that's
        // problematic we may want to consider opening a local pipe and then
        // installing that here? Unsure.
        //
        // Once a fd is opened the `dup2` syscall is used to replace the
        // previous file descriptor stored in `parent_pagemap`. That'll update
        // the pagemap in-place in this child for all future use in case this is
        // further used in the child.
        //
        // And finally once that's all done the `child_pagemap` is itself
        // closed since we have no more need for it.
        unsafe {
            let flags = libc::O_CLOEXEC | libc::O_RDONLY;
            let mut child_pagemap = libc::open(c"/proc/self/pagemap".as_ptr(), flags);
            if child_pagemap == -1 {
                child_pagemap = libc::open(c"/dev/null".as_ptr(), flags);
            }
            if child_pagemap == -1 {
                libc::abort();
            }

            let rc = libc::dup2(child_pagemap, parent_pagemap.as_raw_fd());
            if rc == -1 {
                libc::abort();
            }
            let rc = libc::close(child_pagemap);
            if rc == -1 {
                libc::abort();
            }
        }
    }
});

#[derive(Debug)]
pub struct PageMap(&'static File);

impl PageMap {
    #[cfg(feature = "pooling-allocator")]
    pub fn new() -> Option<PageMap> {
        let file = PROCESS_PAGEMAP.as_ref()?;

        // Check if the `pagemap_scan` ioctl is supported.
        let mut regions = vec![MaybeUninit::uninit(); 1];
        let pm_scan = PageMapScanBuilder::new(ptr::slice_from_raw_parts(ptr::null_mut(), 0))
            .max_pages(1)
            .return_mask(Categories::empty())
            .category_mask(Categories::all())
            .build(&mut regions);

        // SAFETY: we did our best in the `ioctl` code below to model this ioctl
        // safely, and it's safe to issue the ioctl on `/proc/self/pagemap`.
        unsafe {
            ioctl(&file, pm_scan).ok()?;
        }
        Some(PageMap(file))
    }
}

/// Resets `ptr` for `len` bytes.
///
/// This function is a dual implementation of this function in the
/// `pagemap_disabled` module except it uses the `PAGEMAP_SCAN` [ioctl] on
/// Linux to be more clever about calling the `reset_manually` closure.
/// Semantically though this still has the same meaning where all of `ptr` for
/// `len` bytes will be reset, either through `reset_manually` or `decommit`.
/// The optimization here is that `reset_manually` will only be called on
/// regions as-necessary and `decommit` can be skipped entirely in some
/// situations.
///
/// The `PAGEMAP_SCAN` [ioctl] scans a region of memory and reports back
/// "regions of interest" as configured by the scan. It also does things with
/// uffd and write-protected pages, but that's not leveraged here. Specifically
/// this function will perform a scan of `ptr` for `len` bytes which will search
/// for pages that:
///
/// * Are present, or swapped out (which includes migration entries).
/// * Are NOT backed by the "zero" page.
/// * Are NOT backed by a "file" page.
///
/// By default WebAssembly memories/tables are all accessible virtual memory,
/// but paging optimizations on Linux means they don't actually have a backing
/// page. For example when an instance starts for the first time its entire
/// linear memory will be mapped as anonymous memory where page-table-entries
/// don't even exist for the new memory. Most modules will then have an initial
/// image mapped in, but that still won't have any page table entries. When
/// memory is accessed for the first time a page fault will be generated and
/// handled by the kernel.
///
/// If memory is read then the page fault will force a PTE to be allocated to
/// either zero-backed pages (e.g. ZFOD behavior) or a file-backed page if the
/// memory is in the initial image mapping. For ZFOD the kernel uses a single
/// page for the entire system of zeros and for files it uses the page map cache
/// in the kernel to share the same page across many mappings (as it's all
/// read-only anyway). Note that in this situation the PTE allocated will have
/// the write permission disabled meaning that a write will later generate a
/// page fault.
///
/// If memory is written then that will allocate a fresh page from the kernel.
/// If the PTE was not previously present then the fresh page is initialized
/// either with zeros or a copy of the contents of the file-backed mapping. If
/// the PTE was previously present then its previous contents are copied into
/// the new page. In all of these cases the final PTE allocate will be a private
/// page to just this process which will be reflected nowhere else on the
/// system.
///
/// Putting this all together this helps explain the search criteria for
/// `PAGEMAP_SCAN`, notably:
///
/// * `Categories::PRESENT | Categories::SWAPPED` (any of) - a page whose PTE
///   is neither present nor a swap entry was never populated, so the guest
///   left nothing there. A dirty page is not necessarily present at the moment
///   of the scan: reclaim may have swapped it out, and compaction, NUMA
///   balancing or a THP collapse may be migrating it. The kernel reports both
///   of those as a swap entry (`SWAPPED`), so both categories are searched
///   (Scriptw patch; upstream required `PRESENT` and `WRITTEN`, which skipped
///   swapped/migrating pages and also skipped all dirty pages on Linux 7.x
///   non-uffd-wp VMAs where `PAGE_IS_WRITTEN` is 0).
///
/// * `!Categories::PFNZERO` - if a page is mapped to the zero page then it's
///   guaranteed to be readonly and it means that wasm read the memory but
///   didn't write to it, additionally meaning it doesn't need to be reset.
///
/// * `!Categories::FILE` - similar to `!PFNZERO` if a page is mapped to a file
///   then for us that means it's readonly meaning wasm only read the memory,
///   didn't write to it, so the page can be skipped.
///
/// The `PAGEMAP_SCAN` will report back a set of contiguous regions of memory
/// which match our scan flags that we're looking for, each with its `SWAPPED`
/// category. A present region is passed to `reset_manually` as-is. A swapped
/// region is passed to `decommit` instead: writing it would first read it back
/// from swap only to overwrite it, and a page that is swapped in or migrated
/// back between the scan and the decommit is discarded all the same. Either
/// way every reported page reads as its original contents afterwards, which is
/// what makes skipping the unreported ones sound. The ioctl will additionally then
/// report a "walk_end" address which is the last address it considered before
/// the scan was halted. A scan can stop for 3 reasons:
///
/// * The end of the region of memory being scanned was reached. In this case
///   the entire region was scanned meaning that all dirty memory was reported
///   through `reset_manually`. This means that `decommit` can be skipped
///   entirely (or invoked with a 0 length here which will also end up with it
///   being skipped).
///
/// * The scan's `max_pages` setting was reached. The `keep_resident` argument
///   indicates the maximal amount of memory to pass to `reset_manually` and
///   this translates to the `max_pages` configuration option to the ioctl. The
///   sum total of the size of all regions reported from the ioctl is guaranteed
///   to be less than `max_pages`. This means that if a scan reaches the
///   `keep_resident` limit before reaching the end then the ioctl will bail out
///   early. That means that the wasm module's working set of memory was larger
///   than `keep_resident` and then the rest of it will be `decommit`'d away.
///
/// * The scan's returned set of regions exceeds the capacity passed into the
///   ioctl. The `pm_scan_arg` of the ioctl takes a `vec` and `vec_len` which is
///   a region of memory to store a list of `page_region` structures. Below this
///   is always `MAX_REGIONS`. If there are more than this number of disjoint
///   regions of memory that need to be reported then the ioctl will also return
///   early without reaching the end of memory. Note that this means that all
///   further memory will be `decommit`'d with reported regions still going to
///   `reset_manually`. This is arguably something we should detect and improve
///   in Wasmtime, but for now `MAX_REGIONS` is hardcoded.
///
/// In the end this ends up being a "more clever" version of this function than
/// the one in the `pagemap_disabled` module. By using `PAGEMAP_SCAN` we can
/// search for the first `keep_resident` bytes of dirty memory written to by a
/// wasm guest instead of assuming the first `keep_resident` bytes of the region
/// were modified by the guest. This crucially enables the `decommit` operation
/// to a noop if the wasm guest's set of working memory is less than
/// `keep_resident` which means that `memcpy` is sufficient to reset a linear
/// memory or table. This directly translates to higher throughput as it avoids
/// IPIs and synchronization updating page tables and additionally avoids page
/// faults on future executions of the same module.
///
/// # Safety
///
/// Requires that `ptr` is valid to read and write for `len` bytes.
///
/// [ioctl]: https://www.man7.org/linux/man-pages/man2/PAGEMAP_SCAN.2const.html
pub unsafe fn reset_with_pagemap(
    mut pagemap: Option<&PageMap>,
    ptr: *mut u8,
    len: HostAlignedByteCount,
    mut keep_resident: HostAlignedByteCount,
    mut reset_manually: impl FnMut(&mut [u8]),
    mut decommit: impl FnMut(*mut u8, usize),
) -> usize {
    keep_resident = keep_resident.min(len);
    let host_page_size = host_page_size();

    if pagemap.is_some() {
        // Nothing to keep resident? fall back to the default behavior.
        if keep_resident.byte_count() == 0 {
            pagemap = None;
        }

        // Keeping less than one page of memory resident when the original
        // mapping itself is also less than a page? Also fall back to the
        // default behavior as this'll just be a simple memcpy.
        if keep_resident.byte_count() <= host_page_size && len.byte_count() <= host_page_size {
            pagemap = None;
        }
    }

    let pagemap = match pagemap {
        Some(pagemap) => pagemap,

        // Fall back to the default behavior.
        //
        // SAFETY: the safety requirement of
        // `pagemap_disabled::reset_with_pagemap` is the same as this function.
        _ => unsafe {
            return crate::runtime::vm::pagemap_disabled::reset_with_pagemap(
                None,
                ptr,
                len,
                keep_resident,
                reset_manually,
                decommit,
            );
        },
    };

    // For now use a fixed set of regions on the stack, but in the future this
    // may want to use a dynamically allocated vector for more regions for
    // example.
    const MAX_REGIONS: usize = 32;
    let mut storage = [MaybeUninit::uninit(); MAX_REGIONS];

    let scan_arg = PageMapScanBuilder::new(ptr::slice_from_raw_parts(ptr, len.byte_count()))
        .max_pages(keep_resident.byte_count() / host_page_size)
        // We specifically want pages that are NOT backed by the zero page or
        // backed by files. Such pages mean that they haven't changed from their
        // original contents, so they're inverted.
        .category_inverted(DirtyPageScan::INVERTED)
        .category_mask(DirtyPageScan::MASK)
        .category_anyof_mask(DirtyPageScan::ANY_OF)
        .return_mask(DirtyPageScan::RETURNED)
        .build(&mut storage);

    // SAFETY: this should be a safe ioctl as we control the fd we're operating
    // on plus all of `scan_arg`, but this relies on `Ioctl` below being the
    // correct implementation and such.
    let result = match unsafe { ioctl(&pagemap.0, scan_arg) } {
        Ok(result) => result,

        // If the ioctl fails for whatever reason, we at least tried, so fall
        // back to the default behavior.
        //
        // SAFETY: the safety requirement of
        // `pagemap_disabled::reset_with_pagemap` is the same as this function.
        Err(err) => unsafe {
            log::warn!("failed pagemap scan {err}");
            return crate::runtime::vm::pagemap_disabled::reset_with_pagemap(
                None,
                ptr,
                len,
                keep_resident,
                reset_manually,
                decommit,
            );
        },
    };

    // For all present regions that were written in the scan reset them
    // manually, decommit the swapped ones, then afterwards decommit everything
    // the scan did not reach.
    //
    // SAFETY: we're relying on Linux to pass in valid region ranges within
    // the `ptr/len` we specified to the original syscall.
    let bytes_resident = unsafe {
        reset_regions(
            result
                .regions()
                .iter()
                .map(|region| (region.start().cast_mut(), region.len(), region.categories())),
            &mut reset_manually,
            &mut decommit,
        )
    };

    // Report everything after `walk_end` to the end of memory as memory that
    // must be decommitted as the scan didn't reach it. Note that if `walk_end`
    // is already at the end of memory then the byte size of the decommitted
    // memory here will be 0 meaning that this is a noop.
    let scan_size = result.walk_end().addr() - ptr.addr();
    decommit(result.walk_end().cast_mut(), len.byte_count() - scan_size);

    bytes_resident
}

/// The `PAGEMAP_SCAN` filter of the reset: private pages that are present or
/// swapped and are not the zero page or a file page.
///
/// * `PFNZERO | FILE` inverted (Scriptw patch; upstream also required
///   `WRITTEN`): on Linux 6.x non-uffd VMAs reported `WRITTEN` on read-only
///   pages too, and on Linux 7.x (e.g. 7.3) non-uffd VMAs report `WRITTEN = 0`
///   even on dirty private pages, so requiring `WRITTEN` skipped every dirty
///   page and left the previous Store's bytes in the slot. For Wasmtime's
///   private anonymous and CoW file-backed mappings, a page only read maps the
///   shared zero page (`PFNZERO`) or the clean page-cache page (`FILE`), while
///   any written page is an anonymous private page (`!PFNZERO && !FILE`).
/// * `PRESENT | SWAPPED`, any of (Scriptw patch; upstream required
///   `PRESENT`): a dirty page that reclaim swapped out, or that compaction,
///   NUMA balancing or a THP collapse is migrating, is a swap entry when the
///   scan runs, and upstream skipped it, leaving a previous Store's bytes in
///   the slot. A never-populated page is neither; some kernels (6.18 among
///   them) still report it `WRITTEN`, so `PRESENT | SWAPPED` excludes holes on
///   both 6.x and 7.x kernels.
/// * Only `SWAPPED` is returned, which decides how a region is reset.
struct DirtyPageScan;

impl DirtyPageScan {
    const INVERTED: Categories = Categories::PFNZERO.union(Categories::FILE);
    const MASK: Categories = Categories::PFNZERO.union(Categories::FILE);
    const ANY_OF: Categories = Categories::PRESENT.union(Categories::SWAPPED);
    const RETURNED: Categories = Categories::SWAPPED;
}

/// Resets the regions a scan reported, given with their returned categories:
/// a present one in place, a swapped one by `decommit` (writing it would
/// first read it back from swap). Returns the bytes left resident.
///
/// # Safety
///
/// Every region must be valid to write.
unsafe fn reset_regions(
    regions: impl Iterator<Item = (*mut u8, usize, Categories)>,
    reset_manually: &mut impl FnMut(&mut [u8]),
    decommit: &mut impl FnMut(*mut u8, usize),
) -> usize {
    let mut bytes_resident = 0;
    for (start, len, categories) in regions {
        if categories.contains(Categories::SWAPPED) {
            decommit(start, len);
            continue;
        }
        // SAFETY: the caller's contract.
        unsafe {
            reset_manually(&mut *ptr::slice_from_raw_parts_mut(start, len));
        }
        bytes_resident += len;
    }
    bytes_resident
}

/// How the reset leaves one page of a slot.
#[cfg(feature = "scriptw-pagemap-model")]
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyPageReset {
    /// Not reported: never written since the last reset, so already
    /// original.
    Untouched,
    /// Zeroed in place and kept resident.
    Zeroed,
    /// Decommitted.
    Decommitted,
}

/// Runs the reset's region handling on pages whose `PAGEMAP_SCAN` category
/// bits (`PAGE_IS_*`) are `pages`, with the kernel's selection modelled
/// (`pagemap_scan_is_interesting_page`: the categories xor the inverted set
/// must contain the whole mask and meet the any-of set; matching neighbours
/// with equal returned categories form one region). This is the classification
/// a host without a swap device cannot exercise with real swapped pages.
#[cfg(feature = "scriptw-pagemap-model")]
#[doc(hidden)]
pub fn model_dirty_page_reset(pages: &[u64]) -> Vec<DirtyPageReset> {
    const PAGE: usize = 4096;
    let interesting = |bits: u64| {
        let categories = Categories::from_bits_retain(bits) ^ DirtyPageScan::INVERTED;
        categories.contains(DirtyPageScan::MASK) && categories.intersects(DirtyPageScan::ANY_OF)
    };
    let mut regions: Vec<(usize, usize, Categories)> = Vec::new();
    for (index, bits) in pages.iter().enumerate() {
        if !interesting(*bits) {
            continue;
        }
        let returned = Categories::from_bits_retain(*bits) & DirtyPageScan::RETURNED;
        match regions.last_mut() {
            Some((start, len, categories))
                if *start + *len == index * PAGE && *categories == returned =>
            {
                *len += PAGE;
            }
            _ => regions.push((index * PAGE, PAGE, returned)),
        }
    }
    let mut outcome = vec![DirtyPageReset::Untouched; pages.len()];
    let mut memory = vec![0xa5u8; pages.len() * PAGE];
    let base = memory.as_mut_ptr();
    let mut zeroed = Vec::new();
    let mut decommitted = Vec::new();
    // SAFETY: every region lies inside `memory`.
    unsafe {
        reset_regions(
            regions
                .iter()
                .map(|(start, len, categories)| (base.add(*start), *len, *categories)),
            &mut |region: &mut [u8]| {
                region.fill(0);
                zeroed.push((region.as_ptr().addr() - base.addr(), region.len()));
            },
            &mut |start: *mut u8, len: usize| decommitted.push((start.addr() - base.addr(), len)),
        );
    }
    for (list, reset) in [
        (zeroed, DirtyPageReset::Zeroed),
        (decommitted, DirtyPageReset::Decommitted),
    ] {
        for (start, len) in list {
            for page in start / PAGE..(start + len) / PAGE {
                outcome[page] = reset;
            }
        }
    }
    outcome
}

mod ioctl {
    use rustix::ioctl::*;
    use std::ffi::c_void;
    use std::fmt;
    use std::marker;
    use std::mem::MaybeUninit;
    use std::ptr;

    bitflags::bitflags! {
        /// Categories that can be filtered with [`PageMapScan`]
        #[derive(Copy, Clone, PartialEq, Eq)]
        #[repr(transparent)]
        pub struct Categories: u64 {
            /// The page has asynchronous write-protection enabled.
            const WPALLOWED = 1 << 0;
            /// The page has been written to from the time it was write protected.
            const WRITTEN = 1 << 1;
            /// The page is file backed.
            const FILE = 1 << 2;
            /// The page is present in the memory.
            const PRESENT = 1 << 3;
            /// The page is swapped.
            const SWAPPED = 1 << 4;
            /// The page has zero PFN.
            const PFNZERO = 1 << 5;
            /// The page is THP or Hugetlb backed.
            const HUGE = 1 << 6;
            // NB: I don't know what this is and I can't find documentation for
            // it, it's just included here for complete-ness with the API that
            // `PAGEMAP_SCAN` provides.
            const SOFT_DIRTY = 1 << 7;
        }
    }

    impl fmt::Debug for Categories {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            bitflags::parser::to_writer(self, f)
        }
    }

    impl fmt::Display for Categories {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            bitflags::parser::to_writer(self, f)
        }
    }

    /// Builder-style structure for building up a [`PageMapScan`] `ioctl` call.
    pub struct PageMapScanBuilder {
        pm_scan_arg: pm_scan_arg,
    }

    impl PageMapScanBuilder {
        /// Creates a new page map scan that will scan the provided range of memory.
        pub fn new(region: *const [u8]) -> PageMapScanBuilder {
            PageMapScanBuilder {
                pm_scan_arg: pm_scan_arg {
                    size: size_of::<pm_scan_arg>() as u64,
                    flags: 0,
                    start: region.cast::<u8>().addr() as u64,
                    end: region.cast::<u8>().addr().wrapping_add(region.len()) as u64,
                    walk_end: 0,
                    vec: 0,
                    vec_len: 0,
                    max_pages: 0,
                    category_inverted: Categories::empty(),
                    category_anyof_mask: Categories::empty(),
                    category_mask: Categories::empty(),
                    return_mask: Categories::empty(),
                },
            }
        }

        /// Configures the maximum number of returned pages in the output regions.
        ///
        /// Setting this to 0 disables this maximum.
        pub fn max_pages(&mut self, max: usize) -> &mut PageMapScanBuilder {
            self.pm_scan_arg.max_pages = max.try_into().unwrap();
            self
        }

        /// Configures categories which values must match if 0 instead of 1.
        ///
        /// Note that this is a mask which is xor'd to the page's true
        /// categories before testing for `category_mask`. That means that if a
        /// bit needs to be zero then it additionally must be specified in one
        /// of `category_mask` or `category_anyof_mask`.
        ///
        /// For more detail see the `pagemap_scan_is_interesting_page` function
        /// in the Linux kernel source.
        pub fn category_inverted(&mut self, flags: Categories) -> &mut PageMapScanBuilder {
            self.pm_scan_arg.category_inverted = flags;
            self
        }

        /// Only consider pages for which all `flags` match.
        ///
        /// This mask is applied after `category_inverted` is used to flip bits
        /// in a page's categories. Only pages which match all bits in `flags`
        /// will be considered.
        ///
        /// For more detail see the `pagemap_scan_is_interesting_page` function
        /// in the Linux kernel source.
        pub fn category_mask(&mut self, flags: Categories) -> &mut PageMapScanBuilder {
            self.pm_scan_arg.category_mask = flags;
            self
        }

        /// Only consider pages for which any bit of `flags` matches.
        ///
        /// After `category_inverted` and `category_mask` have been applied, if
        /// this option is specified to a non-empty value, then at least one of
        /// `flags` must be in a page's flags to be considered. That means that
        /// flags specified in `category_inverted` will already be inverted for
        /// consideration here. The page categories are and'd with `flags` and
        /// some bit must be set for the page to be considered.
        ///
        /// For more detail see the `pagemap_scan_is_interesting_page` function
        /// in the Linux kernel source.
        pub fn category_anyof_mask(&mut self, flags: Categories) -> &mut PageMapScanBuilder {
            self.pm_scan_arg.category_anyof_mask = flags;
            self
        }

        /// Categories that are to be reported in the regions returned
        pub fn return_mask(&mut self, flags: Categories) -> &mut PageMapScanBuilder {
            self.pm_scan_arg.return_mask = flags;
            self
        }

        /// Finishes this configuration and flags that the scan results will be
        /// placed within `dst`. The returned object can be used to perform the
        /// pagemap scan ioctl.
        pub fn build<'a>(&self, dst: &'a mut [MaybeUninit<PageRegion>]) -> PageMapScan<'a> {
            let mut ret = PageMapScan {
                pm_scan_arg: self.pm_scan_arg,
                _marker: marker::PhantomData,
            };
            ret.pm_scan_arg.vec = dst.as_ptr() as u64;
            ret.pm_scan_arg.vec_len = dst.len() as u64;
            return ret;
        }
    }

    /// Return result of [`PageMapScanBuilder::build`] used to perform an `ioctl`.
    #[repr(transparent)]
    pub struct PageMapScan<'a> {
        pm_scan_arg: pm_scan_arg,
        _marker: marker::PhantomData<&'a mut [MaybeUninit<PageRegion>]>,
    }

    #[derive(Copy, Clone)]
    #[repr(C)]
    struct pm_scan_arg {
        size: u64,
        flags: u64,
        start: u64,
        end: u64,
        walk_end: u64,
        vec: u64,
        vec_len: u64,
        max_pages: u64,
        category_inverted: Categories,
        category_mask: Categories,
        category_anyof_mask: Categories,
        return_mask: Categories,
    }

    /// Return result of a [`PageMapScan`] `ioctl`.
    ///
    /// This reports where the kernel stopped walking with
    /// [`PageMapScanResult::walk_end`] and the description of regions found in
    /// [`PageMapScanResult::regions`].
    #[derive(Debug)]
    pub struct PageMapScanResult<'a> {
        walk_end: *const u8,
        regions: &'a mut [PageRegion],
    }

    impl PageMapScanResult<'_> {
        /// Where the kernel stopped walking pages, which may be earlier than the
        /// end of the requested region
        pub fn walk_end(&self) -> *const u8 {
            self.walk_end
        }

        /// Regions the kernel reported back with categories and such.
        pub fn regions(&self) -> &[PageRegion] {
            self.regions
        }
    }

    /// Return value of [`PageMapScan`], description of regions in the original scan
    /// with the categories queried.
    #[repr(transparent)]
    #[derive(Copy, Clone)]
    pub struct PageRegion(page_region);

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    struct page_region {
        start: u64,
        end: u64,
        categories: Categories,
    }

    impl PageRegion {
        /// Returns the region of memory this represents as `*const [u8]`
        #[inline]
        pub fn region(&self) -> *const [u8] {
            ptr::slice_from_raw_parts(self.start(), self.len())
        }

        /// Returns the base pointer into memory this region represents.
        #[inline]
        pub fn start(&self) -> *const u8 {
            self.0.start as *const u8
        }

        /// Returns the byte length that this region represents.
        #[inline]
        pub fn len(&self) -> usize {
            usize::try_from(self.0.end - self.0.start).unwrap()
        }

        /// Returns the category flags associated with this region.
        ///
        /// Note that this will only contain categories specified in
        /// [`PageMapScanBuilder::return_mask`].
        #[inline]
        pub fn categories(&self) -> Categories {
            self.0.categories
        }
    }

    impl fmt::Debug for PageRegion {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("PageRegion")
                .field("start", &self.start())
                .field("len", &self.len())
                .field("categories", &self.0.categories)
                .finish()
        }
    }

    // SAFETY: this implementation should uphold the various requirements that
    // this trait has, such as `IS_MUTATING` is right, it's only used on the
    // right platform with the right files, etc.
    unsafe impl<'a> Ioctl for PageMapScan<'a> {
        type Output = PageMapScanResult<'a>;

        const IS_MUTATING: bool = true;

        fn opcode(&self) -> Opcode {
            opcode::read_write::<pm_scan_arg>(b'f', 16)
        }

        fn as_ptr(&mut self) -> *mut c_void {
            (&raw mut self.pm_scan_arg).cast()
        }

        unsafe fn output_from_ptr(
            out: IoctlOutput,
            extract_output: *mut c_void,
        ) -> rustix::io::Result<Self::Output> {
            let extract_output = extract_output.cast::<pm_scan_arg>();
            let len = usize::try_from(out).unwrap();
            // SAFETY: it's a requirement of this method that
            // `extract_output` is safe to read and indeed a `pm_scan_arg`.
            // Additionally the slice returned here originated from a slice
            // provided to `PageMapScanBuilder::build` threaded through the
            // `vec` field and it should be safe to thread that back out through
            // to the result.
            let regions = unsafe {
                assert!((len as u64) <= (*extract_output).vec_len);
                std::slice::from_raw_parts_mut((*extract_output).vec as *mut PageRegion, len)
            };
            Ok(PageMapScanResult {
                regions,
                // SAFETY: it's a requirement of this method that
                // `extract_output` is safe to read and indeed a `pm_scan_arg`.
                walk_end: unsafe { (*extract_output).walk_end as *const u8 },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ioctl::*;
    use crate::prelude::*;
    use rustix::ioctl::*;
    use rustix::mm::*;
    use std::fs::File;
    use std::ptr;

    struct MmapAnonymous {
        ptr: *mut std::ffi::c_void,
        len: usize,
    }

    impl MmapAnonymous {
        fn new(pages: usize) -> MmapAnonymous {
            let len = pages * rustix::param::page_size();
            let ptr = unsafe {
                mmap_anonymous(
                    ptr::null_mut(),
                    len,
                    ProtFlags::READ | ProtFlags::WRITE,
                    MapFlags::PRIVATE,
                )
                .unwrap()
            };
            MmapAnonymous { ptr, len }
        }

        fn read(&self, page: usize) {
            unsafe {
                let offset = page * rustix::param::page_size();
                assert!(offset < self.len);
                std::ptr::read_volatile(self.ptr.cast::<u8>().add(offset));
            }
        }

        fn write(&self, page: usize) {
            unsafe {
                let offset = page * rustix::param::page_size();
                assert!(offset < self.len);
                std::ptr::write_volatile(self.ptr.cast::<u8>().add(offset), 1);
            }
        }

        fn region(&self) -> *const [u8] {
            ptr::slice_from_raw_parts(self.ptr.cast(), self.len)
        }

        fn page_region(&self, pages: std::ops::Range<usize>) -> *const [u8] {
            ptr::slice_from_raw_parts(
                self.ptr
                    .cast::<u8>()
                    .wrapping_add(pages.start * rustix::param::page_size()),
                (pages.end - pages.start) * rustix::param::page_size(),
            )
        }

        fn end(&self) -> *const u8 {
            self.ptr.cast::<u8>().wrapping_add(self.len)
        }

        fn page_end(&self, page: usize) -> *const u8 {
            self.ptr
                .cast::<u8>()
                .wrapping_add((page + 1) * rustix::param::page_size())
        }
    }

    impl Drop for MmapAnonymous {
        fn drop(&mut self) {
            unsafe {
                munmap(self.ptr, self.len).unwrap();
            }
        }
    }

    fn ioctl_supported() -> bool {
        let mmap = MmapAnonymous::new(1);
        let mut results = Vec::with_capacity(1);
        let fd = File::open("/proc/self/pagemap").unwrap();
        unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(mmap.region())
                    .category_mask(Categories::WRITTEN)
                    .return_mask(Categories::all())
                    .build(results.spare_capacity_mut()),
            )
            .is_ok()
        }
    }

    #[test]
    fn no_pages_returned() {
        if !ioctl_supported() {
            return;
        }
        let mmap = MmapAnonymous::new(10);
        let mut results = Vec::with_capacity(10);
        let fd = File::open("/proc/self/pagemap").unwrap();

        let result = unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(mmap.region())
                    .category_mask(Categories::WRITTEN)
                    .return_mask(Categories::all())
                    .build(results.spare_capacity_mut()),
            )
            .unwrap()
        };
        assert!(result.regions().is_empty());
        assert_eq!(result.walk_end(), mmap.end());
    }

    #[test]
    fn empty_region() {
        if !ioctl_supported() {
            return;
        }
        let mut results = Vec::with_capacity(10);
        let fd = File::open("/proc/self/pagemap").unwrap();

        let empty_region = ptr::slice_from_raw_parts(rustix::param::page_size() as *const u8, 0);
        let result = unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(empty_region)
                    .return_mask(Categories::all())
                    .build(results.spare_capacity_mut()),
            )
            .unwrap()
        };
        assert!(result.regions().is_empty());
    }

    #[test]
    fn basic_page_flags() {
        if !ioctl_supported() {
            return;
        }
        let mmap = MmapAnonymous::new(10);
        let mut results = Vec::with_capacity(10);
        let fd = File::open("/proc/self/pagemap").unwrap();

        mmap.read(0);
        mmap.write(1);
        mmap.write(2);
        mmap.read(3);

        mmap.read(5);
        mmap.read(6);

        let result = unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(mmap.region())
                    .category_mask(Categories::WRITTEN)
                    .return_mask(Categories::WRITTEN | Categories::PRESENT | Categories::PFNZERO)
                    .build(results.spare_capacity_mut()),
            )
            .unwrap()
        };
        assert_eq!(result.regions().len(), 4);
        assert_eq!(result.walk_end(), mmap.end());
        assert_eq!(result.regions()[0].region(), mmap.page_region(0..1));
        assert_eq!(
            result.regions()[0].categories(),
            Categories::WRITTEN | Categories::PRESENT | Categories::PFNZERO
        );

        assert_eq!(result.regions()[1].region(), mmap.page_region(1..3));
        assert_eq!(
            result.regions()[1].categories(),
            Categories::WRITTEN | Categories::PRESENT
        );

        assert_eq!(result.regions()[2].region(), mmap.page_region(3..4));
        assert_eq!(
            result.regions()[2].categories(),
            Categories::WRITTEN | Categories::PRESENT | Categories::PFNZERO
        );

        assert_eq!(result.regions()[3].region(), mmap.page_region(5..7));
        assert_eq!(
            result.regions()[3].categories(),
            Categories::WRITTEN | Categories::PRESENT | Categories::PFNZERO
        );
    }

    #[test]
    fn only_written_pages() {
        if !ioctl_supported() {
            return;
        }
        let mmap = MmapAnonymous::new(10);
        let mut results = Vec::with_capacity(10);
        let fd = File::open("/proc/self/pagemap").unwrap();

        mmap.read(0);
        mmap.write(1);
        mmap.write(2);
        mmap.read(3);

        mmap.read(5);
        mmap.read(6);

        let result = unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(mmap.region())
                    .category_inverted(Categories::PFNZERO)
                    .category_mask(Categories::WRITTEN | Categories::PFNZERO)
                    .return_mask(Categories::WRITTEN | Categories::PRESENT | Categories::PFNZERO)
                    .build(results.spare_capacity_mut()),
            )
            .unwrap()
        };
        assert_eq!(result.regions().len(), 1);
        assert_eq!(result.walk_end(), mmap.end());

        assert_eq!(result.regions()[0].region(), mmap.page_region(1..3));
        assert_eq!(
            result.regions()[0].categories(),
            Categories::WRITTEN | Categories::PRESENT
        );
    }

    #[test]
    fn region_limit() {
        if !ioctl_supported() {
            return;
        }
        let mmap = MmapAnonymous::new(10);
        let mut results = Vec::with_capacity(1);
        let fd = File::open("/proc/self/pagemap").unwrap();

        mmap.read(0);
        mmap.write(1);
        mmap.read(2);
        mmap.write(3);

        // Ask for written|pfnzero meaning only-read pages. This should return only
        // a single region of the first page.
        let result = unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(mmap.region())
                    .return_mask(Categories::WRITTEN | Categories::PFNZERO)
                    .build(results.spare_capacity_mut()),
            )
            .unwrap()
        };
        assert_eq!(result.regions().len(), 1);
        assert_eq!(result.walk_end(), mmap.page_end(0));

        assert_eq!(result.regions()[0].region(), mmap.page_region(0..1));
        assert_eq!(
            result.regions()[0].categories(),
            Categories::WRITTEN | Categories::PFNZERO
        );

        // If we ask for written pages though (which seems synonymous with
        // present?) then everything should be in one region.
        let result = unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(mmap.region())
                    .return_mask(Categories::WRITTEN)
                    .build(results.spare_capacity_mut()),
            )
            .unwrap()
        };
        assert_eq!(result.regions().len(), 1);
        assert_eq!(result.walk_end(), mmap.page_end(3));

        assert_eq!(result.regions()[0].region(), mmap.page_region(0..4));
        assert_eq!(result.regions()[0].categories(), Categories::WRITTEN);
    }

    #[test]
    fn page_limit() {
        if !ioctl_supported() {
            return;
        }
        let mmap = MmapAnonymous::new(10);
        let mut results = Vec::with_capacity(10);
        let fd = File::open("/proc/self/pagemap").unwrap();

        mmap.read(0);
        mmap.read(1);
        mmap.read(2);
        mmap.read(3);

        // Ask for written|pfnzero meaning only-read pages. This should return only
        // a single region of the first page.
        let result = unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(mmap.region())
                    .return_mask(Categories::WRITTEN | Categories::PFNZERO)
                    .max_pages(2)
                    .build(results.spare_capacity_mut()),
            )
            .unwrap()
        };
        assert_eq!(result.regions().len(), 1);
        assert_eq!(result.walk_end(), mmap.page_end(1));

        assert_eq!(result.regions()[0].region(), mmap.page_region(0..2));
        assert_eq!(
            result.regions()[0].categories(),
            Categories::WRITTEN | Categories::PFNZERO
        );
    }

    #[test]
    fn page_limit_with_hole() {
        if !ioctl_supported() {
            return;
        }
        let mmap = MmapAnonymous::new(10);
        let mut results = Vec::with_capacity(10);
        let fd = File::open("/proc/self/pagemap").unwrap();

        mmap.read(0);
        mmap.read(2);
        mmap.read(3);

        // Ask for written|pfnzero meaning only-read pages. This should return only
        // a single region of the first page.
        let result = unsafe {
            ioctl(
                &fd,
                PageMapScanBuilder::new(mmap.region())
                    .category_mask(Categories::WRITTEN)
                    .return_mask(Categories::WRITTEN | Categories::PFNZERO)
                    .max_pages(2)
                    .build(results.spare_capacity_mut()),
            )
            .unwrap()
        };
        assert_eq!(result.regions().len(), 2);
        assert_eq!(result.walk_end(), mmap.page_end(2));

        assert_eq!(result.regions()[0].region(), mmap.page_region(0..1));
        assert_eq!(
            result.regions()[0].categories(),
            Categories::WRITTEN | Categories::PFNZERO
        );
        assert_eq!(result.regions()[1].region(), mmap.page_region(2..3));
        assert_eq!(
            result.regions()[1].categories(),
            Categories::WRITTEN | Categories::PFNZERO
        );
    }
}
