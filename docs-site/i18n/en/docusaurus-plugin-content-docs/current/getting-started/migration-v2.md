# GPUI 2.0 Migration Guide

Starting with ZzClawTerm 0.2.0, upstream NyaTerm has brought the GPUI implementation into its 2.0 mainline, and this project now uses the native GPUI implementation as its technical baseline. Tauri/WebView references in older entries are historical only.

## Upgrade paths

- **Installed copies**: Check for updates in the app, then choose install and restart. You can also download the new installer from [Releases](https://github.com/jackfahdin/ZzClawTerm/releases) and install over the existing copy.
- **Portable copies**: Download and extract the complete new portable ZIP, copy `data/` from your old directory into the new one, then run the new `ZzClawTerm.exe`. Do not replace only the main executable; the bundled helpers are required as well.

## Data compatibility

Existing connections, credentials, settings, OTP data, known hosts, backups, and cloud-sync data are preserved. Back up important data before upgrading anyway; all portable user data lives under `data/`.

## Changelog

See the [changelog](/changelog) for release changes. Upstream making GPUI its mainline does not mean every ZzClawTerm behavior will follow upstream: our branding, X11 support, update channels, and packaging remain independently maintained.
