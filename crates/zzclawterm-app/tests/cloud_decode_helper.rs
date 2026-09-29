use std::io::{Read, Write};
use std::process::{Command, Stdio};

use zzclawterm_core::RawPortableSnapshot;
use zzclawterm_store::encode_encrypted_raw_portable_snapshot;

fn decode_in_app_helper(ciphertext: &[u8], password: &str) -> Vec<u8> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_zzclawterm"))
        .arg("--zzclawterm-cloud-snapshot-decode-helper")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start application decoder helper");
    {
        let mut stdin = child.stdin.take().expect("helper stdin");
        stdin
            .write_all(&(password.len() as u32).to_le_bytes())
            .expect("password length");
        stdin.write_all(password.as_bytes()).expect("password");
        stdin
            .write_all(&(ciphertext.len() as u64).to_le_bytes())
            .expect("ciphertext length");
        stdin.write_all(ciphertext).expect("ciphertext");
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .expect("helper stdout")
        .read_to_end(&mut output)
        .expect("read helper reply");
    assert!(child.wait().expect("helper exit").success());
    output
}

#[test]
fn windows_app_decoder_helper_validates_snapshot_and_wrong_password() {
    let mut snapshot = RawPortableSnapshot::sync("device", "2.0.0");
    snapshot.recalculate_hash().expect("hash snapshot");
    let ciphertext = encode_encrypted_raw_portable_snapshot(&snapshot, "correct-password")
        .expect("encrypt snapshot");
    let reply = decode_in_app_helper(&ciphertext, "correct-password");
    assert_eq!(reply[0], 0);
    let length = u64::from_le_bytes(reply[1..9].try_into().unwrap()) as usize;
    assert_eq!(length, reply.len() - 9);
    let decoded: serde_json::Value = serde_json::from_slice(&reply[9..]).unwrap();
    assert_eq!(
        decoded["snapshot"]["meta"]["payload_hash"],
        snapshot.meta.payload_hash
    );

    assert_eq!(decode_in_app_helper(&ciphertext, "wrong-password"), [1]);
    let mut corrupted = ciphertext;
    corrupted[20] ^= 0xff;
    assert_eq!(decode_in_app_helper(&corrupted, "correct-password"), [1]);
}

#[test]
fn decoder_helper_exits_on_invalid_ipc_without_starting_app() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_zzclawterm"))
        .arg("--zzclawterm-cloud-snapshot-decode-helper")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start decoder helper");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(&0_u32.to_le_bytes())
        .expect("invalid password length");
    let output = child.wait_with_output().expect("helper exit");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
