//! Shared UI theme tokens (Tauri ThemeColors / --df-* CSS vars).
//! Palette ids preserve the pre-GPUI NyaTerm theme schema.

/// Shell chrome palette keyed by appearance.theme (Tauri theme ids).
/// Field names map to Tauri `ThemeColors` / CSS vars (`--df-*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub bg: u32,
    pub surface: u32,
    pub surface_elevated: u32,
    pub section_header: u32,
    pub hover: u32,
    pub input: u32,
    pub border: u32,
    pub text: u32,
    pub text_muted: u32,
    pub text_dimmed: u32,
    pub primary: u32,
    pub primary_hover: u32,
    pub on_primary: u32,
    pub focus_ring: u32,
    pub link: u32,
    pub accent: u32,
    pub success: u32,
    pub warning: u32,
    pub danger: u32,
    pub terminal_bg: u32,
    pub terminal_fg: u32,
    pub terminal_cursor: u32,
    pub terminal_selection: u32,
    /// ANSI 0..=15 (normal + bright), Tauri `ThemeColors.terminal`.
    pub terminal_ansi: [u32; 16],
}

impl ThemePalette {
    pub fn terminal_ansi_color(self, index: u8) -> u32 {
        self.terminal_ansi[usize::from(index.min(15))]
    }

    /// Boost terminal fg/ANSI contrast against terminal background (Tauri minimum_contrast_ratio).
    pub fn apply_minimum_contrast_ratio(&mut self, ratio: f32) {
        if ratio <= 1.01 {
            return;
        }
        let bg = self.terminal_bg;
        self.terminal_fg = ensure_contrast(self.terminal_fg, bg, ratio);
        self.terminal_cursor = ensure_contrast(self.terminal_cursor, bg, ratio.min(4.5));
        for color in &mut self.terminal_ansi {
            *color = ensure_contrast(*color, bg, ratio);
        }
    }
}

