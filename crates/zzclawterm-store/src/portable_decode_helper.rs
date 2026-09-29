use std::io::{Read, Write};
#[cfg(not(test))]
use std::process::{Command, Stdio};
#[cfg(not(test))]
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use zzclawterm_core::{PortableSnapshotError, RawPortableSnapshot, SecretString};

const HELPER_ARG: &str = "--zzclawterm-cloud-snapshot-decode-helper";
const MAX_CIPHERTEXT_BYTES: usize = 64 * 1024 * 1024;
#[cfg(not(test))]
const MAX_REPLY_BYTES: u64 = 128 * 1024 * 1024;
#[cfg(not(test))]
const HELPER_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize)]
struct DecodeReply {
    snapshot: RawPortableSnapshot,
    source_payload_hash: Option<String>,
}

pub fn run_cloud_snapshot_decode_helper_if_requested() -> Option<i32> {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new(HELPER_ARG)) {
        return None;
    }
    Some(match decode_from_stdin() {
        Ok(()) => 0,
        Err(_) => 1,
    })
}

fn decode_from_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = std::io::stdin().lock();
    let mut password_len = [0; 4];
    stdin.read_exact(&mut password_len)?;
    let password_len = u32::from_le_bytes(password_len) as usize;
    if password_len == 0 || password_len > 4_096 {
        return Err("invalid password length".into());
    }
    let mut password = vec![0; password_len];
    stdin.read_exact(&mut password)?;
    let password = SecretString::new(String::from_utf8(password)?);
    let mut ciphertext_len = [0; 8];
    stdin.read_exact(&mut ciphertext_len)?;
    let ciphertext_len = u64::from_le_bytes(ciphertext_len) as usize;
    if !(13..=MAX_CIPHERTEXT_BYTES).contains(&ciphertext_len) {
        return Err("invalid ciphertext length".into());
    }
    let mut ciphertext = vec![0; ciphertext_len];
    stdin.read_exact(&mut ciphertext)?;
    let mut stdout = std::io::stdout().lock();
    match crate::decode_encrypted_raw_portable_snapshot(&ciphertext, password.expose_secret()) {
        Ok(snapshot) => {
            let reply = DecodeReply {
                source_payload_hash: snapshot.source_payload_hash.clone(),
                snapshot,
            };
            let bytes = serde_json::to_vec(&reply)?;
            stdout.write_all(&[0])?;
            stdout.write_all(&(bytes.len() as u64).to_le_bytes())?;
            stdout.write_all(&bytes)?;
        }
        Err(PortableSnapshotError::Decrypt { .. }) => stdout.write_all(&[1])?,
        Err(PortableSnapshotError::MissingMasterPassword) => stdout.write_all(&[2])?,
        Err(_) => stdout.write_all(&[3])?,
    }
    stdout.flush()?;
    Ok(())
}

#[cfg(not(test))]
pub(crate) fn decode_remote_snapshot_in_helper(
    ciphertext: &[u8],
    password: &str,
) -> Result<RawPortableSnapshot, PortableSnapshotError> {
    if !(13..=MAX_CIPHERTEXT_BYTES).contains(&ciphertext.len()) {
        return Err(PortableSnapshotError::CorruptPayload);
    }
    if password.is_empty() || password.len() > 4_096 {
        return Err(PortableSnapshotError::MissingMasterPassword);
    }
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .arg(HELPER_ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or(PortableSnapshotError::CorruptPayload)?;
    let password = SecretString::from(password);
    let ciphertext = ciphertext.to_vec();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        stdin.write_all(&(password.expose_secret().len() as u32).to_le_bytes())?;
        stdin.write_all(password.expose_secret().as_bytes())?;
        stdin.write_all(&(ciphertext.len() as u64).to_le_bytes())?;
        stdin.write_all(&ciphertext)
    });
    let stdout = child
        .stdout
        .take()
        .ok_or(PortableSnapshotError::CorruptPayload)?;
    let reader = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        stdout.take(MAX_REPLY_BYTES + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= HELPER_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PortableSnapshotError::Codec(
                "cloud snapshot decoder timed out".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = writer.join();
    let bytes = reader
        .join()
        .map_err(|_| PortableSnapshotError::CorruptPayload)??;
    if !status.success() || bytes.len() > MAX_REPLY_BYTES as usize {
        return Err(PortableSnapshotError::CorruptPayload);
    }
    match bytes.first().copied() {
        Some(0) if bytes.len() >= 9 => {
            let size = u64::from_le_bytes(bytes[1..9].try_into().unwrap()) as usize;
            if size != bytes.len() - 9 {
                return Err(PortableSnapshotError::CorruptPayload);
            }
            let mut reply: DecodeReply = serde_json::from_slice(&bytes[9..])?;
            reply.snapshot.source_payload_hash = reply.source_payload_hash;
            Ok(reply.snapshot)
        }
        Some(1) => Err(PortableSnapshotError::Decrypt {
            new_error: "decoder helper rejected password".into(),
            legacy_error: "decoder helper rejected password".into(),
        }),
        Some(2) => Err(PortableSnapshotError::MissingMasterPassword),
        _ => Err(PortableSnapshotError::CorruptPayload),
    }
}
