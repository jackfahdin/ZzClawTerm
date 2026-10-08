//! Packages are always validated from an owned snapshot, never a mutable external source.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zzclawterm_core::plugins::manifest::PluginManifest;
use zzclawterm_core::plugins::template::validate_template;
use zzclawterm_core::plugins::{
    ErrorCode, MAX_MANIFEST_BYTES, MAX_TEXT_BYTES, PluginError, PluginResult,
    validate_resource_path,
};

pub const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_FILES: usize = 512;

pub struct Package {
    pub manifest: PluginManifest,
    pub templates: BTreeMap<String, String>,
    pub component: Option<Vec<u8>>,
}

pub struct StagingDirectory {
    pub path: PathBuf,
    retained: bool,
}
impl StagingDirectory {
    pub fn new(staging: &Path) -> PluginResult<Self> {
        let path = staging.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&path).map_err(|_| io_error())?;
        Ok(Self {
            path,
            retained: false,
        })
    }
    pub fn retain(&mut self) {
        self.retained = true;
    }
}
impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.retained && self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// The old installation remains owned until the preference transaction commits.
/// A failed rollback retains its recovery tree instead of deleting user resources.
pub fn promote(
    stage: &mut StagingDirectory,
    target: &Path,
    staging_root: &Path,
    commit: impl FnOnce() -> PluginResult<()>,
) -> PluginResult<()> {
    let mut rollback = StagingDirectory::new(staging_root)?;
    let old = rollback.path.join("old");
    let had_old = target.exists();
    if had_old {
        let managed_root = staging_root.parent().ok_or_else(unsafe_path)?;
        reject_path_links(managed_root, target)?;
        fs::rename(target, &old).map_err(|_| io_error())?;
    }
    if fs::rename(&stage.path, target).is_err() {
        if had_old && fs::rename(&old, target).is_err() {
            rollback.retain();
            return Err(PluginError::new(
                ErrorCode::Io,
                "Replacement failed; the old package is retained in staging for recovery",
            ));
        }
        return Err(io_error());
    }
    if let Err(error) = commit() {
        if fs::rename(target, &stage.path).is_err()
            || (had_old && fs::rename(&old, target).is_err())
        {
            rollback.retain();
            stage.retain();
            return Err(PluginError::new(
                ErrorCode::Io,
                "Preference commit and rollback failed; preserve installed and staging directories for recovery",
            ));
        }
        return Err(error);
    }
    Ok(())
}

pub fn io_error() -> PluginError {
    PluginError::new(
        ErrorCode::Io,
        "Cannot access plugin files; check the source and directory permissions",
    )
}
fn unsafe_path() -> PluginError {
    PluginError::new(
        ErrorCode::UnsafePath,
        "Plugin packages cannot contain links, device paths or unsafe resource entries",
    )
}
fn package_limit() -> PluginError {
    PluginError::new(
        ErrorCode::PackageLimit,
        "Plugin package exceeds its file count or size limit",
    )
}

pub fn reject_link(path: &Path) -> PluginResult<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|_| io_error())?;
    if metadata.file_type().is_symlink() {
        return Err(unsafe_path());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(unsafe_path());
        }
    }
    Ok(metadata)
}

pub fn reject_path_links(root: &Path, path: &Path) -> PluginResult<()> {
    let relative = path.strip_prefix(root).map_err(|_| unsafe_path())?;
    reject_link(root)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        if current.exists() {
            reject_link(&current)?;
        }
    }
    Ok(())
}

pub fn snapshot(source: &Path, destination: &Path) -> PluginResult<()> {
    if reject_link(source)?.is_dir() {
        let mut count = 0;
        let mut bytes = 0;
        copy_tree(
            source,
            source,
            destination,
            &mut count,
            &mut bytes,
            &mut BTreeSet::new(),
        )
    } else {
        extract_archive(source, destination)
    }
}

