//! Explicit, normalized capabilities for foreign host modules.
//!
//! These declarations authorize emitted Wasm imports. They are independent of
//! the compiler-owned JavaScript builtin catalog and contain no host closures.
use std::collections::BTreeSet;

use crate::TypedExpr;

pub const FOREIGN_ABI_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignDeclarationError(String);

impl ForeignDeclarationError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl std::fmt::Display for ForeignDeclarationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ForeignDeclarationError {}

/// Strict boundary types. Strings preserve every UTF-16 code unit; bytes are
/// copied Uint8Array contents. Marshalling never invokes user coercion hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignValueType {
    Number,
    StringUtf16,
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignResultType {
    Number,
    StringUtf16,
    Bytes,
    Void,
}

/// Async imports receive two GC-rooted resolving functions and return no Wasm
/// result. Their declared result describes the eventual Promise fulfillment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignFunctionMode {
    Synchronous,
    Asynchronous,
}

/// The explicitly declared numerical `error_v1` tag. Its payload is one i32.
/// Absence of this explicit declaration leaves ordinary host failures as traps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForeignErrorTag(());
impl ForeignErrorTag {
    pub const fn code_v1() -> Self {
        Self(())
    }
    pub const fn module(&self) -> &'static str {
        "scriptw_foreign"
    }
    pub const fn name(&self) -> &'static str {
        "error_v1"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignFunctionDeclaration {
    js_export: String,
    import_module: String,
    import_name: String,
    parameters: Vec<ForeignValueType>,
    result: ForeignResultType,
    mode: ForeignFunctionMode,
}

fn nonempty(value: &str, role: &str) -> Result<(), ForeignDeclarationError> {
    if value.is_empty() || value.contains('\0') {
        return Err(ForeignDeclarationError::new(format!(
            "foreign {role} must be nonempty and contain no NUL"
        )));
    }
    Ok(())
}

impl ForeignFunctionDeclaration {
    pub fn new(
        js_export: impl Into<String>,
        import_module: impl Into<String>,
        import_name: impl Into<String>,
        parameters: Vec<ForeignValueType>,
        result: ForeignResultType,
    ) -> Result<Self, ForeignDeclarationError> {
        Self::with_mode(
            js_export,
            import_module,
            import_name,
            parameters,
            result,
            ForeignFunctionMode::Synchronous,
        )
    }

    pub fn new_async(
        js_export: impl Into<String>,
        import_module: impl Into<String>,
        import_name: impl Into<String>,
        parameters: Vec<ForeignValueType>,
        result: ForeignResultType,
    ) -> Result<Self, ForeignDeclarationError> {
        Self::with_mode(
            js_export,
            import_module,
            import_name,
            parameters,
            result,
            ForeignFunctionMode::Asynchronous,
        )
    }

