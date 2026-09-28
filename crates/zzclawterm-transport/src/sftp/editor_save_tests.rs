use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{
    Attrs, Data, FileAttributes, Handle, OpenFlags, Packet, Status, StatusCode, Version,
};

use super::{
    OpenSftpConnection, OpenSftpSession, RemoteFilePath, RemoteTextMetadata, RemoteTextRevision,
    RemoteTextWriteResult, SftpService,
};
use crate::session_config::{SftpSettings, SshSessionConfig};

#[derive(Default)]
struct MockFiles {
    files: HashMap<String, Vec<u8>>,
    fail_replace: bool,
    fail_restore: bool,
    fail_cleanup: bool,
    bad_replacement_size: bool,
    atomic_replace: bool,
}

struct Peer {
    files: Arc<Mutex<MockFiles>>,
    supports_posix_rename: bool,
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: "en".into(),
    }
}

fn rename(
    files: &mut MockFiles,
    source: &str,
    target: &str,
    replace: bool,
) -> Result<(), StatusCode> {
    if files.fail_replace && source.contains(".zzclawterm-edit-") && target == "/file.txt" {
        return Err(StatusCode::Failure);
    }
    if files.fail_restore && source.contains(".zzclawterm-backup-") && target == "/file.txt" {
        return Err(StatusCode::Failure);
    }
    if !replace && files.files.contains_key(target) {
        return Err(StatusCode::Failure);
    }
    let content = files.files.remove(source).ok_or(StatusCode::NoSuchFile)?;
    files.files.insert(target.to_string(), content);
    Ok(())
}

impl russh_sftp::server::Handler for Peer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        let mut version = Version::new();
        if self.supports_posix_rename {
            version
                .extensions
                .insert("posix-rename@openssh.com".into(), "1".into());
        }
        Ok(version)
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let mut files = self.files.lock().unwrap();
        if flags.contains(OpenFlags::CREATE) {
            files.files.insert(filename.clone(), Vec::new());
        } else if !files.files.contains_key(&filename) {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(Handle {
            id,
            handle: filename,
        })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let mut files = self.files.lock().unwrap();
        let content = files.files.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
        let offset = offset as usize;
        content.resize(content.len().max(offset + data.len()), 0);
        content[offset..offset + data.len()].copy_from_slice(&data);
        Ok(ok(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let files = self.files.lock().unwrap();
        let content = files.files.get(&handle).ok_or(StatusCode::NoSuchFile)?;
        let offset = offset as usize;
        if offset >= content.len() {
            return Err(StatusCode::Eof);
        }
        let end = content.len().min(offset + len as usize);
        Ok(Data {
            id,
            data: content[offset..end].to_vec(),
        })
    }

    async fn close(&mut self, id: u32, _handle: String) -> Result<Status, Self::Error> {
        Ok(ok(id))
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let files = self.files.lock().unwrap();
        let content = files.files.get(&path).ok_or(StatusCode::NoSuchFile)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                size: Some(
                    if files.bad_replacement_size
                        && path == "/file.txt"
                        && content == b"new content"
                    {
                        0
                    } else {
                        content.len() as u64
                    },
                ),
                mtime: Some(1),
                ..FileAttributes::empty()
            },
        })
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        rename(&mut self.files.lock().unwrap(), &oldpath, &newpath, false)?;
        Ok(ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let mut files = self.files.lock().unwrap();
        if files.fail_cleanup && filename.contains(".zzclawterm-backup-") {
            return Err(StatusCode::Failure);
        }
        files
            .files
            .remove(&filename)
            .ok_or(StatusCode::NoSuchFile)?;
        Ok(ok(id))
    }

    async fn extended(
        &mut self,
        id: u32,
        request: String,
        data: Vec<u8>,
    ) -> Result<Packet, Self::Error> {
        if request != "posix-rename@openssh.com" {
            return Err(StatusCode::OpUnsupported);
        }
        let mut cursor = data.as_slice();
        let mut next_path = || {
            let (length, rest) = cursor.split_at(4);
            let length = u32::from_be_bytes(length.try_into().unwrap()) as usize;
            let (path, rest) = rest.split_at(length);
            cursor = rest;
            String::from_utf8(path.to_vec()).unwrap()
        };
        let source = next_path();
        let target = next_path();
        let mut files = self.files.lock().unwrap();
        files.atomic_replace = true;
        rename(&mut files, &source, &target, true)?;
        Ok(Packet::Status(ok(id)))
    }
}

