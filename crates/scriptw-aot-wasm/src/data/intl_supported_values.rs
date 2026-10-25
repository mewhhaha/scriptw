//! The native catalogue owner retains checked names for rooted GC publication.
use super::*;
use scriptw_intl::{
    IntlCompilationData, IntlDataSelection, SupportedValuesKey, SupportedValuesList,
    SupportedValuesSetupError,
};
use std::sync::Arc;

#[derive(Debug)]
pub(crate) enum CompiledSupportedValuesTable {
    Available(Arc<SupportedValuesList>),
    Unavailable,
}
impl CompiledSupportedValuesTable {
    pub(crate) fn source(&self) -> Option<Arc<SupportedValuesList>> {
        match self {
            Self::Available(source) => Some(Arc::clone(source)),
            Self::Unavailable => None,
        }
    }
}
#[derive(Debug)]
pub(super) struct CompiledSupportedValuesTables {
    tables: [CompiledSupportedValuesTable; 6],
}
#[derive(Debug)]
pub(super) enum SupportedValuesPoolError {
    Setup(String),
    Selection(String),
    StaticExtent,
    ProviderIdentityMismatch,
}
impl std::fmt::Display for SupportedValuesPoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Setup(error) | Self::Selection(error) => write!(f, "{error}"),
            Self::StaticExtent => f.write_str("supported-value static table exceeds Wasm32"),
            Self::ProviderIdentityMismatch => {
                f.write_str("supported-value catalogue does not belong to the selected provider")
            }
        }
    }
}
impl CompiledSupportedValuesTables {
    fn table(&self, key: SupportedValuesKey) -> &CompiledSupportedValuesTable {
        &self.tables[match key {
            SupportedValuesKey::Calendar => 0,
            SupportedValuesKey::Collation => 1,
            SupportedValuesKey::Currency => 2,
            SupportedValuesKey::NumberingSystem => 3,
            SupportedValuesKey::TimeZone => 4,
            SupportedValuesKey::Unit => 5,
        }]
    }
    fn load(
        pool: &mut StringPool,
        selection: &IntlDataSelection,
    ) -> Result<Self, SupportedValuesPoolError> {
        let selected = selection
            .compilation_data()
            .map_err(|error| SupportedValuesPoolError::Selection(error.to_string()))?;
        let identity = selected.identity().artifact_identity();
        fn table(
            pool: &mut StringPool,
            key: SupportedValuesKey,
            selected: &IntlCompilationData,
            identity: &[u8],
        ) -> Result<CompiledSupportedValuesTable, SupportedValuesPoolError> {
            let source = match selected.supported_values(key) {
                Ok(source) => source,
                Err(SupportedValuesSetupError::UnavailableData(missing)) if missing == key => {
                    return Ok(CompiledSupportedValuesTable::Unavailable)
                }
                Err(error) => return Err(SupportedValuesPoolError::Setup(error.to_string())),
            };
            if source.provider_identity().artifact_identity().as_bytes() != identity {
                return Err(SupportedValuesPoolError::ProviderIdentityMismatch);
            }
            for value in source.values() {
                pool.intern_string(value);
            }
            u32::try_from(source.values().len())
                .map_err(|_| SupportedValuesPoolError::StaticExtent)?;
            Ok(CompiledSupportedValuesTable::Available(source))
        }
        let tables = [
            table(
                pool,
                SupportedValuesKey::Calendar,
                selected,
                identity.as_bytes(),
            )?,
            table(
                pool,
                SupportedValuesKey::Collation,
                selected,
                identity.as_bytes(),
            )?,
            table(
                pool,
                SupportedValuesKey::Currency,
                selected,
                identity.as_bytes(),
            )?,
            table(
                pool,
                SupportedValuesKey::NumberingSystem,
                selected,
                identity.as_bytes(),
            )?,
            table(
                pool,
                SupportedValuesKey::TimeZone,
                selected,
                identity.as_bytes(),
            )?,
            table(
                pool,
                SupportedValuesKey::Unit,
                selected,
                identity.as_bytes(),
            )?,
        ];
        Ok(Self { tables })
    }
}
impl StringPool {
    pub(super) fn collect_intl_supported_values(&mut self, selection: &IntlDataSelection) {
        for key in SupportedValuesKey::ALL {
            self.intern_string(key.as_str());
        }
        self.intern_string("supportedValuesOf");
        self.intern_string("Invalid Intl.supportedValuesOf key");
        self.intern_string(
            "Intl.supportedValuesOf data is unavailable in the selected Custom profile",
        );
        self.intl_supported_values = Some(CompiledSupportedValuesTables::load(self, selection));
    }
    fn checked_intl_supported_values(&self) -> Result<&CompiledSupportedValuesTables, EmitError> {
        self.intl_supported_values
            .as_ref()
            .or_else(|| {
                self.runtime_prefix()
                    .and_then(|prefix| prefix.intl_supported_values.as_ref())
            })
            .ok_or_else(|| EmitError::unsupported("missing rooted supported-value catalogues"))?
            .as_ref()
            .map_err(|error| {
                EmitError::unsupported(format!(
                    "failed to construct supported-value catalogues: {error}"
                ))
            })
    }
    pub(crate) fn intl_supported_values_table(
        &self,
        key: SupportedValuesKey,
    ) -> Result<&CompiledSupportedValuesTable, EmitError> {
        Ok(self.checked_intl_supported_values()?.table(key))
    }
    pub(crate) fn check_intl_supported_values(&self) -> Result<bool, EmitError> {
        if self.intl_supported_values.is_none()
            && self
                .runtime_prefix()
                .map_or(true, |prefix| prefix.intl_supported_values.is_none())
        {
            return Ok(false);
        }
        self.checked_intl_supported_values()?;
        Ok(true)
    }
}
