//! Process-level runtime and program cache tests, and the other process-wide
//! settings (the compilation-pool default) that only a fresh process can show.
//!
//! These tests re-execute this very test binary as fresh child processes with a
//! private `SCRIPTW_CACHE_DIR` and select the child with `--exact <module>::<test>`,
//! so they stay in their own small target: no other area's tests share the child
//! filter or the copied executable.

mod aot_date_cached_realm_choice;
mod compilation_jobs_environment;
mod runtime_artifact_cache;
