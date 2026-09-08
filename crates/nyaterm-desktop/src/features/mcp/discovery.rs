use std::path::{Path, PathBuf};

use nyaterm_mcp_protocol::DiscoveryDocument;

pub struct DiscoveryStore {
    directory: PathBuf,
    file: PathBuf,
}

impl DiscoveryStore {
    pub fn new(config_dir: &Path) -> Self {
        let directory = config_dir.join("mcp");
        let file = directory.join("discovery.json");
        Self { directory, file }
    }

    pub fn remove(&self) -> anyhow::Result<()> {
        match std::fs::remove_file(&self.file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if let Ok(entries) = std::fs::read_dir(&self.directory) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with(".discovery-") && name.ends_with(".tmp") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    pub fn write(&self, document: &DiscoveryDocument) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.directory)?;
        set_private_directory_permissions(&self.directory)?;
        let temporary = self
            .directory
            .join(format!(".discovery-{}.tmp", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(document)?;
        std::fs::write(&temporary, bytes)?;
        if let Err(error) = set_private_file_permissions(&temporary) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }

        atomic_replace(&temporary, &self.file).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            anyhow::Error::from(error)
        })?;
        set_private_file_permissions(&self.file)
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn set_private_directory_permissions(path: &Path) -> anyhow::Result<()> {
    set_windows_current_user_acl(path, true)
}

#[cfg(windows)]
fn set_private_file_permissions(path: &Path) -> anyhow::Result<()> {
    set_windows_current_user_acl(path, false)
}

#[cfg(windows)]
fn set_windows_current_user_acl(path: &Path, directory: bool) -> anyhow::Result<()> {
    super::windows_acl::set_current_user_only(path, directory)
}

#[cfg(not(any(unix, windows)))]
fn set_private_directory_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn set_private_file_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

pub(super) fn default_config_dir() -> anyhow::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    default_config_dir_from(&executable, dirs::home_dir().as_deref())
}

fn default_config_dir_from(executable: &Path, home: Option<&Path>) -> anyhow::Result<PathBuf> {
    let directory = executable
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot resolve NyaTerm executable directory"))?;
    if ["nyaterm-portable", "portable.flag"]
        .into_iter()
        .any(|marker| directory.join(marker).is_file())
    {
        return Ok(directory.join("data").join("config"));
    }
    Ok(home
        .ok_or_else(|| anyhow::anyhow!("cannot resolve user home directory"))?
        .join(".nyaterm"))
}

#[cfg(test)]
mod tests {
    use super::{DiscoveryStore, default_config_dir_from};
    #[cfg(windows)]
    use nyaterm_mcp_protocol::DiscoveryDocument;

    #[test]
    fn stale_discovery_can_be_removed_repeatedly() {
        let root = std::env::temp_dir().join(format!("nyaterm-mcp-test-{}", uuid::Uuid::new_v4()));
        let store = DiscoveryStore::new(&root);
        store.remove().unwrap();
        store.remove().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn both_portable_markers_resolve_discovery_under_executable_data() {
        for marker in ["nyaterm-portable", "portable.flag"] {
            let root = std::env::temp_dir().join(format!(
                "nyaterm-mcp-portable-{}-{}",
                marker.replace('.', "-"),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join(marker), b"").unwrap();
            let executable = root.join(if cfg!(windows) {
                "nyaterm.exe"
            } else {
                "nyaterm"
            });
            assert_eq!(
                default_config_dir_from(&executable, None).unwrap(),
                root.join("data").join("config")
            );
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[cfg(windows)]
    #[test]
    fn discovery_replacement_preserves_private_acl_and_removes_temporary_files() {
        let root = std::env::temp_dir().join(format!(
            "nyaterm-mcp-discovery-test-{}",
            uuid::Uuid::new_v4()
        ));
        let store = DiscoveryStore::new(&root);
        let first = discovery_document("first-token", "first-generation");
        let second = discovery_document("second-token", "second-generation");

        store.write(&first).unwrap();
        store.write(&second).unwrap();

        let actual: DiscoveryDocument =
            serde_json::from_slice(&std::fs::read(&store.file).unwrap()).unwrap();
        assert_eq!(actual.token, second.token);
        assert_eq!(actual.generation, second.generation);
        super::super::windows_acl::assert_current_user_only(&store.directory, true);
        super::super::windows_acl::assert_current_user_only(&store.file, false);

        let temporary_files = std::fs::read_dir(&store.directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with(".discovery-") && name.ends_with(".tmp")
            })
            .count();
        assert_eq!(temporary_files, 0);

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    fn discovery_document(token: &str, generation: &str) -> DiscoveryDocument {
        DiscoveryDocument {
            version: 1,
            pid: std::process::id(),
            host: "127.0.0.1".into(),
            port: 47_123,
            token: token.into(),
            generation: generation.into(),
            permission_mode: "read-only".into(),
        }
    }
}
