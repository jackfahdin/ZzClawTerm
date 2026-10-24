# macOS application icon

`icon.icns` is copied unchanged from `src-tauri/icons-macos/icon.icns` in
[486978d5d375cd433cf1be61d892ed48aa5d59f0](https://github.com/nyakang/zzclawterm/commit/486978d5d375cd433cf1be61d892ed48aa5d59f0).
Its transparent margins keep the artwork at the macOS Dock and Launchpad visual
size. `scripts/release/package_native.py` uses it only for macOS bundles; the
cross-platform icons remain in `../icons/`.
