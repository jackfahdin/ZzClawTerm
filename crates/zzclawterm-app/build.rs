fn main() {
    // `rust_embed` expands to `include_bytes!` for the files that exist when the
    // macro runs, so cargo only tracks those paths. Without this, adding or
    // removing an asset leaves a stale `EmbeddedAssets` and the new icon 404s at
    // runtime until something else forces `assets.rs` to recompile.
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=resources/icons/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        compile_windows_resources();
    }
}

fn compile_windows_resources() {
    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"),
    );
    let icon = manifest_dir.join("resources/icons/icon.ico");
    let icon = icon.to_string_lossy().replace('\\', "\\\\");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("Cargo OUT_DIR"));
    let resource = out_dir.join("zzclawterm.rc");
    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo package version");
    // Cargo has already validated SemVer. Match package_native.py: the final
    // numeric prerelease identifier becomes the fourth WORD; otherwise use 0.
    let without_build = version.split('+').next().expect("SemVer version");
    let (release, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(release, pre)| (release, Some(pre)));
    let revision = prerelease
        .and_then(|pre| pre.rsplit('.').next())
        .filter(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
        .unwrap_or("0");
    let numeric: Vec<u16> = release
        .split('.')
        .chain(std::iter::once(revision))
        .map(|part| {
            part.parse()
                .expect("Windows version component exceeds 65535")
        })
        .collect();
    let dotted = numeric
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(".");
    let comma = numeric
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let flags = if prerelease.is_some() { "0x2" } else { "0x0" };

    // GPUI's Windows backend loads the application icon from resource ID 1
    // and assigns it to the registered window class used by taskbar windows.
    std::fs::write(
        &resource,
        format!(
            r#"#include <winver.h>
LANGUAGE 0x09, 0x01
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {comma}
PRODUCTVERSION {comma}
FILEFLAGSMASK VS_FFI_FILEFLAGSMASK
FILEFLAGS {flags}
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
FILESUBTYPE 0
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "ZzClawTerm\0"
            VALUE "ProductName", "ZzClawTerm\0"
            VALUE "FileVersion", "{dotted}\0"
            VALUE "ProductVersion", "{version}\0"
            VALUE "InternalName", "zzclawterm\0"
            VALUE "OriginalFilename", "ZzClawTerm.exe\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
        ),
    )
    .expect("write ZzClawTerm Windows resource script");
    embed_resource::compile(&resource, embed_resource::NONE)
        .manifest_optional()
        .expect("compile ZzClawTerm Windows resources");
}