fn copy_tree(
    root: &Path,
    directory: &Path,
    destination: &Path,
    count: &mut usize,
    bytes: &mut u64,
    names: &mut BTreeSet<String>,
) -> PluginResult<()> {
    for entry in fs::read_dir(directory).map_err(|_| io_error())? {
        let entry = entry.map_err(|_| io_error())?;
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|_| unsafe_path())?;
        let name = relative
            .to_str()
            .ok_or_else(unsafe_path)?
            .replace('\\', "/");
        validate_resource_path(&name)?;
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(unsafe_path());
        }
        *count += 1;
        if *count > MAX_FILES {
            return Err(package_limit());
        }
        let metadata = reject_link(&path)?;
        let target = destination.join(relative);
        if metadata.is_dir() {
            fs::create_dir(&target).map_err(|_| io_error())?;
            copy_tree(root, &path, destination, count, bytes, names)?;
        } else if metadata.is_file() {
            let data = read_bounded(&path, MAX_FILE_BYTES)?;
            *bytes += data.len() as u64;
            if *bytes > MAX_PACKAGE_BYTES {
                return Err(package_limit());
            }
            fs::write(&target, &data).map_err(|_| io_error())?;
        } else {
            return Err(unsafe_path());
        }
    }
    Ok(())
}

fn extract_archive(source: &Path, destination: &Path) -> PluginResult<()> {
    if fs::metadata(source).map_err(|_| io_error())?.len() > MAX_ARCHIVE_BYTES {
        return Err(package_limit());
    }
    let file = File::open(source).map_err(|_| io_error())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| {
        PluginError::new(
            ErrorCode::InvalidManifest,
            "Use a valid ZIP plugin archive with plugin.toml at its root",
        )
    })?;
    if archive.len() > MAX_FILES {
        return Err(package_limit());
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|_| unsafe_path())?;
        let name = if entry.is_dir() {
            entry.name().trim_end_matches('/')
        } else {
            entry.name()
        };
        validate_resource_path(name)?;
        if !names.insert(name.to_ascii_lowercase()) || entry.is_symlink() {
            return Err(unsafe_path());
        }
        if let Some(mode) = entry.unix_mode() {
            let kind = mode & 0o170000;
            if kind != 0 && kind != 0o100000 && kind != 0o040000 {
                return Err(unsafe_path());
            }
        }
        total = total.checked_add(entry.size()).ok_or_else(package_limit)?;
        if entry.size() > MAX_FILE_BYTES || total > MAX_PACKAGE_BYTES {
            return Err(package_limit());
        }
        let target = destination.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|_| io_error())?;
        } else {
            fs::create_dir_all(target.parent().ok_or_else(unsafe_path)?).map_err(|_| io_error())?;
            let mut data = Vec::new();
            entry
                .by_ref()
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|_| io_error())?;
            if data.len() as u64 > MAX_FILE_BYTES {
                return Err(package_limit());
            }
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|_| unsafe_path())?;
            file.write_all(&data).map_err(|_| io_error())?;
        }
    }
    Ok(())
}

pub fn read_bounded(path: &Path, limit: u64) -> PluginResult<Vec<u8>> {
    if !reject_link(path)?.is_file() {
        return Err(unsafe_path());
    }
    let mut file = File::open(path).map_err(|_| io_error())?;
    let mut data = Vec::new();
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|_| io_error())?;
    if data.len() as u64 > limit {
        return Err(package_limit());
    }
    Ok(data)
}

pub fn load(directory: &Path, host_version: &str) -> PluginResult<Package> {
    if !reject_link(directory)?.is_dir() {
        return Err(unsafe_path());
    }
    let raw = read_bounded(&directory.join("plugin.toml"), MAX_MANIFEST_BYTES as u64)?;
    let raw = std::str::from_utf8(&raw)
        .map_err(|_| PluginError::new(ErrorCode::InvalidManifest, "plugin.toml must be UTF-8"))?;
    let manifest = PluginManifest::parse(raw, host_version)?;
    let mut templates = BTreeMap::new();
    for action in &manifest.actions {
        if let Some(path) = &action.template {
            let path = directory.join(path);
            reject_path_links(directory, &path)?;
            let raw = read_bounded(&path, MAX_TEXT_BYTES as u64)?;
            let template = String::from_utf8(raw).map_err(|_| {
                PluginError::new(
                    ErrorCode::InvalidManifest,
                    "Template resources must be UTF-8",
                )
            })?;
            validate_template(action, &template)?;
            templates.insert(action.id.clone(), template);
        }
    }
    let component = manifest
        .component
        .as_ref()
        .map(|path| {
            let path = directory.join(path);
            reject_path_links(directory, &path)?;
            read_bounded(&path, MAX_FILE_BYTES)
        })
        .transpose()?;
    Ok(Package {
        manifest,
        templates,
        component,
    })
}
