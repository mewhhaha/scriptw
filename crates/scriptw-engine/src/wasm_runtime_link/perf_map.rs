//! Opt-in Linux perf map for a serial, single-program profiling child.
//! No Modules are retained. Changed mappings replace the snapshot; multi-program
//! or concurrent profiles must not assume the final map describes earlier code.
//! Use `/tmp` for perf's automatic lookup. Per-Store debug_guest code copies are
//! outside this diagnostic's shared-code mapping contract.
use super::{WasmModules, WasmProgramRef};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

#[derive(PartialEq, Eq)]
struct Identity {
    path: PathBuf,
    modules: Vec<(usize, usize, [u8; 32])>,
}

#[derive(Default)]
struct State {
    last: Option<Identity>,
    failed: bool,
}

pub(super) fn record(modules: &WasmModules, source: WasmProgramRef<'_>) {
    let Some(directory) = std::env::var_os("SCRIPTW_PERF_MAP_DIR") else {
        return;
    };
    let path = PathBuf::from(directory).join(format!("perf-{}.map", std::process::id()));
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    let mut state = STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if state.failed {
        return;
    }
    let mut inputs = Vec::with_capacity(2);
    if let (Some(module), Some(runtime)) = (&modules.runtime, source.runtime) {
        inputs.push(("R", module, runtime.bytes(), runtime.wasm_sha256()));
    }
    inputs.push((
        "P",
        &modules.program,
        source.bytes,
        Sha256::digest(source.bytes).into(),
    ));
    let identity = Identity {
        path,
        modules: inputs
            .iter()
            .map(|(_, m, _, hash)| (m.text().as_ptr() as usize, m.text().len(), *hash))
            .collect(),
    };
    if state.last.as_ref() == Some(&identity) {
        return;
    }
    let result = (|| -> std::io::Result<()> {
        let mut map = Vec::new();
        let mut receipt = Vec::new();
        writeln!(
            receipt,
            "scriptw-perf-map-v1 pid={} boundary=serial-single-program-snapshot",
            std::process::id()
        )?;
        for (label, module, wasm, hash) in inputs {
            write_module(&mut map, label, module, wasm)?;
            writeln!(
                receipt,
                "{label} wasm_sha256={} native_text_sha256={} base={:x} bytes={}",
                hex(hash),
                hex(Sha256::digest(module.text()).into()),
                module.text().as_ptr() as usize,
                module.text().len()
            )?;
        }
        writeln!(receipt, "map_sha256={}", hex(Sha256::digest(&map).into()))?;
        let temporary = identity.path.with_extension("map.tmp");
        std::fs::write(&temporary, &map)?;
        std::fs::rename(temporary, &identity.path)?;
        std::fs::write(identity.path.with_extension("map.identity"), receipt)?;
        Ok(())
    })();
    match result {
        Ok(()) => state.last = Some(identity),
        Err(error) => {
            state.failed = true;
            eprintln!("scriptw perf map unavailable: {error}");
        }
    }
}

fn hex(hash: [u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_module(
    output: &mut impl Write,
    label: &str,
    module: &wasmtime::Module,
    wasm: &[u8],
) -> std::io::Result<()> {
    let mut names = BTreeMap::new();
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        if let Ok(wasmparser::Payload::ExportSection(exports)) = payload {
            for export in exports {
                let export = export.map_err(std::io::Error::other)?;
                if export.kind == wasmparser::ExternalKind::Func {
                    names.entry(export.index).or_insert(export.name);
                }
            }
        }
    }
    let base = module.text().as_ptr() as usize;
    for function in module.functions() {
        let index = function.index.as_u32();
        let name = function
            .name
            .as_deref()
            .or_else(|| names.get(&index).copied())
            .unwrap_or("unnamed");
        let name: String = name
            .chars()
            .take(512)
            .map(|c| if c.is_ascii_graphic() { c } else { '_' })
            .collect();
        writeln!(
            output,
            "{:x} {:x} scriptw:{label}:f{index}:{name}",
            base + function.offset,
            function.len
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_map_uses_actual_loaded_text_and_export_function_indices() {
        // (module (func (export "a b") (result i32) i32.const 7))
        let wasm = [
            0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 7, 7, 1, 3, 97, 32,
            98, 0, 0, 10, 6, 1, 4, 0, 65, 7, 11,
        ];
        let engine = wasmtime::Engine::new(&crate::wasmtime_config::base_config(
            crate::wasmtime_config::WasmNativeCompilationMode::Fast,
            wasmtime::WasmBacktraceDetails::Disable,
        ))
        .unwrap();
        let module = wasmtime::Module::new(&engine, wasm).unwrap();
        let function = module.functions().next().unwrap();
        let mut output = Vec::new();
        write_module(&mut output, "P", &module, &wasm).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!(
                "{:x} {:x} scriptw:P:f0:a_b\n",
                module.text().as_ptr() as usize + function.offset,
                function.len
            )
        );
    }
}
