use super::{DiagnosticsExportOptions, DiagnosticsRuntimeSnapshot, export_diagnostics_archive};
use crate::{AppRuntime, RuntimeMode};

#[test]
fn exports_diagnostics_archive_with_manifest_runtime_and_logs() {
    let (_temp_dir, runtime) = test_runtime("diagnostics-export");
    std::fs::create_dir_all(runtime.log_dir()).expect("log dir");
    std::fs::write(
        runtime.log_dir().join("zzclawterm-diagnostics.test.jsonl"),
        b"{\"event\":\"test\"}\n",
    )
    .expect("write log");
    let output_path = runtime.data_dir().join("diagnostics.zip");

    let info = export_diagnostics_archive(&runtime, &test_options(), &output_path)
        .expect("export diagnostics");

    assert_eq!(info.log_files, 1);
    assert!(info.bytes > 0);
    let file = std::fs::File::open(&output_path).expect("open zip");
    let mut archive = zip::ZipArchive::new(file).expect("read zip");
    assert!(archive.by_name("manifest.json").is_ok());
    assert!(archive.by_name("runtime_snapshot.json").is_ok());
    assert!(
        archive
            .by_name("logs/zzclawterm-diagnostics.test.jsonl")
            .is_ok()
    );

    std::fs::remove_dir_all(runtime.data_dir()).ok();
}

#[test]
fn diagnostics_archive_ignores_non_matching_logs() {
    let (_temp_dir, runtime) = test_runtime("diagnostics-filter");
    std::fs::create_dir_all(runtime.log_dir()).expect("log dir");
    std::fs::write(runtime.log_dir().join("other.jsonl"), b"skip").expect("write other");
    let output_path = runtime.data_dir().join("diagnostics.zip");

    let info = export_diagnostics_archive(&runtime, &test_options(), &output_path)
        .expect("export diagnostics");

    assert_eq!(info.log_files, 0);
    let file = std::fs::File::open(&output_path).expect("open zip");
    let mut archive = zip::ZipArchive::new(file).expect("read zip");
    assert!(archive.by_name("logs/other.jsonl").is_err());

    std::fs::remove_dir_all(runtime.data_dir()).ok();
}

fn test_runtime(name: &str) -> (crate::test_support::TestTempDir, AppRuntime) {
    let data_dir = crate::test_support::TestTempDir::new(&format!("zzclawterm-core-{name}"));
    let runtime = AppRuntime::from_parts_for_test(
        RuntimeMode::Installed,
        data_dir.path().to_path_buf(),
        data_dir.join("config"),
        data_dir.join("logs"),
        data_dir.join("cache"),
        None,
    );
    (data_dir, runtime)
}

fn test_options() -> DiagnosticsExportOptions {
    DiagnosticsExportOptions {
        app_version: "test".to_string(),
        language: "en".to_string(),
        log_level: "info".to_string(),
        retention_days: 7,
        runtime_snapshot: DiagnosticsRuntimeSnapshot {
            active_sessions: 1,
            local_sessions: 1,
            ssh_sessions: 0,
            telnet_sessions: 0,
            raw_tcp_sessions: 0,
            serial_sessions: 0,
            open_tunnels: 0,
            pending_tunnels: 0,
            saved_connections: 2,
            saved_tunnels: 3,
            running_transfers: 0,
            paused_transfers: 0,
            completed_transfers: 0,
            failed_transfers: 0,
        },
    }
}
