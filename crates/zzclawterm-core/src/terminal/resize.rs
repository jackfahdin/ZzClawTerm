#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalResizeGeometry {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalViewportInsets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl TerminalViewportInsets {
    pub const fn symmetric(padding: f32) -> Self {
        Self {
            left: padding,
            right: padding,
            top: padding,
            bottom: padding,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalBackendResize {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

pub fn terminal_snapped_cell_height(cell_height: f32, scale_factor: f32) -> f32 {
    let cell_height = cell_height.max(1.0);
    let scale_factor = if scale_factor.is_finite() {
        scale_factor.max(1e-3)
    } else {
        1.0
    };
    (cell_height * scale_factor).round().max(1.0) / scale_factor
}

impl TerminalBackendResize {
    pub fn new(cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) -> Self {
        Self {
            cols,
            rows,
            pixel_width,
            pixel_height,
        }
    }
}

pub fn terminal_resize_geometry_for_size(
    width: f32,
    height: f32,
    cell_width: f32,
    cell_height: f32,
    padding: f32,
    gutter_width: f32,
) -> TerminalResizeGeometry {
    terminal_resize_geometry_for_size_with_insets(
        width,
        height,
        cell_width,
        cell_height,
        TerminalViewportInsets::symmetric(padding),
        gutter_width,
    )
}

pub fn terminal_resize_geometry_for_size_with_insets(
    width: f32,
    height: f32,
    cell_width: f32,
    cell_height: f32,
    insets: TerminalViewportInsets,
    gutter_width: f32,
) -> TerminalResizeGeometry {
    terminal_resize_geometry_for_size_with_insets_and_scale(
        width,
        height,
        cell_width,
        cell_height,
        insets,
        gutter_width,
        1.0,
    )
}

/// Calculate the grid from the same device-pixel-snapped viewport used by the painter.
pub fn terminal_resize_geometry_for_size_with_insets_and_scale(
    width: f32,
    height: f32,
    cell_width: f32,
    cell_height: f32,
    insets: TerminalViewportInsets,
    gutter_width: f32,
    scale_factor: f32,
) -> TerminalResizeGeometry {
    let cell_width = cell_width.max(1.);
    let cell_height = cell_height.max(1.);
    let scale_factor = if scale_factor.is_finite() {
        scale_factor.max(1e-3)
    } else {
        1.0
    };
    let usable_width =
        (width - insets.left.max(0.) - insets.right.max(0.) - gutter_width).max(cell_width);
    let usable_height = (height - insets.top.max(0.) - insets.bottom.max(0.)).max(cell_height);
    let raw_cols = (usable_width / cell_width).next_up().floor();
    let snapped_cell_height = terminal_snapped_cell_height(cell_height, scale_factor);
    let line_height_device_px = snapped_cell_height * scale_factor;
    let available_height_device_px = (usable_height * scale_factor).floor().max(0.0);
    let raw_rows = (available_height_device_px / line_height_device_px)
        .next_up()
        .floor();
    let clamped_cols = raw_cols.clamp(20., 500.);
    let clamped_rows = raw_rows.clamp(4., 200.);
    let cols = clamped_cols as u16;
    let rows = clamped_rows as u16;
    let pixel_width = if (raw_cols - clamped_cols).abs() < f32::EPSILON {
        usable_width
    } else {
        clamped_cols * cell_width
    };
    // Keep the remainder outside the grid so the renderer and PTY never disagree by a row.
    let pixel_height = clamped_rows * line_height_device_px / scale_factor;
    let pixel_width = pixel_width.round().clamp(1., u16::MAX as f32) as u16;
    let pixel_height = pixel_height.round().clamp(1., u16::MAX as f32) as u16;
    TerminalResizeGeometry {
        cols,
        rows,
        pixel_width,
        pixel_height,
    }
}

pub fn terminal_backend_resize_changed(
    last: Option<TerminalBackendResize>,
    next: TerminalBackendResize,
) -> bool {
    last != Some(next)
}

#[cfg(test)]
mod tests {
    use super::{
        TerminalBackendResize, TerminalResizeGeometry, TerminalViewportInsets,
        terminal_backend_resize_changed, terminal_resize_geometry_for_size,
        terminal_resize_geometry_for_size_with_insets,
        terminal_resize_geometry_for_size_with_insets_and_scale,
    };

    #[test]
    fn resize_geometry_keeps_usable_pixel_remainder() {
        let geometry = terminal_resize_geometry_for_size(812., 612., 10., 20., 8., 72.);

        assert_eq!(
            geometry,
            TerminalResizeGeometry {
                cols: 72,
                rows: 29,
                pixel_width: 724,
                pixel_height: 580,
            }
        );
    }

    #[test]
    fn resize_geometry_clamps_grid_but_keeps_nonzero_pixels() {
        let geometry = terminal_resize_geometry_for_size(10., 10., 10., 20., 8., 72.);

        assert_eq!(geometry.cols, 20);
        assert_eq!(geometry.rows, 4);
        assert_eq!(geometry.pixel_width, 200);
        assert_eq!(geometry.pixel_height, 80);
    }

    #[test]
    fn resize_geometry_supports_tauri_left_only_workspace_padding() {
        let geometry = terminal_resize_geometry_for_size_with_insets(
            812.,
            612.,
            10.,
            20.,
            TerminalViewportInsets {
                left: 8.,
                right: 0.,
                top: 0.,
                bottom: 0.,
            },
            72.,
        );

        assert_eq!(geometry.cols, 73);
        assert_eq!(geometry.rows, 30);
        assert_eq!(geometry.pixel_width, 732);
        assert_eq!(geometry.pixel_height, 600);
    }

    #[test]
    fn resize_geometry_does_not_promote_a_partial_device_pixel_row() {
        let almost_24_rows = f32::from_bits((18.0f32 * 24.0).to_bits() - 1);
        let geometry = terminal_resize_geometry_for_size_with_insets(
            424.,
            almost_24_rows,
            10.,
            18.,
            TerminalViewportInsets::symmetric(0.),
            0.,
        );

        assert_eq!(geometry.cols, 42);
        assert_eq!(geometry.rows, 23);
    }

    #[test]
    fn resize_geometry_snaps_rows_at_fractional_scale() {
        let geometry = terminal_resize_geometry_for_size_with_insets_and_scale(
            812.,
            612.,
            10.,
            18.,
            TerminalViewportInsets::symmetric(0.),
            0.,
            1.25,
        );

        assert_eq!(geometry.rows, 33);
        assert_eq!(geometry.pixel_height, 607);
    }

    #[test]
    fn backend_resize_detects_pixel_only_changes() {
        let first = TerminalBackendResize::new(80, 24, 800, 432);

        assert!(terminal_backend_resize_changed(None, first));
        assert!(!terminal_backend_resize_changed(Some(first), first));
        assert!(terminal_backend_resize_changed(
            Some(first),
            TerminalBackendResize::new(80, 24, 960, 432)
        ));
        assert!(terminal_backend_resize_changed(
            Some(first),
            TerminalBackendResize::new(80, 25, 800, 450)
        ));
    }
}
