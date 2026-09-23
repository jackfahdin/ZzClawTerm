use std::env;
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use zzclawterm_transport::{
    SftpDuplicateDecision, SftpDuplicatePolicy, SftpDuplicateRequest, SftpDuplicateResolver,
    SftpFileType, SftpPathTransferOptions, SftpService, SftpSettings, SftpTransferControl,
    SftpTransferOptions, SftpWriteTextResult, SshCredentialProvider, SshHostKey,
    SshHostKeyDecision, SshHostKeyVerifier, SshOtpProvider, SshSessionConfig,
};

struct AcceptEphemeralHostKey;

struct MergeDirectoriesAndRenameFiles;

impl SftpDuplicateResolver for MergeDirectoriesAndRenameFiles {
    fn resolve_duplicate(
        &self,
        request: &SftpDuplicateRequest,
    ) -> Result<SftpDuplicateDecision, String> {
        Ok(if request.is_directory {
            SftpDuplicateDecision::Overwrite
        } else {
            SftpDuplicateDecision::Rename
        })
    }
}

impl SshHostKeyVerifier for AcceptEphemeralHostKey {
    fn verify(&self, _host_key: &SshHostKey) -> Result<SshHostKeyDecision, String> {
        Ok(SshHostKeyDecision::Accept)
    }
}

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} must be set for the SFTP E2E test"))
}

fn test_config() -> SshSessionConfig {
    SshSessionConfig {
        attempt: Default::default(),
        post_login: None,
        name: "SFTP E2E".to_string(),
        host: required_env("ZZCLAWTERM_TEST_SFTP_HOST"),
        port: required_env("ZZCLAWTERM_TEST_SFTP_PORT")
            .parse()
            .expect("ZZCLAWTERM_TEST_SFTP_PORT must be a valid port"),
        host_key_alias: None,
        username: required_env("ZZCLAWTERM_TEST_SFTP_USERNAME"),
        password: Some(required_env("ZZCLAWTERM_TEST_SFTP_PASSWORD").into()),
        key_auth: None,
        agent_auth: false,
        agent_endpoint: Default::default(),
        agent_forwarding: false,
        agent_forwarding_config: None,
        agent_stored_key_provider: None,
        otp_id: None,
        auto_fill_otp: false,
        proxy_jump: None,
        proxy: None,
        allow_none_auth: false,
        profile: Default::default(),
        backspace_mode: "del".to_string(),
        term: "xterm-256color".to_string(),
        x11_forwarding: false,
        x11_display: String::new(),
        encoding: "UTF-8".to_string(),
        ssh_algorithms: None,
        sftp: SftpSettings::default(),
        deferred_pty: true,
        terminal_shell_integration: true,
        keep_alive_interval_secs: 0,
        keep_alive_mode: Default::default(),
        cols: 80,
        rows: 24,
        pixel_width: 0,
        pixel_height: 0,
        host_key_verifier: Some(Arc::new(AcceptEphemeralHostKey)),
        credential_provider: None::<Arc<dyn SshCredentialProvider>>,
        agent_prompt_provider: None,
        otp_provider: None::<Arc<dyn SshOtpProvider>>,
    }
}

#[test]
#[ignore = "requires ZZCLAWTERM_TEST_SFTP_* variables and a disposable SFTP directory"]
fn sftp_service_round_trips_file_manager_operations() -> anyhow::Result<()> {
    let service = SftpService::new(test_config());
    let root = required_env("ZZCLAWTERM_TEST_SFTP_ROOT")
        .trim_end_matches('/')
        .to_string();
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let remote_dir = format!("{root}/zzclawterm-e2e-{}-{unique}", std::process::id());
    let remote_file = format!("{remote_dir}/file2.txt");
    let renamed_file = format!("{remote_dir}/file10.txt");
    let uploaded_file = format!("{remote_dir}/uploaded.txt");
    let local_dir =
        zzclawterm_core::test_support::TestTempDir::new(&format!("zzclawterm-sftp-e2e-{unique}"));
    let local_source = local_dir.join("source.txt");
    let local_download = local_dir.join("download.txt");

    fs::create_dir_all(&local_dir)?;
    fs::write(&local_source, "uploaded through SFTP\n")?;

    let result = (|| -> anyhow::Result<()> {
        service.create_dir_path(&remote_dir, Some(0o750))?;
        service.create_file_path(&remote_file, Some(0o640))?;

        let saved =
            service.write_text_file(&remote_file, "hello from ZzClawTerm\n", None, None, true)?;
        anyhow::ensure!(matches!(saved, SftpWriteTextResult::Saved { .. }));

        let text = service.read_text_file(&remote_file, 1024)?;
        anyhow::ensure!(text.content == "hello from ZzClawTerm\n");

        let properties = service.file_properties(&remote_file)?;
        anyhow::ensure!(properties.name == "file2.txt");
        anyhow::ensure!(properties.file_type == SftpFileType::File);

        service.rename_path(&remote_file, &renamed_file)?;
        service.upload_file(&local_source, &uploaded_file)?;
        service.download_file(&uploaded_file, &local_download)?;
        anyhow::ensure!(fs::read_to_string(&local_download)? == "uploaded through SFTP\n");

        let entries = service.list_dir(&remote_dir)?;
        anyhow::ensure!(entries.iter().any(|entry| entry.name == "file10.txt"));
        anyhow::ensure!(entries.iter().any(|entry| entry.name == "uploaded.txt"));
        anyhow::ensure!(!entries.iter().any(|entry| entry.name == "file2.txt"));

        // Exercise actual directory enumeration, deferred downloads, and commit together.
        // Invalid local names on ignored links must not abort the regular-file downloads.
        let collision_dir = format!("{remote_dir}/collisions");
        service.create_dir_path(&collision_dir, None)?;
        for (name, contents) in [("foo", "first"), ("foo(1)", "second")] {
            let path = format!("{collision_dir}/{name}");
            service.create_file_path(&path, None)?;
            service.write_text_file(&path, contents, None, None, true)?;
        }
        service.create_symlink_path(format!("{collision_dir}/bad\\link"), "foo")?;
        service.create_symlink_path(format!("{collision_dir}/CON"), "foo")?;
        let collision_target = local_dir.join("collisions");
        fs::create_dir(&collision_target)?;
        fs::write(collision_target.join("foo"), b"existing")?;
        let summary = service.download_path_with_progress_and_path_options(
            &collision_dir,
            &collision_target,
            SftpTransferControl::default(),
            SftpPathTransferOptions::new(
                SftpDuplicatePolicy::Ask,
                Some(Arc::new(MergeDirectoriesAndRenameFiles)),
                SftpTransferOptions::default().with_download_threads(2),
            ),
            |_| {},
        )?;
        anyhow::ensure!(!summary.skipped);
        anyhow::ensure!(fs::read(collision_target.join("foo"))? == b"existing");
        let mut contents = fs::read_dir(&collision_target)?
            .map(|entry| fs::read(entry?.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        contents.sort();
        anyhow::ensure!(contents == [b"existing".to_vec(), b"first".to_vec(), b"second".to_vec()]);
        Ok(())
    })();

    let remote_cleanup = service.delete_path(&remote_dir);
    let local_cleanup = fs::remove_dir_all(&local_dir);
    result?;
    remote_cleanup?;
    local_cleanup?;
    Ok(())
}
