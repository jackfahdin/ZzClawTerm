use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(super) fn prepare_artifact(
    artifact: PathBuf,
    executable: PathBuf,
) -> Result<(PathBuf, PathBuf), String> {
    #[cfg(windows)]
    {
        if !executable
            .parent()
            .is_some_and(|directory| directory.join("Uninstall.exe").is_file())
        {
            return Err("this installation must be updated manually".into());
        }
        Ok((artifact, executable))
    }
    #[cfg(target_os = "macos")]
    {
        let target = executable
            .ancestors()
            .nth(3)
            .ok_or("application bundle unavailable")?
            .to_path_buf();
        if target
            .components()
            .any(|part| part.as_os_str() == "Caskroom" || part.as_os_str() == "Cellar")
        {
            return Err("use Homebrew to update this installation".into());
        }
        if target.extension().and_then(|value| value.to_str()) != Some("app") {
            return Err("not an application bundle".into());
        }
        if target.file_name().and_then(|name| name.to_str())
            != Some(zzclawterm_core::app_identity::AppFlavor::current().macos_bundle_name())
        {
            return Err("application bundle belongs to a different identity".into());
        }
        let stage = target
            .parent()
            .ok_or("application parent unavailable")?
            .join(format!(".zzclawterm-update-{}", zzclawterm_core::uuid()));
        std::fs::create_dir(&stage).map_err(|error| error.to_string())?;
        let file = std::fs::File::open(&artifact).map_err(|error| error.to_string())?;
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for entry in archive.entries().map_err(|error| error.to_string())? {
            let mut entry = entry.map_err(|error| error.to_string())?;
            if !entry.header().entry_type().is_file() && !entry.header().entry_type().is_dir() {
                return Err("unsupported bundle archive entry".into());
            }
            if !entry.unpack_in(&stage).map_err(|error| error.to_string())? {
                return Err("invalid bundle archive path".into());
            }
        }
        let bundle =
            staged_macos_bundle(&stage, zzclawterm_core::app_identity::AppFlavor::current())?;
        Ok((bundle, target))
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = executable;
        let target = PathBuf::from(
            std::env::var_os("APPIMAGE")
                .ok_or("use the package manager to update this installation")?,
        );
        let target = target.canonicalize().map_err(|error| error.to_string())?;
        let stage = target
            .parent()
            .ok_or("AppImage parent unavailable")?
            .join(format!(
                ".zzclawterm-update-{}.AppImage",
                zzclawterm_core::uuid()
            ));
        std::fs::copy(&artifact, &stage).map_err(|error| error.to_string())?;
        std::fs::set_permissions(&stage, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        Ok((stage, target))
    }
}

/// Start a detached installer only after the existing application close guards pass.
/// The helper waits for this process before touching the installed application.
pub(super) fn launch_installer(artifact: &Path, target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        let script = artifact
            .parent()
            .ok_or("update directory unavailable")?
            .join("install.ps1");
        std::fs::write(
            &script,
            r#"param([int]$ParentProcessId,[string]$Installer,[string]$Executable)
$ErrorActionPreference = 'Stop'
Wait-Process -Id $ParentProcessId -ErrorAction SilentlyContinue
$directory = [System.IO.Path]::GetDirectoryName([System.IO.Path]::GetFullPath($Executable))
if (-not (Test-Path -LiteralPath ([System.IO.Path]::Combine($directory, 'Uninstall.exe')))) { throw 'Not an installed ZzClawTerm directory' }
$backup = $directory + '.previous-' + [guid]::NewGuid().ToString('N')
Copy-Item -LiteralPath $directory -Destination $backup -Recurse -ErrorAction Stop
$process = Start-Process -FilePath $Installer -ArgumentList @('/S', ('/D=' + $directory)) -PassThru -Wait -WindowStyle Hidden
if ($process.ExitCode -ne 0) {
  $failed = $directory + '.failed-' + [guid]::NewGuid().ToString('N')
  Move-Item -LiteralPath $directory -Destination $failed
  Move-Item -LiteralPath $backup -Destination $directory
}
Start-Process -FilePath $Executable -WindowStyle Normal
"#,
        )
        .map_err(|error| error.to_string())?;
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(script)
            .arg(std::process::id().to_string())
            .arg(artifact)
            .arg(target)
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(unix)]
    {
        let script = r#"parent=$1; staged=$2; target=$3; mode=$4
while kill -0 "$parent" 2>/dev/null; do sleep 0.2; done
backup="${target}.previous.$$"
if test -e "$backup"; then exit 1; fi
mv -- "$target" "$backup" || exit 1
if mv -- "$staged" "$target"; then
  if test "$mode" = macos; then open "$target"; else "$target" >/dev/null 2>&1 & fi
else
  mv -- "$backup" "$target"
fi
"#;
        Command::new("sh")
            .args(["-c", script, "zzclawterm-update"])
            .arg(std::process::id().to_string())
            .arg(artifact)
            .arg(target)
            .arg(std::env::consts::OS)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn staged_macos_bundle(
    stage: &Path,
    flavor: zzclawterm_core::app_identity::AppFlavor,
) -> Result<PathBuf, String> {
    let bundle = stage.join(flavor.macos_bundle_name());
    for binary in [
        "ZzClawTerm",
        "zzclawterm-rdp-helper",
        "zzclawterm-vnc-helper",
        "zzclawterm-mcp",
    ] {
        if !bundle.join("Contents/MacOS").join(binary).is_file() {
            return Err("update bundle is missing an application helper".into());
        }
    }
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::staged_macos_bundle;
    use zzclawterm_core::app_identity::AppFlavor;

    #[test]
    fn staged_updates_require_the_current_flavor_bundle_and_all_helpers() {
        let stage = std::env::temp_dir().join(format!(
            "zzclawterm-update-test-{}",
            zzclawterm_core::uuid()
        ));
        for flavor in [AppFlavor::Stable, AppFlavor::Preview] {
            assert!(staged_macos_bundle(&stage, flavor).is_err());
            let bundle = stage.join(flavor.macos_bundle_name());
            let binaries = bundle.join("Contents/MacOS");
            std::fs::create_dir_all(&binaries).unwrap();
            for name in [
                "ZzClawTerm",
                "zzclawterm-rdp-helper",
                "zzclawterm-vnc-helper",
            ] {
                std::fs::write(binaries.join(name), b"test").unwrap();
            }
            assert!(staged_macos_bundle(&stage, flavor).is_err());
            std::fs::write(binaries.join("zzclawterm-mcp"), b"test").unwrap();
            assert_eq!(staged_macos_bundle(&stage, flavor).unwrap(), bundle);
        }
        std::fs::remove_dir_all(stage).unwrap();
    }
}
