//! `scriptw precompile`: the embedder's precompiled image, produced ahead of
//! deployment through the same `EmbeddedRuntime::precompile` an embedder calls.
use scriptw_engine::{
    configure_wasm_runtime, CompilationJobs, CompileOptions, EmbeddedRuntime, Engine,
    ExecutionProfile, ExecutionProfileUse, HostSurfacePolicy, NativeCompilationPolicy,
    RealmBuilder,
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
    /// An execution profile of this same program and compiler to build
    /// with (feature `profile-experiments` only).
    profile: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<PrecompileArgs, String> {
    let mut policy = None;
    let mut source = None;
    let mut force_module = false;
    let mut output = None;
    #[cfg_attr(not(feature = "profile-experiments"), allow(unused_mut))]
    let mut profile = None;
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
            #[cfg(feature = "profile-experiments")]
            "--profile" if profile.is_none() => {
                let path = args
                    .next()
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| "--profile needs an execution profile path".to_owned())?;
                profile = Some(PathBuf::from(path));
            }
            "--module" if !force_module => force_module = true,
            "--policy" | "-o" | "--output" | "--module" => {
                return Err(format!("{argument} may only be specified once"));
            }
            #[cfg(feature = "profile-experiments")]
            "--profile" => {
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
        profile,
    })
}

pub(super) fn command(
    args: Vec<String>,
    host_surface_policy: HostSurfacePolicy,
) -> Result<(), String> {
    let PrecompileArgs {
        policy,
        source: path,
        module,
        output,
        profile,
    } = parse(&args)?;
    let source = super::read_source(&path)?;
    let execution_profile = match &profile {
        None => ExecutionProfileUse::None,
        Some(profile) => {
            let bytes = fs::read(profile)
                .map_err(|error| format!("cannot read {}: {error}", profile.display()))?;
            ExecutionProfileUse::Apply(std::sync::Arc::new(
                ExecutionProfile::decode(&bytes)
                    .map_err(|error| format!("{}: {error}", profile.display()))?,
            ))
        }
    };
    let compiler = Engine::new(RealmBuilder::new().build());
    let options = CompileOptions {
        filename: Some(path.clone()),
        host_surface_policy,
        execution_profile,
        ..CompileOptions::default()
    };
    let unit = if module {
        compiler.compile_module(&source, options)
    } else {
        compiler.compile_script(&source, options)
    }
    .map_err(|error| error.to_string())?;
    let artifact = compiler
        .emit_wasm(&unit)
        .map_err(|error| error.to_string())?;
    drop(unit);
    drop(compiler);
    let jobs = CompilationJobs::current();
    let mut config = wasmtime::Config::new();
    configure_wasm_runtime(&mut config, policy, jobs);
    let engine = wasmtime::Engine::new(&config)
        .map_err(|error| format!("cannot create the Wasmtime Engine: {error:#}"))?;
    let image =
        EmbeddedRuntime::precompile(&engine, &artifact).map_err(|error| error.to_string())?;
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
///
/// The image's mode is the creation mode the umask allows, without write
/// bits (0444 under the usual umask 022, 0400 under 077). The mode does not
/// protect the image from replacement: rename needs only write access to the
/// directory, so the directory is the trust boundary.
fn publish(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .ok_or_else(|| "precompile output needs a file name".to_owned())?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let (temporary, mut file) = create_transaction(parent, name)?;
    let result: Result<(), String> = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
        // Creation already masked 0444 with the umask; this clears the
        // write bits where creation could not (and on other platforms),
        // never adding a read bit the umask removed.
        let mut permissions = file
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", temporary.display()))?
            .permissions();
        permissions.set_readonly(true);
        file.set_permissions(permissions)
            .map_err(|error| format!("cannot make {} read-only: {error}", temporary.display()))?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| {
            format!(
                "cannot publish precompiled image {}: {error}",
                path.display()
            )
        })
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

/// Creates a new transaction file next to the image. Its name carries the
/// process id and a random suffix, so a file a killed run left behind (whose
/// process id a later run may have again) never blocks publication.
fn create_transaction(
    parent: &Path,
    name: &std::ffi::OsStr,
) -> Result<(std::path::PathBuf, fs::File), String> {
    use std::hash::{BuildHasher, Hasher};
    let mut attempts = 0;
    loop {
        let random = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        let mut temporary_name = std::ffi::OsString::from(".");
        temporary_name.push(name);
        temporary_name.push(format!(
            ".scriptw-precompile-{}-{random:016x}.tmp",
            std::process::id()
        ));
        let temporary = parent.join(temporary_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o444);
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempts < 8 => {
                attempts += 1;
            }
            Err(error) => {
                return Err(format!(
                    "cannot create precompiled image transaction {}: {error}",
                    temporary.display()
                ))
            }
        }
    }
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
                profile: None,
            }
        );
        #[cfg(feature = "profile-experiments")]
        assert_eq!(
            parse(&strings(&["app.js", "--profile", "p.swprof", "-o", "a"]))
                .map(|args| args.profile),
            Ok(Some("p.swprof".into()))
        );
        #[cfg(not(feature = "profile-experiments"))]
        assert!(parse(&strings(&["app.js", "--profile", "p.swprof", "-o", "a"])).is_err());
        let parsed = parse(&strings(&[
            "--policy", "fast", "--module", "app.js", "--output", "a",
        ]));
        assert_eq!(
            parsed.as_ref().map(|args| args.policy),
            Ok(NativeCompilationPolicy::Fast)
        );
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
            &["app.js", "-o", "a", "--profile"],
            &["app.js", "-o", "a", "--profile", ""],
            &["app.js", "-o", "a", "--profile", "p", "--profile", "q"],
        ] {
            assert!(parse(&strings(invalid)).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn publication_replaces_the_image_read_only_and_leaves_no_transaction() {
        let directory =
            std::env::temp_dir().join(format!("scriptw-precompile-publish-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let image = directory.join("app.swep");
        publish(&image, b"first").unwrap();
        publish(&image, b"second").unwrap();
        assert_eq!(fs::read(&image).unwrap(), b"second");
        let permissions = fs::metadata(&image).unwrap().permissions();
        assert!(permissions.readonly());
        #[cfg(target_os = "linux")]
        {
            let umask = fs::read_to_string("/proc/self/status")
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("Umask:"))
                .map(|mask| u32::from_str_radix(mask.trim(), 8).unwrap())
                .unwrap();
            assert_eq!(
                std::os::unix::fs::PermissionsExt::mode(&permissions) & 0o777,
                0o444 & !umask
            );
        }
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(&directory).unwrap();
    }
}
