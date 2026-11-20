// The binary and examples link this library, so they share its allocator.
#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::cell::RefCell;
use std::io::Write;
use std::sync::{Arc, Mutex};

type OutputBuffer = Arc<Mutex<Vec<u8>>>;

thread_local! {
    static CLI_STDOUT: RefCell<Option<OutputBuffer>> = const { RefCell::new(None) };
    static CLI_STDERR: RefCell<Option<OutputBuffer>> = const { RefCell::new(None) };
}

enum CliOutputEnding {
    None,
    Newline,
}

fn write_cli_stdout(arguments: std::fmt::Arguments<'_>, ending: CliOutputEnding) {
    CLI_STDOUT.with(|slot| {
        let output = slot.borrow();
        let output = output.as_ref().expect("CLI stdout is installed");
        let mut output = output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match ending {
            CliOutputEnding::None => {
                write!(output, "{arguments}").expect("writing CLI stdout should succeed");
            }
            CliOutputEnding::Newline => {
                writeln!(output, "{arguments}").expect("writing CLI stdout should succeed");
            }
        }
    });
}

fn write_cli_stderr(arguments: std::fmt::Arguments<'_>, ending: CliOutputEnding) {
    CLI_STDERR.with(|slot| {
        let output = slot.borrow();
        let output = output.as_ref().expect("CLI stderr is installed");
        let mut output = output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match ending {
            CliOutputEnding::None => {
                write!(output, "{arguments}").expect("writing CLI stderr should succeed");
            }
            CliOutputEnding::Newline => {
                writeln!(output, "{arguments}").expect("writing CLI stderr should succeed");
            }
        }
    });
}

macro_rules! print {
    ($($arg:tt)*) => {{ $crate::write_cli_stdout(format_args!($($arg)*), $crate::CliOutputEnding::None) }};
}

macro_rules! println {
    () => {{ $crate::write_cli_stdout(format_args!(""), $crate::CliOutputEnding::Newline) }};
    ($($arg:tt)*) => {{ $crate::write_cli_stdout(format_args!($($arg)*), $crate::CliOutputEnding::Newline) }};
}

macro_rules! eprintln {
    () => {{ $crate::write_cli_stderr(format_args!(""), $crate::CliOutputEnding::Newline) }};
    ($($arg:tt)*) => {{ $crate::write_cli_stderr(format_args!($($arg)*), $crate::CliOutputEnding::Newline) }};
}

include!("main.rs");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliCapture {
    pub exit_code: u8,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Runs one CLI invocation in-process with injected arguments and captured
/// output. Wasmtime's compiled engine and caches are shared process-wide, but
/// Product execution constructs a fresh ECMAScript realm for each invocation.
/// Differential replay delegates to the explicitly selected worker executable;
/// embedding callers supply `--worker-bin` because their process is not the CLI.
pub fn run_cli_capture<I, S>(args: I) -> CliCapture
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let stdout = Arc::new(Mutex::new(Vec::new()));
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let previous_stdout = CLI_STDOUT.with(|slot| slot.replace(Some(stdout.clone())));
    let previous_stderr = CLI_STDERR.with(|slot| slot.replace(Some(stderr.clone())));

    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let result = command_main(args, stdout.clone());
    let exit_code = match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    };

    CLI_STDOUT.with(|slot| *slot.borrow_mut() = previous_stdout);
    CLI_STDERR.with(|slot| *slot.borrow_mut() = previous_stderr);
    let stdout = Arc::try_unwrap(stdout)
        .expect("CLI stdout should have no remaining owners")
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let stderr = Arc::try_unwrap(stderr)
        .expect("CLI stderr should have no remaining owners")
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    CliCapture {
        exit_code,
        stdout,
        stderr,
    }
}

/// Reusable command entrypoint for embedders. Output is buffered only for the
/// duration of the synchronous invocation so host printing from engine worker
/// threads can safely target the injected stream.
pub fn run_cli<I, S>(args: I, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let capture = run_cli_capture(args);
    if stdout.write_all(&capture.stdout).is_err() || stderr.write_all(&capture.stderr).is_err() {
        return 1;
    }
    capture.exit_code
}