fn relative_luminance(rgb: u32) -> f32 {
    let channel = |c: u32| -> f32 {
        let v = (c as f32) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let r = channel((rgb >> 16) & 0xff);
    let g = channel((rgb >> 8) & 0xff);
    let b = channel(rgb & 0xff);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn contrast_ratio(a: u32, b: u32) -> f32 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (lighter, darker) = if la > lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

fn ensure_contrast(fg: u32, bg: u32, min_ratio: f32) -> u32 {
    if contrast_ratio(fg, bg) >= min_ratio {
        return fg;
    }
    let bg_lum = relative_luminance(bg);
    // Prefer lightening on dark bg, darkening on light bg.
    let target_toward_white = bg_lum < 0.5;
    let mut best = fg;
    let mut best_ratio = contrast_ratio(fg, bg);
    for step in 1..=24 {
        let t = step as f32 / 24.0;
        let mix = |c: u32, toward: u32| -> u32 {
            let c = c as f32;
            let toward = toward as f32;
            (c + (toward - c) * t).round().clamp(0.0, 255.0) as u32
        };
        let candidate = if target_toward_white {
            let r = mix((fg >> 16) & 0xff, 0xff);
            let g = mix((fg >> 8) & 0xff, 0xff);
            let b = mix(fg & 0xff, 0xff);
            (r << 16) | (g << 8) | b
        } else {
            let r = mix((fg >> 16) & 0xff, 0x00);
            let g = mix((fg >> 8) & 0xff, 0x00);
            let b = mix(fg & 0xff, 0x00);
            (r << 16) | (g << 8) | b
        };
        let ratio = contrast_ratio(candidate, bg);
        if ratio > best_ratio {
            best_ratio = ratio;
            best = candidate;
        }
        if ratio >= min_ratio {
            return candidate;
        }
    }
    best
}

/// All selectable appearance theme ids (Tauri theme list order).
pub const APPEARANCE_THEME_IDS: &[&str] = &[
    "github-dark",
    "nya-high-contrast",
    "nya-high-contrast-white",
    "dracula",
    "nord",
    "monokai-pro",
    "solarized-light",
    "catppuccin-mocha",
    "tokyo-night",
    "one-dark-pro",
    "rose-pine",
    "gruvbox-dark",
    "github-light",
    "catppuccin-latte",
    "rose-pine-dawn",
    "nord-light",
    "one-light",
];

/// Human label for Settings theme chips.
pub fn appearance_theme_label(theme: &str) -> &'static str {
    match theme {
        "github-dark" => "GitHub Dark",
        "nya-high-contrast" => "Nya HC",
        "nya-high-contrast-white" => "Nya HC White",
        "dracula" => "Dracula",
        "nord" => "Nord",
        "monokai-pro" => "Monokai Pro",
        "solarized-light" => "Solarized",
        "catppuccin-mocha" => "Catppuccin",
        "tokyo-night" => "Tokyo Night",
        "one-dark-pro" => "One Dark",
        "rose-pine" => "Rosé Pine",
        "gruvbox-dark" => "Gruvbox",
        "github-light" => "GitHub Light",
        "catppuccin-latte" => "Latte",
        "rose-pine-dawn" => "Pine Dawn",
        "nord-light" => "Nord Light",
        "one-light" => "One Light",
        "catppuccin" => "Catppuccin",
        _ => "Custom",
    }
}

pub fn theme_palette(theme: &str) -> ThemePalette {
    match theme {
        "github-dark" => ThemePalette {
            bg: 0x0d1117,
            surface: 0x161b22,
            surface_elevated: 0x21262d,
            section_header: 0x12171f,
            hover: 0x1c2128,
            input: 0x0d1117,
            border: 0x30363d,
            text: 0xc9d1d9,
            text_muted: 0x8b949e,
            text_dimmed: 0x6e7681,
            primary: 0x3b82f6,
            primary_hover: 0x2563eb,
            on_primary: 0x0d1117,
            focus_ring: 0x58a6ff,
            link: 0x58a6ff,
            accent: 0x58a6ff,
            success: 0x3fb950,
            warning: 0xd29922,
            danger: 0xff7b72,
            terminal_bg: 0x0d1117,
            terminal_fg: 0xc9d1d9,
            terminal_cursor: 0xc9d1d9,
            terminal_selection: 0x264f78,
            terminal_ansi: [
                0x484f58, 0xff7b72, 0x3fb950, 0xd29922, 0x58a6ff, 0xbc8cff, 0x39c5cf, 0xb1bac4,
                0x8b949e, 0xffa198, 0x3fb950, 0xe3b341, 0x79c0ff, 0xd2a8ff, 0x56d4dd, 0xf0f6fc,
            ],
        },
        "nya-high-contrast" => ThemePalette {
            bg: 0x03070d,
            surface: 0x07111c,
            surface_elevated: 0x102337,
            section_header: 0x081320,
            hover: 0x102337,
            input: 0x071624,
            border: 0x1b3852,
            text: 0xd7dee8,
            text_muted: 0x9aa8ba,
            text_dimmed: 0x6f7d90,
            primary: 0x22d3ee,
            primary_hover: 0x06b6d4,
            on_primary: 0x001116,
            focus_ring: 0x67e8f9,
            link: 0x67e8f9,
            accent: 0xf472b6,
            success: 0x6ee7b7,
            warning: 0xfacc15,
            danger: 0xff6b6b,
            terminal_bg: 0x000307,
            terminal_fg: 0xd7dee8,
            terminal_cursor: 0xf8fafc,
            terminal_selection: 0x123a55,
            terminal_ansi: [
                0x1b2430, 0xff6b6b, 0x6ee7b7, 0xfacc15, 0x60a5fa, 0xc084fc, 0x22d3ee, 0xd7dee8,
                0x6f7d90, 0xff8a8a, 0x8ff0cb, 0xfde047, 0x93c5fd, 0xd8b4fe, 0x67e8f9, 0xffffff,
            ],
        },
        "nya-high-contrast-white" => ThemePalette {
            bg: 0xffffff,
            surface: 0xf3f6fb,
            surface_elevated: 0xdbe5f2,
            section_header: 0xe8eef7,
            hover: 0xdbe5f2,
            input: 0xffffff,
            border: 0x7c8da3,
            text: 0x0b1220,
            text_muted: 0x26384f,
            text_dimmed: 0x5f7087,
            primary: 0x004e89,
            primary_hover: 0x003a66,
            on_primary: 0xffffff,
            focus_ring: 0x0067b8,
            link: 0x075985,
            accent: 0x6d28d9,
            success: 0x166534,
            warning: 0x854d0e,
            danger: 0xb91c1c,
            terminal_bg: 0xffffff,
            terminal_fg: 0x0b1220,
            terminal_cursor: 0x004e89,
            terminal_selection: 0xc8dcf6,
            terminal_ansi: [
                0x111827, 0xb91c1c, 0x166534, 0x854d0e, 0x1d4ed8, 0x7e22ce, 0x0e7490, 0x4b5563,
                0x374151, 0x991b1b, 0x14532d, 0x713f12, 0x1e40af, 0x6b21a8, 0x155e75, 0x111827,
            ],
        },
        "dracula" => ThemePalette {
            bg: 0x251b33,
            surface: 0x1d1828,
            surface_elevated: 0x342743,
            section_header: 0x2b203a,
            hover: 0x342743,
            input: 0x1f1a2b,
            border: 0x5a426e,
            text: 0xf8f8f2,
            text_muted: 0xbd93f9,
            text_dimmed: 0x6272a4,
            primary: 0xbd93f9,
            primary_hover: 0xa77bfa,
            on_primary: 0x282a36,
            focus_ring: 0xbd93f9,
            link: 0x8be9fd,
            accent: 0xff79c6,
            success: 0x50fa7b,
            warning: 0xf1fa8c,
            danger: 0xff5555,
            terminal_bg: 0x24172f,
            terminal_fg: 0xf8f8f2,
            terminal_cursor: 0xf8f8f2,
            terminal_selection: 0x4a3561,
            terminal_ansi: [
                0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
                0x7f8fbd, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
            ],
        },
        "nord" => ThemePalette {
            bg: 0x253040,
            surface: 0x334154,
            surface_elevated: 0x41506a,
            section_header: 0x2d394b,
            hover: 0x41506a,
            input: 0x2b3749,
            border: 0x596a82,
            text: 0xd8dee9,
            text_muted: 0x81a1c1,
            text_dimmed: 0x7b88a1,
            primary: 0x88c0d0,
            primary_hover: 0x7bb8c9,
            on_primary: 0x2e3440,
            focus_ring: 0x88c0d0,
            link: 0x81a1c1,
            accent: 0x5e81ac,
            success: 0xa3be8c,
            warning: 0xebcb8b,
            danger: 0xbf616a,
            terminal_bg: 0x202a38,
            terminal_fg: 0xd8dee9,
            terminal_cursor: 0xd8dee9,
            terminal_selection: 0x425777,
            terminal_ansi: [
                0x3b4252, 0xe78284, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0,
                0x8b94a8, 0xf08a93, 0xb1cc99, 0xf1d59d, 0x93b4d6, 0xc59cbd, 0x9bd3e4, 0xeceff4,
            ],
        },
        "monokai-pro" => ThemePalette {
            bg: 0x2f271f,
            surface: 0x24201b,
            surface_elevated: 0x44392b,
            section_header: 0x342b22,
            hover: 0x44392b,
            input: 0x25211c,
            border: 0x5a4b39,
            text: 0xfcfcfa,
            text_muted: 0xc1c0c0,
            text_dimmed: 0x8a8171,
            primary: 0xffd866,
            primary_hover: 0xe6c25e,
            on_primary: 0x1d2021,
            focus_ring: 0xffd866,
            link: 0x78dce8,
            accent: 0x78dce8,
            success: 0xa9dc76,
            warning: 0xffd866,
            danger: 0xff6188,
            terminal_bg: 0x2b241d,
            terminal_fg: 0xfcfcfa,
            terminal_cursor: 0xfcfcfa,
            terminal_selection: 0x564735,
            terminal_ansi: [
                0x403e41, 0xff6188, 0xa9dc76, 0xffd866, 0x66d9ef, 0xab9df2, 0x78dce8, 0xfcfcfa,
                0x727072, 0xff7b9a, 0xbdef83, 0xffe385, 0x7ee0f5, 0xbfaef7, 0x8ce7f2, 0xffffff,
            ],
        },
        "solarized-light" => ThemePalette {
            bg: 0xfff4d6,
            surface: 0xf1dfb8,
            surface_elevated: 0xe7d19f,
            section_header: 0xf6e7c4,
            hover: 0xe7d19f,
            input: 0xfff7df,
            border: 0xc9a86a,
            text: 0x586e75,
            text_muted: 0x93a1a1,
            text_dimmed: 0x839496,
            primary: 0x268bd2,
            primary_hover: 0x1e7abc,
            on_primary: 0x002b36,
            focus_ring: 0x268bd2,
            link: 0x268bd2,
            accent: 0x2aa198,
            success: 0x859900,
            warning: 0xb58900,
            danger: 0xdc322f,
            terminal_bg: 0xfff7df,
            terminal_fg: 0x586e75,
            terminal_cursor: 0x586e75,
            terminal_selection: 0xead6a7,
            terminal_ansi: [
                0x073642, 0xc42b2b, 0x667900, 0x7a5f00, 0x006fb3, 0xb12a6d, 0x007f79, 0x657b83,
                0x002b36, 0xa91f1f, 0x556b00, 0x665100, 0x005f9e, 0x962057, 0x006b66, 0x586e75,
            ],
        },
        "catppuccin-mocha" => ThemePalette {
            bg: 0x221827,
            surface: 0x1a1420,
            surface_elevated: 0x33263c,
            section_header: 0x24192b,
            hover: 0x33263c,
            input: 0x1d1624,
            border: 0x4d3a57,
            text: 0xcdd6f4,
            text_muted: 0xa6adc8,
            text_dimmed: 0x6c7086,
            primary: 0xcba6f7,
            primary_hover: 0xb48bf2,
            on_primary: 0x1e1e2e,
            focus_ring: 0xcba6f7,
            link: 0x89b4fa,
            accent: 0xf5c2e7,
            success: 0xa6e3a1,
            warning: 0xf9e2af,
            danger: 0xf38ba8,
            terminal_bg: 0x1e1324,
            terminal_fg: 0xcdd6f4,
            terminal_cursor: 0xf5e0dc,
            terminal_selection: 0x4a3758,
            terminal_ansi: [
                0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de,
                0x585b70, 0xf5a0b7, 0xb6f0b2, 0xfbe8c0, 0x9cc5ff, 0xf7cfed, 0xa6eee2, 0xcdd6f4,
            ],
        },
        "tokyo-night" => ThemePalette {
            bg: 0x111827,
            surface: 0x0f1524,
            surface_elevated: 0x1f2d4d,
            section_header: 0x151e33,
            hover: 0x1f2d4d,
            input: 0x111a2d,
            border: 0x31436d,
            text: 0xa9b1d6,
            text_muted: 0x737aa2,
            text_dimmed: 0x565f89,
            primary: 0x7aa2f7,
            primary_hover: 0x6690e6,
            on_primary: 0x1a1b26,
            focus_ring: 0x7aa2f7,
            link: 0x7dcfff,
            accent: 0xbb9af7,
            success: 0x9ece6a,
            warning: 0xe0af68,
            danger: 0xf7768e,
            terminal_bg: 0x0b1020,
            terminal_fg: 0xa9b1d6,
            terminal_cursor: 0xc0caf5,
            terminal_selection: 0x294f8d,
            terminal_ansi: [
                0x414868, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xc0caf5,
                0x565f89, 0xff8ea3, 0xb9f27c, 0xf4c980, 0x8db8ff, 0xc7a9ff, 0x9ae6ff, 0xd5dcff,
            ],
        },
        "one-dark-pro" => ThemePalette {
            bg: 0x252a32,
            surface: 0x1f242b,
            surface_elevated: 0x333b47,
            section_header: 0x292f38,
            hover: 0x333b47,
            input: 0x212730,
            border: 0x4b5563,
            text: 0xabb2bf,
            text_muted: 0x7f848e,
            text_dimmed: 0x5c6370,
            primary: 0x61afef,
            primary_hover: 0x519fd9,
            on_primary: 0x282c34,
            focus_ring: 0x61afef,
            link: 0x61afef,
            accent: 0xc678dd,
            success: 0x98c379,
            warning: 0xe5c07b,
            danger: 0xe06c75,
            terminal_bg: 0x222831,
            terminal_fg: 0xabb2bf,
            terminal_cursor: 0x528bff,
            terminal_selection: 0x465163,
            terminal_ansi: [
                0x3f4451, 0xef7d86, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
                0x8b929e, 0xef7d86, 0x98c379, 0xd19a66, 0x61afef, 0xc678dd, 0x56b6c2, 0xe6e6e6,
            ],
        },
        "rose-pine" => ThemePalette {
            bg: 0x21121d,
            surface: 0x2b1a28,
            surface_elevated: 0x3b2638,
            section_header: 0x281826,
            hover: 0x3b2638,
            input: 0x261727,
            border: 0x5b4259,
            text: 0xe0def4,
            text_muted: 0x908caa,
            text_dimmed: 0x6e6a86,
            primary: 0xc4a7e7,
            primary_hover: 0xb195d9,
            on_primary: 0x191724,
            focus_ring: 0xc4a7e7,
            link: 0x9ccfd8,
            accent: 0xebbcba,
            success: 0x9ccfd8,
            warning: 0xf6c177,
            danger: 0xeb6f92,
            terminal_bg: 0x1d101b,
            terminal_fg: 0xe0def4,
            terminal_cursor: 0x524f67,
            terminal_selection: 0x4c3449,
            terminal_ansi: [
                0x26233a, 0xeb6f92, 0x9ccfd8, 0xf6c177, 0x9ccfd8, 0xc4a7e7, 0x5db0c4, 0xe0def4,
                0x6e6a86, 0xff86a4, 0xb7e3ea, 0xffd18f, 0xb7e3ea, 0xd3b7f3, 0x5db0c4, 0xf2e9f6,
            ],
        },
        "gruvbox-dark" => ThemePalette {
            bg: 0x2b2118,
            surface: 0x211b14,
            surface_elevated: 0x443322,
            section_header: 0x302417,
            hover: 0x443322,
            input: 0x241d15,
            border: 0x604832,
            text: 0xebdbb2,
            text_muted: 0xa89984,
            text_dimmed: 0x7c6f64,
            primary: 0xfabd2f,
            primary_hover: 0xe5ab27,
            on_primary: 0x1d2021,
            focus_ring: 0xfabd2f,
            link: 0x83a598,
            accent: 0x8ec07c,
            success: 0xb8bb26,
            warning: 0xfabd2f,
            danger: 0xfb4934,
            terminal_bg: 0x271d14,
            terminal_fg: 0xebdbb2,
            terminal_cursor: 0xebdbb2,
            terminal_selection: 0x5a4129,
            terminal_ansi: [
                0x3c3836, 0xff6f61, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xd5c4a1,
                0x928374, 0xff5f52, 0xc7d94c, 0xffd75f, 0x9bbdb0, 0xe19ab0, 0xa3d39c, 0xfbf1c7,
            ],
        },
        "github-light" => ThemePalette {
            bg: 0xffffff,
            surface: 0xf2f5f8,
            surface_elevated: 0xe6edf5,
            section_header: 0xeef3f8,
            hover: 0xe6edf5,
            input: 0xf7f9fb,
            border: 0xc3ccd6,
            text: 0x1f2328,
            text_muted: 0x656d76,
            text_dimmed: 0x8b949e,
            primary: 0x0969da,
            primary_hover: 0x0550ae,
            on_primary: 0xffffff,
            focus_ring: 0x0969da,
            link: 0x0969da,
            accent: 0x1a7f37,
            success: 0x1a7f37,
            warning: 0x9a6700,
            danger: 0xcf222e,
            terminal_bg: 0xffffff,
            terminal_fg: 0x1f2328,
            terminal_cursor: 0x0969da,
            terminal_selection: 0xc7e5ff,
            terminal_ansi: [
                0x1f2328, 0xcf222e, 0x116329, 0x9a6700, 0x0969da, 0x8250df, 0x1b7c83, 0x6e7781,
                0x57606a, 0xa40e26, 0x1a7f37, 0x7d4e00, 0x218bff, 0xa475f9, 0x3192aa, 0x8c959f,
            ],
        },
        "catppuccin-latte" => ThemePalette {
            bg: 0xf3effa,
            surface: 0xe6e0f0,
            surface_elevated: 0xd8cee8,
            section_header: 0xebe4f4,
            hover: 0xd8cee8,
            input: 0xede7f4,
            border: 0xb8abc9,
            text: 0x4c4f69,
            text_muted: 0x6c6f85,
            text_dimmed: 0x8c8fa1,
            primary: 0x8839ef,
            primary_hover: 0x7528d4,
            on_primary: 0xffffff,
            focus_ring: 0x8839ef,
            link: 0x1e66f5,
            accent: 0xea76cb,
            success: 0x40a02b,
            warning: 0xdf8e1d,
            danger: 0xd20f39,
            terminal_bg: 0xf7f3fb,
            terminal_fg: 0x4c4f69,
            terminal_cursor: 0xdc8a78,
            terminal_selection: 0xd9cdea,
            terminal_ansi: [
                0x5c5f77, 0xd20f39, 0x2d7f1f, 0x9a5d00, 0x1558d6, 0xb13c9a, 0x0b747b, 0x6c6f85,
                0x6c6f85, 0xd20f39, 0x2d7f1f, 0x9a5d00, 0x1558d6, 0xb13c9a, 0x0b747b, 0x4c4f69,
            ],
        },
        "rose-pine-dawn" => ThemePalette {
            bg: 0xfff0e8,
            surface: 0xf7dfd2,
            surface_elevated: 0xebcdbf,
            section_header: 0xf7e5dc,
            hover: 0xebcdbf,
            input: 0xfff8f2,
            border: 0xd8b3a5,
            text: 0x575279,
            text_muted: 0x797593,
            text_dimmed: 0x817c96,
            primary: 0x907aa9,
            primary_hover: 0x7d6899,
            on_primary: 0x191724,
            focus_ring: 0x907aa9,
            link: 0x286983,
            accent: 0xd7827e,
            success: 0x56949f,
            warning: 0xea9d34,
            danger: 0xb4637a,
            terminal_bg: 0xfff6ef,
            terminal_fg: 0x575279,
            terminal_cursor: 0x575279,
            terminal_selection: 0xebcbbf,
            terminal_ansi: [
                0x575279, 0x9f4a61, 0x35717a, 0x9a6200, 0x3d7699, 0x7b6098, 0x286983, 0x797593,
                0x797593, 0x8f3f56, 0x35717a, 0x805000, 0x326985, 0x6f548a, 0x1f5e75, 0x575279,
            ],
        },
        "nord-light" => ThemePalette {
            bg: 0xeef6fb,
            surface: 0xdce8f2,
            surface_elevated: 0xc9d9e7,
            section_header: 0xe2edf5,
            hover: 0xc9d9e7,
            input: 0xe6f0f7,
            border: 0xadc0d2,
            text: 0x2e3440,
            text_muted: 0x4c566a,
            text_dimmed: 0x7b88a1,
            primary: 0x5e81ac,
            primary_hover: 0x4e6f96,
            on_primary: 0xffffff,
            focus_ring: 0x5e81ac,
            link: 0x5e81ac,
            accent: 0x88c0d0,
            success: 0x5e8f57,
            warning: 0xb7791f,
            danger: 0xbf616a,
            terminal_bg: 0xf4f9fd,
            terminal_fg: 0x2e3440,
            terminal_cursor: 0x5e81ac,
            terminal_selection: 0xc8d8e8,
            terminal_ansi: [
                0x2e3440, 0x9f4f59, 0x3f7a38, 0x8a5b00, 0x4e6f96, 0x76566f, 0x3c6f7d, 0x4c566a,
                0x4c566a, 0x8d424b, 0x346b2e, 0x734b00, 0x405f82, 0x76566f, 0x3c6f7d, 0x2e3440,
            ],
        },
        "one-light" => ThemePalette {
            bg: 0xfafafa,
            surface: 0xececec,
            surface_elevated: 0xdedfe3,
            section_header: 0xeeeeef,
            hover: 0xdedfe3,
            input: 0xf4f4f5,
            border: 0xc5c6cc,
            text: 0x383a42,
            text_muted: 0x696c77,
            text_dimmed: 0x7f848e,
            primary: 0x4078f2,
            primary_hover: 0x2f65de,
            on_primary: 0xffffff,
            focus_ring: 0x4078f2,
            link: 0x4078f2,
            accent: 0xa626a4,
            success: 0x50a14f,
            warning: 0xc18401,
            danger: 0xca1243,
            terminal_bg: 0xfafafa,
            terminal_fg: 0x383a42,
            terminal_cursor: 0x526eff,
            terminal_selection: 0xdfe3ea,
            terminal_ansi: [
                0x383a42, 0xb74137, 0x367d35, 0x8d5c00, 0x2f65de, 0xa626a4, 0x0184bc, 0x696c77,
                0x696c77, 0xb74137, 0x367d35, 0x8d5c00, 0x2f65de, 0x8f1f8d, 0x006f9e, 0x383a42,
            ],
        },
        _ => ThemePalette {
            bg: 0x0d1117,
            surface: 0x161b22,
            surface_elevated: 0x21262d,
            section_header: 0x12171f,
            hover: 0x1c2128,
            input: 0x0d1117,
            border: 0x30363d,
            text: 0xc9d1d9,
            text_muted: 0x8b949e,
            text_dimmed: 0x6e7681,
            primary: 0x3b82f6,
            primary_hover: 0x2563eb,
            on_primary: 0x0d1117,
            focus_ring: 0x58a6ff,
            link: 0x58a6ff,
            accent: 0x58a6ff,
            success: 0x3fb950,
            warning: 0xd29922,
            danger: 0xff7b72,
            terminal_bg: 0x0d1117,
            terminal_fg: 0xc9d1d9,
            terminal_cursor: 0xc9d1d9,
            terminal_selection: 0x264f78,
            terminal_ansi: [
                0x484f58, 0xff7b72, 0x3fb950, 0xd29922, 0x58a6ff, 0xbc8cff, 0x39c5cf, 0xb1bac4,
                0x8b949e, 0xffa198, 0x3fb950, 0xe3b341, 0x79c0ff, 0xd2a8ff, 0x56d4dd, 0xf0f6fc,
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::theme_palette;

    #[test]
    fn semantic_colors_match_tauri_theme_tokens() {
        let expected = [
            (
                "github-dark",
                0x3b82f6,
                0x2563eb,
                0x0d1117,
                0x58a6ff,
                0x58a6ff,
                0x58a6ff,
            ),
            (
                "nya-high-contrast",
                0x22d3ee,
                0x06b6d4,
                0x001116,
                0x67e8f9,
                0x67e8f9,
                0xf472b6,
            ),
            (
                "nya-high-contrast-white",
                0x004e89,
                0x003a66,
                0xffffff,
                0x0067b8,
                0x075985,
                0x6d28d9,
            ),
            (
                "dracula", 0xbd93f9, 0xa77bfa, 0x282a36, 0xbd93f9, 0x8be9fd, 0xff79c6,
            ),
            (
                "nord", 0x88c0d0, 0x7bb8c9, 0x2e3440, 0x88c0d0, 0x81a1c1, 0x5e81ac,
            ),
            (
                "monokai-pro",
                0xffd866,
                0xe6c25e,
                0x1d2021,
                0xffd866,
                0x78dce8,
                0x78dce8,
            ),
            (
                "solarized-light",
                0x268bd2,
                0x1e7abc,
                0x002b36,
                0x268bd2,
                0x268bd2,
                0x2aa198,
            ),
            (
                "catppuccin-mocha",
                0xcba6f7,
                0xb48bf2,
                0x1e1e2e,
                0xcba6f7,
                0x89b4fa,
                0xf5c2e7,
            ),
            (
                "tokyo-night",
                0x7aa2f7,
                0x6690e6,
                0x1a1b26,
                0x7aa2f7,
                0x7dcfff,
                0xbb9af7,
            ),
            (
                "one-dark-pro",
                0x61afef,
                0x519fd9,
                0x282c34,
                0x61afef,
                0x61afef,
                0xc678dd,
            ),
            (
                "rose-pine",
                0xc4a7e7,
                0xb195d9,
                0x191724,
                0xc4a7e7,
                0x9ccfd8,
                0xebbcba,
            ),
            (
                "gruvbox-dark",
                0xfabd2f,
                0xe5ab27,
                0x1d2021,
                0xfabd2f,
                0x83a598,
                0x8ec07c,
            ),
            (
                "github-light",
                0x0969da,
                0x0550ae,
                0xffffff,
                0x0969da,
                0x0969da,
                0x1a7f37,
            ),
            (
                "catppuccin-latte",
                0x8839ef,
                0x7528d4,
                0xffffff,
                0x8839ef,
                0x1e66f5,
                0xea76cb,
            ),
            (
                "rose-pine-dawn",
                0x907aa9,
                0x7d6899,
                0x191724,
                0x907aa9,
                0x286983,
                0xd7827e,
            ),
            (
                "nord-light",
                0x5e81ac,
                0x4e6f96,
                0xffffff,
                0x5e81ac,
                0x5e81ac,
                0x88c0d0,
            ),
            (
                "one-light",
                0x4078f2,
                0x2f65de,
                0xffffff,
                0x4078f2,
                0x4078f2,
                0xa626a4,
            ),
        ];

        for (id, primary, hover, on_primary, focus, link, accent) in expected {
            let palette = theme_palette(id);
            assert_eq!(palette.primary, primary, "{id} primary");
            assert_eq!(palette.primary_hover, hover, "{id} primary hover");
            assert_eq!(palette.on_primary, on_primary, "{id} on primary");
            assert_eq!(palette.focus_ring, focus, "{id} focus ring");
            assert_eq!(palette.link, link, "{id} link");
            assert_eq!(palette.accent, accent, "{id} accent");
        }
    }
}
