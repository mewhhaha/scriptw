//! Separate function/global index domains prevent accidental map substitution.

pub(super) trait Index: Copy {
    fn new(raw: u32) -> Self;
    fn raw(self) -> u32;
}
macro_rules! index {
    ($name:ident) => {
        #[derive(Clone, Copy)]
        pub(super) struct $name(pub(super) u32);
        impl Index for $name {
            fn new(raw: u32) -> Self {
                Self(raw)
            }
            fn raw(self) -> u32 {
                self.0
            }
        }
    };
}
index!(FunctionIndex);
index!(GlobalIndex);

pub(super) struct IndexMap<I> {
    imports: Vec<Option<I>>,
    total: u32,
    removed: u32,
}
impl<I: Index> IndexMap<I> {
    pub(super) fn new(keep: &[bool], defined: u32) -> Result<Self, String> {
        let imported = u32::try_from(keep.len()).map_err(|_| "import count exceeds u32")?;
        let total = imported
            .checked_add(defined)
            .ok_or("index space exceeds u32")?;
        let mut next = 0;
        let imports = keep
            .iter()
            .map(|keep| {
                if *keep {
                    let index = I::new(next);
                    next += 1;
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        Ok(Self {
            imports,
            total,
            removed: imported - next,
        })
    }
    /// None means a known removed import; out-of-range indices remain errors.
    pub(super) fn get(&self, index: I) -> Result<Option<I>, String> {
        let raw = index.raw();
        if raw >= self.total {
            return Err(format!(
                "index {raw} exceeds original index space {}",
                self.total
            ));
        }
        if let Some(value) = self.imports.get(raw as usize) {
            return Ok(*value);
        }
        Ok(Some(I::new(raw - self.removed)))
    }
    pub(super) fn require(&self, index: I) -> Result<I, String> {
        self.get(index)?.ok_or_else(|| {
            format!(
                "removed import {} remains semantically referenced",
                index.raw()
            )
        })
    }
    pub(super) fn removed(&self) -> u32 {
        self.removed
    }
}
