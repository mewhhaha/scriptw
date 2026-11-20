//! `scriptw precompile`: the embedder's precompiled image, produced ahead of
//! deployment through the same `EmbeddedRuntime::precompile` an embedder calls.
use scriptw_engine::{
    configure_wasm_runtime, CompilationJobs, CompileOptions, EmbeddedRuntime, Engine,
    HostSurfacePolicy, NativeCompilationPolicy, RealmBuilder,
};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
struct PrecompileArgs {
    policy: NativeCompilationPolicy,
    source: String,
    module: bool,
    output: PathBuf,
}

fn parse(args: &[String]) -> Result<PrecompileArgs, String> {
    let mut policy = None;
    let mut source = None;
    let mut force_module = false;
    let mut output = None;
    let mut args = args.iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--policy" if policy.is_none() => {
                policy = Some(match args.next().map(String::as_str) {
                    Some("speed") => NativeCompilationPolicy::Speed,
                    Some("fast") => NativeCompilationPolicy::Fast,
                    Some(value) => {
                        return Err(format!(
                            "unknown --policy value: {value} (expected speed or fast)"
                        ))
                    }
                    None => return Err("--policy needs speed or fast".into()),
                });
            }
            "-o" | "--output" if output.is_none() => {
                let path = args
                    .next()
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| format!("{argument} needs an image path"))?;
                output = Some(PathBuf::from(path));
            }
            "--module" if !force_module => force_module = true,
            "--policy" | "-o" | "--output" | "--module" => {
                return Err(format!("{argument} may only be specified once"));
            }
            value if !value.starts_with('-') && source.is_none() => source = Some(value.to_owned()),
            value => return Err(format!("unknown precompile argument: {value}")),
        }
    }
    let source = source.ok_or_else(|| "precompile needs a source file".to_string())?;
    Ok(PrecompileArgs {
        policy: policy.unwrap_or_default(),
        module: force_module || super::is_module_path(&source),
        source,
        output: output.ok_or_else(|| "precompile needs -o IMAGE".to_string())?,
    })
}

pub(super) fn command(args: Vec<String>, host_surface_policy: HostSurfacePolicy) -> Result<(), String> {
    let PrecompileArgs {
        policy,
        source: path,
        module,
        output,
    } = parse(&args)?;
    let source = super::read_source(&path)?;
    let compiler = Engine::new(RealmBuilder::new().build());
    let options = CompileOptions {
        filename: Some(path.clone()),
        host_surface_policy,
        ..CompileOptions::default()
    };
    let unit = if module {
        compiler.compile_module(&source, options)
    } else {
        compiler.compile_script(&source, options)
    }
    .map_err(|error| error.to_string())?;
    let artifact = compiler.emit_wasm(&unit).map_err(|error| error.to_string())?;
    drop(unit);
    drop(compiler);
    let jobs = CompilationJobs::current();
    let mut config = wasmtime::Config::new();
    configure_wasm_runtime(&mut config, policy, jobs);
    let engine = wasmtime::Engine::new(&config)
        .map_err(|error| format!("cannot create the Wasmtime Engine: {error:#}"))?;
    let image = EmbeddedRuntime::precompile(&engine, &artifact).map_err(|error| error.to_string())?;
    publish(&output, image.as_bytes())?;
    println!(
        "precompiled image: {} ({} bytes, {} policy, {} compilation job(s))",
        output.display(),
        image.as_bytes().len(),
        match policy {
            NativeCompilationPolicy::Speed => "speed",
            NativeCompilationPolicy::Fast => "fast",
        },
        jobs.get()
    );
    Ok(())
}

/// Write a synced read-only sibling, then rename it over `path`: a reader sees
/// either the previous image or the complete new one, never a partial file.
fn publish(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .ok_or_else(|| "precompile output needs a file name".to_owned())?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary_name = std::ffi::OsString::from(".");
    temporary_name.push(name);
    temporary_name.push(format!(".scriptw-precompile-{}.tmp", std::process::id()));
    let temporary = parent.join(temporary_name);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o444);
    let mut file = options.open(&temporary).map_err(|error| {
        format!(
            "cannot create precompiled image transaction {}: {error}",
            temporary.display()
        )
    })?;
    let result: Result<(), String> = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
        // The mode is also set explicitly: creation masks it with the umask.
        let mut permissions = file
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", temporary.display()))?
            .permissions();
        permissions.set_readonly(true);
        #[cfg(unix)]
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o444);
        file.set_permissions(permissions)
            .map_err(|error| format!("cannot make {} read-only: {error}", temporary.display()))?;
        drop(file);
        fs::rename(&temporary, path)
            .map_err(|error| format!("cannot publish precompiled image {}: {error}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            format!("precompiled image was published but directory sync failed: {error}")
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn arguments_choose_policy_goal_and_output_once() {
        assert_eq!(
            parse(&strings(&["app.js", "-o", "app.swep"])).unwrap(),
            PrecompileArgs {
                policy: NativeCompilationPolicy::Speed,
                source: "app.js".into(),
                module: false,
                output: "app.swep".into(),
            }
        );
        let parsed = parse(&strings(&["--policy", "fast", "--module", "app.js", "--output", "a"]));
        assert_eq!(parsed.as_ref().map(|args| args.policy), Ok(NativeCompilationPolicy::Fast));
        assert_eq!(parsed.map(|args| args.module), Ok(true));
        assert!(parse(&strings(&["app.mjs", "-o", "a"])).unwrap().module);
        for invalid in [
            &["app.js"][..],
            &["-o", "a"],
            &["app.js", "-o"],
            &["app.js", "-o", ""],
            &["app.js", "-o", "a", "-o", "b"],
            &["app.js", "other.js", "-o", "a"],
            &["--policy", "size", "app.js", "-o", "a"],
            &["--policy", "fast", "--policy", "fast", "app.js", "-o", "a"],
            &["--module", "--module", "app.js", "-o", "a"],
            &["--jobs", "2", "app.js", "-o", "a"],
        ] {
            assert!(parse(&strings(invalid)).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn publication_replaces_the_image_read_only_and_leaves_no_transaction() {
        let directory = std::env::temp_dir().join(format!(
            "scriptw-precompile-publish-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let image = directory.join("app.swep");
        publish(&image, b"first").unwrap();
        publish(&image, b"second").unwrap();
        assert_eq!(fs::read(&image).unwrap(), b"second");
        let permissions = fs::metadata(&image).unwrap().permissions();
        assert!(permissions.readonly());
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&permissions) & 0o777,
            0o444
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(&directory).unwrap();
    }
}
