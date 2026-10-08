use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=wit/plugin.wit");
    // The WIT package declaration is the ABI authority for both bindings.
    let wit = fs::read_to_string("wit/plugin.wit").expect("read plugin WIT");
    let version = wit
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("package zzclawterm:plugin@")?
                .strip_suffix(';')
        })
        .expect("versioned plugin WIT package");
    let parts: Vec<u32> = version
        .split('.')
        .map(|part| part.parse().expect("numeric plugin ABI version"))
        .collect();
    assert_eq!(parts.len(), 3, "plugin ABI must have major.minor.patch");
    assert_eq!(version, format!("{}.{}.{}", parts[0], parts[1], parts[2]));
    let output = format!(
        "pub const API_VERSION_TEXT: &str = {version:?};\n\
         pub const API_VERSION_PARTS: (u32, u32, u32) = ({}, {}, {});\n\
         pub const API_VERSION_BYTES: [u8; {}] = *b{version:?};\n",
        parts[0],
        parts[1],
        parts[2],
        version.len()
    );
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"))
            .join("abi_version.rs"),
        output,
    )
    .expect("write plugin ABI constants");
}
