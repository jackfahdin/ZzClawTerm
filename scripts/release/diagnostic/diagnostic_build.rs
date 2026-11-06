//! Compiled into isolated diagnostic sources, never an ordinary release.

use std::fmt::{self, Write};

pub fn write_metadata(writer: &mut impl Write) -> fmt::Result {
    writeln!(
        writer,
        "Diagnostic-Build: {}",
        env!("ZZCLAWTERM_DIAGNOSTIC_ID")
    )?;
    writeln!(writer, "Source-SHA: {}", env!("ZZCLAWTERM_DIAGNOSTIC_SOURCE"))?;
    writeln!(
        writer,
        "Instrumentation-SHA256: {}",
        env!("ZZCLAWTERM_DIAGNOSTIC_PATCH")
    )?;
    writeln!(writer, "Compiler: {}", env!("ZZCLAWTERM_DIAGNOSTIC_RUSTC"))?;
    writeln!(
        writer,
        "Build-Profile: release; debuginfo=2; strip=none; panic=unwind"
    )
}

/// Exercises the actual GUI executable and panic hook without opening a store,
/// acquiring the application instance, or initializing GPUI.
pub fn run_self_test_if_requested() -> std::io::Result<bool> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--zzclawterm-diagnostic-self-test")) {
        return Ok(false);
    }
    let directory = args
        .next()
        .ok_or_else(|| std::io::Error::other("missing self-test directory"))?;
    crate::crash_report::install(directory.into())?;
    zzclawterm_core::diagnostic_probe::record(zzclawterm_core::diagnostic_probe::Stage::SaveRequested);
    let _ = std::panic::catch_unwind(symbol_probe);
    Ok(true)
}

#[inline(never)]
fn symbol_probe() {
    panic!("ZzClawTerm diagnostic symbol self-test (intentional, caught)");
}
