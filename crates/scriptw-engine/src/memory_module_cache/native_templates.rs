use super::*;
#[cfg(feature = "native-source-compiler")]
mod instances;
#[cfg(feature = "native-source-compiler")]
pub(crate) use instances::MemoryNativeInstanceCache;

impl MemoryWasmModuleCache {
    /// Ordinary modules and native templates share this Engine-bound cache.
    /// A matching byte key never authorizes a Module from another Engine.
    pub(crate) fn get_for_engine(
        &mut self,
        key: &WasmModuleMemoryCacheKey,
        engine: &crate::WasmtimeEngine,
    ) -> Option<WasmtimeModule> {
        let entry = self.entries.iter().find(|entry| entry.key == *key)?;
        if !crate::WasmtimeEngine::same(entry.module.engine(), engine) {
            return None;
        }
        self.get(key)
    }

    pub(crate) fn retain_compiled_for_engine(
        &mut self,
        key: WasmModuleMemoryCacheKey,
        module: WasmtimeModule,
    ) -> WasmtimeModule {
        if let Some(index) = self.entries.iter().position(|entry| {
            entry.key == key && !crate::WasmtimeEngine::same(entry.module.engine(), module.engine())
        }) {
            let removed = self
                .entries
                .remove(index)
                .expect("foreign cache entry exists");
            self.retained_image_bytes = self
                .retained_image_bytes
                .checked_sub(removed.image_bytes)
                .expect("foreign image belongs to aggregate");
        }
        self.retain_compiled(key, module)
    }
}

#[cfg(test)]
mod tests;