fn seeded_service(supports_posix_rename: bool) -> (SftpService, Arc<Mutex<MockFiles>>) {
    let service = SftpService::new(SshSessionConfig {
        sftp: SftpSettings {
            compatibility_mode: true,
            ..Default::default()
        },
        ..Default::default()
    });
    let files = Arc::new(Mutex::new(MockFiles::default()));
    files
        .lock()
        .unwrap()
        .files
        .insert("/file.txt".into(), b"old".to_vec());
    let peer = Peer {
        files: files.clone(),
        supports_posix_rename,
    };
    let compatibility = service.compatibility.as_ref().unwrap();
    let session = compatibility
        .block_on(async move {
            let (client, server) = tokio::io::duplex(65536);
            tokio::spawn(russh_sftp::server::run(server, peer));
            Ok(OpenSftpSession {
                sftp: Arc::new(SftpSession::new(client).await?),
                connection: Arc::new(Mutex::new(Some(OpenSftpConnection::Multiplex))),
                persistent: true,
            })
        })
        .unwrap();
    *compatibility.cache.lock().unwrap() = Some(session);
    (service, files)
}

fn save(service: &SftpService) -> anyhow::Result<RemoteTextWriteResult> {
    let revision = RemoteTextRevision::from_bytes(
        b"old",
        RemoteTextMetadata {
            size: 3,
            modified_at: Some(1),
        },
    );
    service.write_text_document_path(
        &RemoteFilePath {
            display_path: "/file.txt".into(),
            raw_path_token: None,
        },
        "new content",
        Some(revision),
        false,
    )
}

#[test]
fn editor_save_uses_atomic_replacement_when_available() {
    let (service, files) = seeded_service(true);
    assert!(matches!(
        save(&service).unwrap(),
        RemoteTextWriteResult::Saved { .. }
    ));
    let files = files.lock().unwrap();
    assert_eq!(files.files["/file.txt"], b"new content");
    assert!(files.atomic_replace);
    assert_eq!(files.files.len(), 1);
}

#[test]
fn editor_save_uses_backup_on_older_servers() {
    let (service, files) = seeded_service(false);
    assert!(matches!(
        save(&service).unwrap(),
        RemoteTextWriteResult::Saved { .. }
    ));
    let files = files.lock().unwrap();
    assert_eq!(files.files["/file.txt"], b"new content");
    assert!(!files.atomic_replace);
    assert_eq!(files.files.len(), 1);
}

#[test]
fn editor_save_restores_original_when_replacement_fails() {
    let (service, files) = seeded_service(false);
    files.lock().unwrap().fail_replace = true;
    assert!(
        save(&service)
            .unwrap_err()
            .to_string()
            .contains("replace original")
    );
    let files = files.lock().unwrap();
    assert_eq!(files.files["/file.txt"], b"old");
    assert_eq!(files.files.len(), 1);
}

#[test]
fn editor_save_preserves_backup_when_restore_fails() {
    let (service, files) = seeded_service(false);
    {
        let mut files = files.lock().unwrap();
        files.fail_replace = true;
        files.fail_restore = true;
    }
    let error = save(&service).unwrap_err().to_string();
    let files = files.lock().unwrap();
    let backup = files
        .files
        .iter()
        .find(|(path, _)| path.contains(".zzclawterm-backup-"))
        .unwrap();
    assert_eq!(backup.1, b"old");
    assert!(error.contains(backup.0));
}

#[test]
fn editor_save_reports_cleanup_warning_after_success() {
    let (service, files) = seeded_service(false);
    files.lock().unwrap().fail_cleanup = true;
    let result = save(&service).unwrap();
    let RemoteTextWriteResult::SavedWithBackup { backup_path, .. } = result else {
        panic!("expected backup warning");
    };
    let files = files.lock().unwrap();
    assert_eq!(files.files["/file.txt"], b"new content");
    assert_eq!(files.files[&backup_path], b"old");
}

#[test]
fn editor_save_restores_original_when_replacement_verification_fails() {
    let (service, files) = seeded_service(false);
    files.lock().unwrap().bad_replacement_size = true;
    assert!(
        save(&service)
            .unwrap_err()
            .to_string()
            .contains("verification")
    );
    let files = files.lock().unwrap();
    assert_eq!(files.files["/file.txt"], b"old");
    assert_eq!(files.files.len(), 1);
}
