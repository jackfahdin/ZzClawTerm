fn main() {
    // `rust_i18n::i18n!` reads `locales/` while the proc macro expands, so cargo
    // tracks none of those paths the way `include_str!` used to. Without this,
    // editing a translation leaves the previous catalog compiled into the binary
    // until something else forces `lib.rs` to rebuild.
    println!("cargo:rerun-if-changed=locales");
}
