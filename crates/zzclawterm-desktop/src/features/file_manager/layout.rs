use gpui::{Pixels, px};
use zzclawterm_core::models::settings::FileManagerPaneUiSettings;

use super::pane::{BrowserPaneState, PaneEndpoint};
use crate::models::{TransferBrowserSortColumn, TransferBrowserSortDirection};

pub(super) const COLUMNS: [TransferBrowserSortColumn; 6] = [
    TransferBrowserSortColumn::Name,
    TransferBrowserSortColumn::Size,
    TransferBrowserSortColumn::Modified,
    TransferBrowserSortColumn::Permissions,
    TransferBrowserSortColumn::Owner,
    TransferBrowserSortColumn::Group,
];

pub(super) fn column_key(column: TransferBrowserSortColumn) -> &'static str {
    match column {
        TransferBrowserSortColumn::Name => "name",
        TransferBrowserSortColumn::Size => "size",
        TransferBrowserSortColumn::Modified => "modified",
        TransferBrowserSortColumn::Permissions => "permissions",
        TransferBrowserSortColumn::Owner => "owner",
        TransferBrowserSortColumn::Group => "group",
    }
}

impl BrowserPaneState {
    pub(super) fn restore_view_settings(&mut self, settings: &FileManagerPaneUiSettings) {
        for column in COLUMNS {
            if let Some(width) = settings.column_widths.get(column_key(column)) {
                self.column_widths
                    .set(column, px((*width).clamp(72, 2000) as f32));
            }
        }
        self.visible_columns = settings.visible_columns.clone();
        if !self.visible_columns.iter().any(|key| key == "name") {
            self.visible_columns.insert(0, "name".into());
        }
        self.sort_column = COLUMNS
            .into_iter()
            .find(|column| column_key(*column) == settings.sort_column)
            .unwrap_or(TransferBrowserSortColumn::Name);
        self.sort_direction = if settings.descending {
            TransferBrowserSortDirection::Descending
        } else {
            TransferBrowserSortDirection::Ascending
        };
        self.row_height = if settings.row_height == 28 { 28 } else { 30 };
        self.remembered_directories = settings.directories.clone();
    }

    pub(super) fn view_settings(&self) -> FileManagerPaneUiSettings {
        FileManagerPaneUiSettings {
            column_widths: COLUMNS
                .into_iter()
                .map(|column| {
                    (
                        column_key(column).into(),
                        f32::from(self.column_widths.get(column)).round() as u32,
                    )
                })
                .collect(),
            visible_columns: self.visible_columns.clone(),
            sort_column: column_key(self.sort_column).into(),
            descending: self.sort_direction == TransferBrowserSortDirection::Descending,
            row_height: self.row_height,
            directories: self.remembered_directories.clone(),
        }
    }

    pub(super) fn table_columns(&self, viewport: f32) -> Vec<(TransferBrowserSortColumn, Pixels)> {
        let mut columns: Vec<_> = COLUMNS
            .into_iter()
            .filter(|column| {
                (*column == TransferBrowserSortColumn::Name
                    || self
                        .visible_columns
                        .iter()
                        .any(|key| key == column_key(*column)))
                    && !(self.endpoint == PaneEndpoint::Local
                        && *column == TransferBrowserSortColumn::Permissions)
            })
            .map(|column| (column, self.column_widths.get(column)))
            .collect();
        let other: f32 = columns
            .iter()
            .filter(|(column, _)| *column != TransferBrowserSortColumn::Name)
            .map(|(_, width)| f32::from(*width))
            .sum();
        if let Some((_, name_width)) = columns.first_mut() {
            *name_width = px((viewport - other).max(f32::from(*name_width)));
        }
        columns
    }
}
