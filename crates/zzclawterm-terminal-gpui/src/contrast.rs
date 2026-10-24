//! Presentation-only contrast correction. Terminal colors remain unmodified.

use std::collections::{HashMap, VecDeque};

use zzclawterm_terminal::{terminal_char_cell_width, terminal_is_zero_width_mark};
use zzclawterm_ui::ThemePalette;

use crate::types::TerminalHighlightSpan;

const CONTRAST_CACHE_CAP: usize = 4096;

pub(super) fn normalize_ratio(ratio: f32) -> f32 {
    if ratio.is_finite() {
        ratio.clamp(1.0, 21.0)
    } else {
        1.0
    }
}

fn luminance(rgb: u32) -> f64 {
    let channel = |shift: u32| {
        let value = ((rgb >> shift) & 255) as f64 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

pub(super) fn contrast_ratio(fg: u32, bg: u32) -> f32 {
    let (a, b) = (luminance(fg), luminance(bg));
    ((a.max(b) + 0.05) / (a.min(b) + 0.05)) as f32
}

fn mix(fg: u32, target: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let a = ((fg >> shift) & 255) as f32;
        let b = ((target >> shift) & 255) as f32;
        (a + (b - a) * amount).round() as u32
    };
    channel(16) << 16 | channel(8) << 8 | channel(0)
}

fn candidate(fg: u32, bg: u32, target: u32, ratio: f32) -> Option<u32> {
    if contrast_ratio(target, bg) < ratio {
        return None;
    }
    let (mut low, mut high) = (0.0, 1.0);
    // Keep the passing endpoint, including after RGB quantization.
    let mut best = target;
    for _ in 0..16 {
        let mid = (low + high) / 2.0;
        let rgb = mix(fg, target, mid);
        if contrast_ratio(rgb, bg) >= ratio {
            high = mid;
            best = rgb;
        } else {
            low = mid;
        }
    }
    Some(best)
}

fn distance(a: u32, b: u32) -> u32 {
    [16, 8, 0]
        .into_iter()
        .map(|shift| {
            let delta = ((a >> shift) & 255) as i32 - ((b >> shift) & 255) as i32;
            (delta * delta) as u32
        })
        .sum()
}

fn ensure_contrast(fg: u32, bg: u32, ratio: f32) -> u32 {
    if ratio <= 1.0 || contrast_ratio(fg, bg) >= ratio {
        return fg;
    }
    match (
        candidate(fg, bg, 0, ratio),
        candidate(fg, bg, 0xffffff, ratio),
    ) {
        (Some(dark), Some(light)) => {
            if distance(fg, dark) <= distance(fg, light) {
                dark
            } else {
                light
            }
        }
        (Some(color), None) | (None, Some(color)) => color,
        (None, None) => {
            if contrast_ratio(0, bg) >= contrast_ratio(0xffffff, bg) {
                0
            } else {
                0xffffff
            }
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct TerminalContrastCache {
    colors: HashMap<(u32, u32, u32), u32>,
    order: VecDeque<(u32, u32, u32)>,
}

impl TerminalContrastCache {
    pub(super) fn resolve(&mut self, fg: u32, bg: u32, ratio: f32) -> u32 {
        let ratio = normalize_ratio(ratio);
        if ratio <= 1.0 {
            return fg;
        }
        let key = (fg, bg, ratio.to_bits());
        if let Some(color) = self.colors.get(&key) {
            return *color;
        }
        let color = ensure_contrast(fg, bg, ratio);
        if self.colors.len() == CONTRAST_CACHE_CAP {
            let oldest = self.order.pop_front().expect("cached contrast color");
            self.colors.remove(&oldest);
        }
        self.colors.insert(key, color);
        self.order.push_back(key);
        color
    }
}

#[derive(Default)]
pub(super) struct TerminalContrastOverlays<'a> {
    pub(super) occurrences: &'a [(usize, usize)],
    pub(super) search: &'a [(usize, usize)],
    pub(super) active_search: &'a [(usize, usize)],
    pub(super) selection: Option<(usize, usize)>,
}

impl TerminalContrastOverlays<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
            && self.search.is_empty()
            && self.active_search.is_empty()
            && self.selection.is_none()
    }

    pub(super) fn background(&self, col: usize, base: u32, palette: ThemePalette) -> u32 {
        let contains = |ranges: &[(usize, usize)]| {
            ranges.iter().any(|&(start, end)| start <= col && col < end)
        };
        if self
            .selection
            .is_some_and(|(start, end)| start <= col && col < end)
        {
            palette.terminal_selection
        } else if contains(self.search) || contains(self.active_search) {
            palette.terminal_find_bg
        } else {
            // Match the repeated-occurrence wash, including overlapping ranges.
            self.occurrences
                .iter()
                .filter(|&&(start, end)| start <= col && col < end)
                .fold(base, |bg, _| mix(bg, palette.text_muted, 88.0 / 255.0))
        }
    }
}

pub(super) fn contrast_excluded(ch: char) -> bool {
    ('\u{2500}'..='\u{259f}').contains(&ch) || ('\u{e0a0}'..='\u{e0d7}').contains(&ch)
}

/// Run once after foreground styling. These spans are for glyphs only; protocol
/// backgrounds must be collected from the unmodified spans or snapshot cells.
pub(super) fn apply_contrast(
    spans: &[TerminalHighlightSpan],
    palette: ThemePalette,
    ratio: f32,
    overlays: &TerminalContrastOverlays<'_>,
    cache: &mut TerminalContrastCache,
) -> Vec<TerminalHighlightSpan> {
    let mut out = Vec::new();
    let mut col = 0;
    for span in spans {
        let mut segment_start = 0;
        let mut previous = None;
        for (offset, ch) in span.text.char_indices() {
            if terminal_is_zero_width_mark(ch) && previous.is_some() {
                continue;
            }
            let bg = overlays.background(col, span.bg.unwrap_or(palette.terminal_bg), palette);
            let key = (bg, contrast_excluded(ch));
            if let Some(previous) = previous
                && previous != key
            {
                push_segment(
                    &mut out,
                    span,
                    segment_start,
                    offset,
                    previous,
                    palette,
                    ratio,
                    cache,
                );
                segment_start = offset;
            }
            previous = Some(key);
            col += terminal_char_cell_width(ch);
        }
        if let Some(key) = previous {
            push_segment(
                &mut out,
                span,
                segment_start,
                span.text.len(),
                key,
                palette,
                ratio,
                cache,
            );
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn push_segment(
    out: &mut Vec<TerminalHighlightSpan>,
    span: &TerminalHighlightSpan,
    start: usize,
    end: usize,
    (bg, excluded): (u32, bool),
    palette: ThemePalette,
    ratio: f32,
    cache: &mut TerminalContrastCache,
) {
    let mut result = TerminalHighlightSpan {
        text: span.text[start..end].to_string(),
        color: span.color,
        bg: span.bg,
        keyword: span.keyword,
        underline: span.underline,
        strikeout: span.strikeout,
        bold: span.bold,
        italic: span.italic,
        hidden: span.hidden,
    };
    let fg = span.color.unwrap_or(palette.terminal_fg);
    result.color = Some(if span.hidden {
        bg
    } else if excluded {
        fg
    } else {
        cache.resolve(fg, bg, ratio)
    });
    result.bg = Some(bg);
    out.push(result);
}

#[cfg(test)]
mod tests;
