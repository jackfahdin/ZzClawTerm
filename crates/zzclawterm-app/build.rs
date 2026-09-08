fn main() {
    // `rust_embed` expands to `include_bytes!` for the files that exist when the
    // macro runs, so cargo only tracks those paths. Without this, adding or
    // removing an asset leaves a stale `EmbeddedAssets` and the new icon 404s at
    // runtime until something else forces `assets.rs` to recompile.
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=resources/icons/icon.ico");

    // GPUI's Windows platform loads the window/taskbar icon from the exe's
    // embedded icon resource #1, which is winresource's default icon id.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_icon();
    }
}

#[cfg(windows)]
fn embed_windows_icon() {
    if let Err(error) = winresource::WindowsResource::new()
        .set_icon("resources/icons/icon.ico")
        .compile()
    {
        println!("cargo:warning=failed to embed Windows icon resource: {error}");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon() {}
