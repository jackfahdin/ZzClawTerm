# Native window backgrounds

Appearance settings offer **Opaque** (the default), **Transparent**, and **Native
Frosted Glass** for main windows. Settings and other child windows keep their
ordinary opaque background. Changes preview in the editing workspace; Apply
persists them and updates other workspaces, while Cancel restores the preview.

The three opacity controls affect backgrounds only:

- **Window Background Opacity** controls the theme tint underneath wallpaper and
  content surfaces. Its default is 20%; it is ignored in Opaque mode.
- **Content Surface Opacity** controls the backgrounds above the native material
  or wallpaper. Text, icons, explicit ANSI cell backgrounds, selections, cursors,
  and remote-desktop framebuffers retain their normal opacity.
- **Image Opacity** controls only the wallpaper image. Wallpaper may be used
  together with either native material.

These layers compose, so a content background can obscure a low-opacity window
tint. In native mode the terminal base background is painted once; the legacy
wallpaper shading is retained when using Opaque mode.

Windows requires build 17763 or newer, the native composition entry point, and
GPUI DirectComposition. `GPUI_DISABLE_DIRECT_COMPOSITION=1` or `true` disables
native materials. macOS uses the existing GPUI native visual-effect view. Other
platforms currently use Opaque. Known unsupported selections show a fallback
message and retain the requested preference. GPUI does not report whether the
system actually displayed a requested effect; system policy and accessibility
settings may change its appearance.

The settings document stores `appearance.window_material` as `opaque`,
`transparent`, or `blurred`, and `appearance.window_background_opacity` as an
integer percentage. Unknown material strings are retained and render as Opaque.
Both fields are device-local for cloud sync and included in full backups. Old
Tauri transparency keys are preserved and do not enable the new feature.

Mica, Mica Alt, adjustable blur radius, and opacity for the entire window's
contents are outside this implementation.

## Validation

Automated coverage includes compatibility reads and writes, unknown material
preservation, backup restoration, device-local sync merging, capability fallback,
draft cancellation, shared workspace updates, cached panel colors, single-layer
tint painting, and opaque ANSI explicit/inverse backgrounds.

Windows native visual validation passed, as confirmed by the user on 2026-10-17.
The specific system version and scenarios tested were not recorded. macOS visual
validation remains pending because a macOS host was unavailable. The GPUI test
renderer checks application composition and state; it does not verify the
operating system's blur or text rasterization.

The platform visual checklist covers wallpaper on/off, dark/light themes, repeated
material switching, maximization, multiple main windows, TUI programs, rapid
scrolling, large output, RDP/VNC pages, and text appearance. Also check the actual
result with system transparency disabled or reduced.
