use scriptw_ir::CompletionKindIr;

pub(crate) const COMPLETION_KIND_NORMAL: i64 = CompletionKindIr::Normal.abi_code();
pub(crate) const COMPLETION_KIND_THROW: i64 = CompletionKindIr::Throw.abi_code();
pub(crate) const COMPLETION_KIND_RETURN: i64 = CompletionKindIr::Return.abi_code();
pub(crate) const COMPLETION_KIND_BREAK: i64 = CompletionKindIr::Break.abi_code();
pub(crate) const COMPLETION_KIND_CONTINUE: i64 = CompletionKindIr::Continue.abi_code();