    fn with_mode(
        js_export: impl Into<String>,
        import_module: impl Into<String>,
        import_name: impl Into<String>,
        parameters: Vec<ForeignValueType>,
        result: ForeignResultType,
        mode: ForeignFunctionMode,
    ) -> Result<Self, ForeignDeclarationError> {
        let js_export = js_export.into();
        let import_module = import_module.into();
        let import_name = import_name.into();
        nonempty(&js_export, "export name")?;
        nonempty(&import_module, "Wasm import module")?;
        nonempty(&import_name, "Wasm import name")?;
        if matches!(
            import_module.as_str(),
            "scriptw_runtime" | "scriptw_foreign" | "scriptw_host"
        ) {
            return Err(ForeignDeclarationError::new(format!(
                "foreign import namespace {import_module:?} is reserved by Scriptw"
            )));
        }
        u32::try_from(parameters.len())
            .ok()
            .and_then(|count| {
                count.checked_add(match mode {
                    ForeignFunctionMode::Synchronous => 0,
                    ForeignFunctionMode::Asynchronous => 2,
                })
            })
            .ok_or_else(|| {
                ForeignDeclarationError::new(
                    "foreign parameter count exceeds the Wasm index domain",
                )
            })?;
        Ok(Self {
            js_export,
            import_module,
            import_name,
            parameters,
            result,
            mode,
        })
    }
    pub fn js_export(&self) -> &str {
        &self.js_export
    }
    pub fn import_module(&self) -> &str {
        &self.import_module
    }
    pub fn import_name(&self) -> &str {
        &self.import_name
    }
    pub fn parameters(&self) -> &[ForeignValueType] {
        &self.parameters
    }
    pub const fn result(&self) -> ForeignResultType {
        self.result
    }
    pub const fn mode(&self) -> ForeignFunctionMode {
        self.mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignModuleDeclaration {
    specifier: String,
    functions: Vec<ForeignFunctionDeclaration>,
}
impl ForeignModuleDeclaration {
    pub fn new(
        specifier: impl Into<String>,
        mut functions: Vec<ForeignFunctionDeclaration>,
    ) -> Result<Self, ForeignDeclarationError> {
        let specifier = specifier.into();
        nonempty(&specifier, "module specifier")?;
        if functions.is_empty() {
            return Err(ForeignDeclarationError::new(
                "a foreign module must declare a function",
            ));
        }
        functions.sort_by(|a, b| a.js_export.cmp(&b.js_export));
        if functions
            .windows(2)
            .any(|pair| pair[0].js_export == pair[1].js_export)
        {
            return Err(ForeignDeclarationError::new(format!(
                "duplicate foreign export in {specifier:?}"
            )));
        }
        let mut imports = BTreeSet::new();
        if functions
            .iter()
            .any(|function| !imports.insert((function.import_module(), function.import_name())))
        {
            return Err(ForeignDeclarationError::new(
                "duplicate or conflicting foreign Wasm import",
            ));
        }
        drop(imports);
        Ok(Self {
            specifier,
            functions,
        })
    }
    pub fn specifier(&self) -> &str {
        &self.specifier
    }
    pub fn functions(&self) -> &[ForeignFunctionDeclaration] {
        &self.functions
    }

    /// Compiler-owned scaffold. Its return-array pointers are replaced with
    /// ForeignCall IR from retained provenance; no user-spellable intrinsic
    /// or ambient global authorizes a foreign call.
    pub(crate) fn wrapper_source(&self) -> String {
        let mut source = String::new();
        for (index, function) in self.functions.iter().enumerate() {
            let name = format!("__scriptw_foreign_{index}");
            let parameters = (0..function.parameters.len())
                .map(|i| format!("p{i}"))
                .collect::<Vec<_>>()
                .join(",");
            source.push_str(&format!("const {name}=function({parameters}){{return [{parameters}];}};\nexport {{{name} as {}}};\n", serde_json::to_string(function.js_export()).expect("Rust string serializes")));
        }
        source
    }
}

/// A function's rank in the normalized registry, distinct from every Wasm index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForeignFunctionId(u32);
impl ForeignFunctionId {
    pub const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForeignModuleRegistry {
    modules: Vec<ForeignModuleDeclaration>,
    error_tag: Option<ForeignErrorTag>,
}
impl ForeignModuleRegistry {
    pub fn new(
        mut modules: Vec<ForeignModuleDeclaration>,
    ) -> Result<Self, ForeignDeclarationError> {
        modules.sort_by(|a, b| a.specifier.cmp(&b.specifier));
        if modules
            .windows(2)
            .any(|pair| pair[0].specifier == pair[1].specifier)
        {
            return Err(ForeignDeclarationError::new(
                "duplicate foreign module specifier",
            ));
        }
        let mut imports = BTreeSet::new();
        let mut count = 0u32;
        for module in &modules {
            for function in module.functions() {
                if !imports.insert((function.import_module(), function.import_name())) {
                    return Err(ForeignDeclarationError::new(format!(
                        "duplicate or conflicting foreign Wasm import {:?}/{:?}",
                        function.import_module(),
                        function.import_name()
                    )));
                }
                count = count.checked_add(1).ok_or_else(|| {
                    ForeignDeclarationError::new(
                        "foreign registry exceeds its function index domain",
                    )
                })?;
            }
        }
        drop(imports);
        Ok(Self {
            modules,
            error_tag: None,
        })
    }
    pub const fn abi_version(&self) -> u32 {
        FOREIGN_ABI_VERSION
    }
    pub fn modules(&self) -> &[ForeignModuleDeclaration] {
        &self.modules
    }
    pub fn module(&self, specifier: &str) -> Option<&ForeignModuleDeclaration> {
        self.modules
            .binary_search_by(|module| module.specifier.as_str().cmp(specifier))
            .ok()
            .map(|index| &self.modules[index])
    }
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
    pub fn error_tag(&self) -> Option<&ForeignErrorTag> {
        self.error_tag.as_ref()
    }
    pub fn with_error_tag(mut self, tag: ForeignErrorTag) -> Self {
        self.error_tag = Some(tag);
        self
    }
    pub fn functions(
        &self,
    ) -> impl Iterator<Item = (ForeignFunctionId, &ForeignFunctionDeclaration)> {
        self.modules
            .iter()
            .flat_map(|module| module.functions.iter())
            .enumerate()
            .map(|(index, function)| {
                (
                    ForeignFunctionId(u32::try_from(index).expect("registry index validated")),
                    function,
                )
            })
    }
    pub fn function(&self, id: ForeignFunctionId) -> Option<&ForeignFunctionDeclaration> {
        self.functions()
            .find_map(|(candidate, function)| (candidate == id).then_some(function))
    }
    pub fn function_id(&self, specifier: &str, export: &str) -> Option<ForeignFunctionId> {
        let mut index = 0u32;
        for module in &self.modules {
            for function in &module.functions {
                if module.specifier == specifier && function.js_export == export {
                    return Some(ForeignFunctionId(index));
                }
                index += 1;
            }
        }
        None
    }
    pub fn to_normalized_json(&self) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "abi_version": FOREIGN_ABI_VERSION,
            "error_tag": self.error_tag.map(|tag| serde_json::json!({"module": tag.module(), "name": tag.name(), "payload": ["i32"]})),
            "modules": self.modules.iter().map(|module| serde_json::json!({
                "specifier": module.specifier(), "functions": module.functions().iter().map(|function| serde_json::json!({
                    "export": function.js_export(), "module": function.import_module(), "name": function.import_name(),
                    "mode": match function.mode() { ForeignFunctionMode::Synchronous => "sync", ForeignFunctionMode::Asynchronous => "async" },
                    "parameters": function.parameters().iter().map(|parameter| match parameter { ForeignValueType::Number => "number", ForeignValueType::StringUtf16 => "utf16", ForeignValueType::Bytes => "bytes" }).collect::<Vec<_>>(),
                    "result": match function.result() { ForeignResultType::Number => "number", ForeignResultType::StringUtf16 => "utf16", ForeignResultType::Bytes => "bytes", ForeignResultType::Void => "void" }
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        })).expect("validated registry serializes")
    }
    pub fn from_normalized_json(bytes: &[u8]) -> Result<Self, ForeignDeclarationError> {
        use serde_json::Value;
        fn fields<'a>(
            value: &'a Value,
            names: &[&str],
        ) -> Result<&'a serde_json::Map<String, Value>, ForeignDeclarationError> {
            let object = value.as_object().ok_or_else(|| {
                ForeignDeclarationError::new("foreign declaration must be an object")
            })?;
            if object.len() != names.len() || names.iter().any(|name| !object.contains_key(*name)) {
                return Err(ForeignDeclarationError::new(
                    "unknown or missing foreign declaration field",
                ));
            }
            Ok(object)
        }
        fn text<'a>(
            object: &'a serde_json::Map<String, Value>,
            key: &str,
        ) -> Result<&'a str, ForeignDeclarationError> {
            object[key].as_str().ok_or_else(|| {
                ForeignDeclarationError::new(format!("foreign {key} must be a string"))
            })
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|error| {
            ForeignDeclarationError::new(format!("invalid foreign declaration JSON: {error}"))
        })?;
        let root = fields(&value, &["abi_version", "error_tag", "modules"])?;
        if root["abi_version"].as_u64() != Some(u64::from(FOREIGN_ABI_VERSION)) {
            return Err(ForeignDeclarationError::new(
                "unsupported foreign ABI version",
            ));
        }
        let mut modules = Vec::new();
        for module in root["modules"]
            .as_array()
            .ok_or_else(|| ForeignDeclarationError::new("foreign modules must be an array"))?
        {
            let module = fields(module, &["specifier", "functions"])?;
            let mut functions = Vec::new();
            for function in module["functions"]
                .as_array()
                .ok_or_else(|| ForeignDeclarationError::new("foreign functions must be an array"))?
            {
                let function = fields(
                    function,
                    &["export", "module", "name", "mode", "parameters", "result"],
                )?;
                let mode = match text(function, "mode")? {
                    "sync" => ForeignFunctionMode::Synchronous,
                    "async" => ForeignFunctionMode::Asynchronous,
                    _ => {
                        return Err(ForeignDeclarationError::new(
                            "unsupported foreign function mode",
                        ))
                    }
                };
                let parameters = function["parameters"]
                    .as_array()
                    .ok_or_else(|| {
                        ForeignDeclarationError::new("foreign parameters must be an array")
                    })?
                    .iter()
                    .map(|parameter| match parameter.as_str() {
                        Some("number") => Ok(ForeignValueType::Number),
                        Some("utf16") => Ok(ForeignValueType::StringUtf16),
                        Some("bytes") => Ok(ForeignValueType::Bytes),
                        _ => Err(ForeignDeclarationError::new(
                            "unsupported foreign parameter type",
                        )),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let result = match text(function, "result")? {
                    "number" => ForeignResultType::Number,
                    "utf16" => ForeignResultType::StringUtf16,
                    "bytes" => ForeignResultType::Bytes,
                    "void" => ForeignResultType::Void,
                    _ => {
                        return Err(ForeignDeclarationError::new(
                            "unsupported foreign result type",
                        ))
                    }
                };
                functions.push(ForeignFunctionDeclaration::with_mode(
                    text(function, "export")?,
                    text(function, "module")?,
                    text(function, "name")?,
                    parameters,
                    result,
                    mode,
                )?);
            }
            modules.push(ForeignModuleDeclaration::new(
                text(module, "specifier")?,
                functions,
            )?);
        }
        let mut registry = Self::new(modules)?;
        if !root["error_tag"].is_null() {
            let tag = fields(&root["error_tag"], &["module", "name", "payload"])?;
            let expected = ForeignErrorTag::code_v1();
            if text(tag, "module")? != expected.module()
                || text(tag, "name")? != expected.name()
                || tag["payload"] != serde_json::json!(["i32"])
            {
                return Err(ForeignDeclarationError::new(
                    "unsupported foreign error tag ABI",
                ));
            }
            registry = registry.with_error_tag(expected);
        }
        if registry.to_normalized_json() != bytes {
            return Err(ForeignDeclarationError::new(
                "foreign declaration JSON is not canonical normalized ABI data",
            ));
        }
        Ok(registry)
    }
}

/// Only trusted module-wrapper lowering can mint the actual call operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignCallIr {
    function: ForeignFunctionId,
    operands: Vec<TypedExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForeignModuleSource {
    pub(crate) registry: ForeignModuleRegistry,
    pub(crate) canonical_bytes: Vec<u8>,
}
impl ForeignCallIr {
    pub(crate) fn new(
        registry: &ForeignModuleRegistry,
        function: ForeignFunctionId,
        operands: Vec<TypedExpr>,
    ) -> Self {
        assert_eq!(
            registry
                .function(function)
                .expect("declared wrapper function")
                .parameters()
                .len(),
            operands.len()
        );
        Self { function, operands }
    }
    pub const fn function(&self) -> ForeignFunctionId {
        self.function
    }
    pub fn operands(&self) -> &[TypedExpr] {
        &self.operands
    }
}

#[cfg(test)]
mod tests;
