use super::*;
use crate::gc_types::ProgramLiteralIndex;

/// The checked compile-time owner of a collected literal. A Program index is
/// already relative to that pool's runtime boundary; it is not a global slot.
#[derive(Debug)]
pub(crate) enum PooledLiteralDomain {
    Runtime(PooledStringIndex),
    Program(ProgramLiteralIndex),
}

impl StringPool {
    pub(crate) fn pooled_string_domain(
        &self,
        index: PooledStringIndex,
    ) -> Result<PooledLiteralDomain, EmitError> {
        let count = self.pooled_string_count();
        let runtime = self.compiler_owned_boundary().strings();
        if runtime > count || index.ordinal() >= count {
            return Err(EmitError::unsupported(
                "pooled literal is outside its collected source pool",
            ));
        }
        if index.ordinal() < runtime {
            Ok(PooledLiteralDomain::Runtime(index))
        } else {
            ProgramLiteralIndex::for_pool(self, index).map(PooledLiteralDomain::Program)
        }
    }

    /// R's complete stable prefix. A P literal cannot extend the shared table.
    #[cfg(test)]
    pub(crate) fn runtime_pooled_string_initializers(
        &self,
    ) -> impl Iterator<Item = (PooledStringIndex, &PooledCodeUnits)> {
        let end = self.compiler_owned_boundary().strings();
        self.refs
            .values()
            .filter(move |reference| reference.index.ordinal() < end)
            .map(|reference| (reference.index, &reference.code_units))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_runtime_prefix() -> StringPool {
        let mut pool = StringPool::default();
        pool.intern_string("runtime");
        pool.intern_string("runtime name");
        pool.compiler_owned = pool.boundary();
        pool
    }

    #[test]
    fn literal_domain_retains_runtime_dedup_and_uses_program_local_indexes() {
        let mut pool = with_runtime_prefix();
        pool.intern_string("program z");
        pool.intern_string("!program first in sorted map");
        pool.intern_string("runtime");
        let runtime = pool.pooled_string_index("runtime").unwrap();
        assert!(matches!(
            pool.pooled_string_domain(runtime).unwrap(),
            PooledLiteralDomain::Runtime(index) if index.ordinal() == runtime.ordinal()
        ));
        for (source, ordinal) in [("program z", 0), ("!program first in sorted map", 1)] {
            let index = pool.pooled_string_index(source).unwrap();
            assert!(matches!(
                pool.pooled_string_domain(index).unwrap(),
                PooledLiteralDomain::Program(index) if index.ordinal() == ordinal
            ));
        }
        assert!(ProgramLiteralIndex::for_pool(&pool, runtime).is_err());
        assert_eq!(pool.runtime_pooled_string_initializers().count(), 2);
    }

    #[test]
    fn empty_program_and_uncollected_indexes_have_no_program_slot() {
        let pool = with_runtime_prefix();
        let absent = PooledStringIndex::for_insertion(pool.refs.len());
        assert!(pool.pooled_string_domain(absent).is_err());
        assert!(ProgramLiteralIndex::for_pool(&pool, absent).is_err());
        let far = PooledStringIndex::for_insertion(u32::MAX as usize - 1);
        assert!(pool.pooled_string_domain(far).is_err());
        assert!(ProgramLiteralIndex::for_pool(&pool, far).is_err());
        assert_eq!(pool.program_pooled_string_initializers().count(), 0);
    }

    #[test]
    fn runtime_initializers_do_not_change_when_a_program_adds_literals() {
        let mut pool = with_runtime_prefix();
        let before = pool
            .runtime_pooled_string_initializers()
            .map(|(index, literal)| (index.ordinal(), literal.byte_offset, literal.length))
            .collect::<Vec<_>>();
        pool.intern_string("\0program");
        pool.intern_string("\u{10000}program");
        let after = pool
            .runtime_pooled_string_initializers()
            .map(|(index, literal)| (index.ordinal(), literal.byte_offset, literal.length))
            .collect::<Vec<_>>();
        assert_eq!(before, after);
        assert_eq!(before.len(), 2);
        assert_eq!(pool.pooled_string_count(), 4);
    }
}
