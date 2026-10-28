//! Explicit diagnostic configuration; ordinary benchmark invocations remain
//! automatic. Closed SDK types reject unbounded heap sizes and unknown modes.
use scriptw_engine::{
    WasmProfileGcInitialCapacity, WasmProfileNativeMode, WasmProfileRegisterAllocation,
    WasmRuntimeProfileOptions,
};
use serde_json::{json, Value};

pub(super) struct InvocationOptions {
    pub(super) row_phases: bool,
    pub(super) policy: Option<WasmRuntimeProfileOptions>,
}

pub(super) fn parse(args: &[String]) -> Result<InvocationOptions, String> {
    let mut row_phases = false;
    let mut native = None;
    let mut capacity = None;
    let mut allocation = None;
    let mut flags = args.iter();
    while let Some(flag) = flags.next() {
        if flag == "--row-phases" {
            if row_phases {
                return Err("duplicate --row-phases".into());
            }
            row_phases = true;
            continue;
        }
        let value = flags
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--native-mode" if native.is_none() => {
                native = Some(match value.as_str() {
                    "fast" => WasmProfileNativeMode::Fast,
                    "size-optimized" => WasmProfileNativeMode::SizeOptimized,
                    "speed" => WasmProfileNativeMode::Speed,
                    _ => return Err("native mode must be fast, size-optimized or speed".into()),
                });
            }
            "--gc-initial-mib" if capacity.is_none() => {
                capacity = Some(match value.as_str() {
                    "8" => WasmProfileGcInitialCapacity::MiB8,
                    "16" => WasmProfileGcInitialCapacity::MiB16,
                    "32" => WasmProfileGcInitialCapacity::MiB32,
                    "64" => WasmProfileGcInitialCapacity::MiB64,
                    "128" => WasmProfileGcInitialCapacity::MiB128,
                    _ => return Err("GC initial capacity must be 8, 16, 32, 64 or 128 MiB".into()),
                });
            }
            "--regalloc" if allocation.is_none() => {
                allocation = Some(match value.as_str() {
                    "single-pass" => WasmProfileRegisterAllocation::SinglePass,
                    "backtracking" => WasmProfileRegisterAllocation::Backtracking,
                    _ => {
                        return Err("register allocation must be single-pass or backtracking".into())
                    }
                });
            }
            _ => return Err(format!("unknown or duplicate option: {flag}")),
        }
    }
    let policy = (native.is_some() || capacity.is_some() || allocation.is_some()).then(|| {
        WasmRuntimeProfileOptions::new(
            native.unwrap_or(WasmProfileNativeMode::Fast),
            capacity.unwrap_or(WasmProfileGcInitialCapacity::MiB16),
            allocation.unwrap_or(WasmProfileRegisterAllocation::SinglePass),
        )
    });
    Ok(InvocationOptions { row_phases, policy })
}

pub(super) fn policy_json(options: WasmRuntimeProfileOptions) -> Value {
    json!({
        "native_mode": options.native_mode().as_str(),
        "gc_initial_mib": options.initial_gc_capacity().mebibytes(),
        "register_allocation": options.register_allocation().as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn automatic_and_explicit_default_cohorts_remain_distinct() {
        assert!(parse(&[]).unwrap().policy.is_none());
        let explicit = parse(&strings(&["--gc-initial-mib", "16"]))
            .unwrap()
            .policy
            .unwrap();
        assert_eq!(
            policy_json(explicit),
            json!({"native_mode":"fast", "gc_initial_mib":16, "register_allocation":"single-pass"})
        );
    }

    #[test]
    fn combined_options_retain_each_independent_experiment() {
        for (capacity, expected_mib) in [("8", 8), ("64", 64)] {
            let parsed = parse(&strings(&[
                "--native-mode",
                "speed",
                "--row-phases",
                "--gc-initial-mib",
                capacity,
                "--regalloc",
                "backtracking",
            ]))
            .unwrap();
            assert!(parsed.row_phases);
            assert_eq!(
                policy_json(parsed.policy.unwrap()),
                json!({"native_mode":"speed", "gc_initial_mib":expected_mib, "register_allocation":"backtracking"})
            );
        }
    }

    #[test]
    fn unknown_unbounded_missing_and_duplicate_options_reject() {
        for args in [
            vec!["--native-mode", "other"],
            vec!["--gc-initial-mib", "0"],
            vec!["--gc-initial-mib", "4096"],
            vec!["--regalloc", "other"],
            vec!["--regalloc"],
            vec!["--gc-initial-mib", "16", "--gc-initial-mib", "32"],
            vec!["--row-phases", "--row-phases"],
            vec!["--unknown", "value"],
        ] {
            assert!(parse(&strings(&args)).is_err(), "{args:?}");
        }
    }
}
