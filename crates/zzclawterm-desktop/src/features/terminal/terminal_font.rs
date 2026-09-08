use gpui::{Font, FontFallbacks, SharedString, font};

/// Families that GPUI resolves to a platform-specific concrete font.
pub(in crate::features) fn is_generic_terminal_font_family(family: &str) -> bool {
    let family = family.trim();
    family.eq_ignore_ascii_case("monospace")
        || family.eq_ignore_ascii_case("system-monospace")
        || family.eq_ignore_ascii_case("ui-monospace")
}

/// Resolved terminal font descriptor retaining the primary family and ordered fallbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::features) struct ResolvedAppearanceFont {
    pub family: String,
    pub fallbacks: Option<FontFallbacks>,
    /// Ordered family names retained from user configuration so runtime promotion
    /// can rebuild the complete descriptor.
    pub fallback_families: Vec<String>,
}

impl ResolvedAppearanceFont {
    pub(in crate::features) fn font(&self) -> Font {
        let mut font = font(SharedString::from(self.family.clone()));
        font.fallbacks = self.fallbacks.clone();
        font
    }

    pub(in crate::features) fn with_primary_family(&self, family: &str) -> Self {
        let fallback_families = self
            .fallback_families
            .iter()
            .filter(|candidate| !candidate.eq_ignore_ascii_case(family))
            .cloned()
            .collect::<Vec<_>>();
        let fallbacks = (!fallback_families.is_empty())
            .then(|| FontFallbacks::from_fonts(fallback_families.clone()));
        Self {
            family: family.to_string(),
            fallbacks,
            fallback_families,
        }
    }
}

/// Measured metrics and the concrete family selected by GPUI.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::features) struct TerminalFontMeasurement {
    pub(in crate::features) cell_width: f32,
    pub(in crate::features) resolved_family: String,
}

/// Reason why a terminal font could not be validated for fixed-width painting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::features) enum TerminalFontMeasurementFailure {
    FontNotResolved,
    ResolvedFamilyMismatch,
    MissingGlyphAdvance,
    NotMonospaced,
    ShapingMismatch,
}
