fn main() {
    // `rust_embed` expands to `include_bytes!` for the files that exist when the
    // macro runs, so cargo only tracks those paths. Without this, adding or
    // removing an asset leaves a stale `EmbeddedAssets` and the new icon 404s at
    // runtime until something else forces `assets.rs` to recompile.
    println!("cargo:rerun-if-changed=assets");
}
