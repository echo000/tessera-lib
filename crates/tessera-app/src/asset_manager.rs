use std::collections::BTreeSet;
use std::path::PathBuf;

use gpui::Rgba as Color;

use crate::ColumnStatus;
use crate::Controller;
use crate::Icon;
use crate::SearchTerm;
use crate::Settings;

/// Column-level filters applied after search and before sorting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetColumnFilters {
    /// Allowed asset type labels. `None` means all types.
    pub types: Option<BTreeSet<String>>,
    /// Allowed asset status labels. `None` means all statuses.
    pub statuses: Option<BTreeSet<String>>,
}

impl AssetColumnFilters {
    /// Whether no column filter is active.
    pub fn is_empty(&self) -> bool {
        self.types.is_none() && self.statuses.is_none()
    }
}

/// Asset manager handles loading, exporting and management of game assets.
pub trait AssetManager: Send + Sync + 'static {
    /// Whether or not the asset manager supports loading game files on disk.
    fn supports_files(&self) -> bool {
        false
    }

    /// Whether or not the asset manager supports loading from the game directly.
    fn supports_games(&self) -> bool {
        false
    }

    /// Whether loading selects a directory instead of individual files.
    fn loads_folders(&self) -> bool {
        false
    }

    /// Gets information about the specific asset, in the form of column data.
    fn assets_info(&self, index: usize) -> Vec<(String, Option<Color>)>;

    /// The number of visible assets, whether they are search results, or just loaded.
    fn assets_visible(&self) -> usize;

    /// The total number of assets loaded.
    fn assets_total(&self) -> usize;

    /// Whether or not there are visible assets.
    fn assets_empty(&self) -> bool {
        self.assets_visible() == 0
    }

    /// Search for assets, or reset the search term.
    fn search(&self, term: Option<SearchTerm>);

    /// Sets column filters. Filters are applied on top of the current search result.
    fn set_column_filters(&self, filters: AssetColumnFilters) {
        let _ = filters;
    }

    /// Sort assets based on column status, returns the new column sort statuses.
    fn sort(&self, column: Option<usize>, statuses: Vec<ColumnStatus>) -> Vec<ColumnStatus> {
        let _ = column;
        let _ = statuses;

        Vec::new()
    }

    /// Loads one or more given files.
    fn load_files(&self, settings: Settings, files: Vec<PathBuf>) -> Result<(), String>;

    /// Loads a running game instance.
    fn load_game(&self, _settings: Settings) -> Result<(), String> {
        Err("Loading running games is not supported by this backend".into())
    }

    /// Optional icon to display as an indicator on the main window.
    ///
    /// This icon is refreshed in two different ways:
    /// - Once when the application initially loads.
    /// - Whenever a call to `load_files` or `load_game` finishes.
    fn display_icon(&self) -> Option<Icon> {
        None
    }

    /// Request one or more assets be exported.
    fn export(&self, settings: Settings, controller: Controller, assets: Vec<usize>);

    /// Cancels an active export.
    fn export_cancel(&self);

    /// Request the given assets data for preview, optionally forcing a raw file preview.
    fn preview(
        &self,
        settings: Settings,
        controller: Controller,
        asset: usize,
        raw: bool,
        request_id: u64,
    );

    /// Gets the export subdirectory for the current game.
    fn game_export_sub_dir(&self, settings: &Settings) -> Option<PathBuf> {
        let _ = settings;
        None
    }

    /// Searches all loaded model assets by name, for binding an animation
    /// preview to a model. Returns pairs of a stable model id (as accepted by
    /// [`AssetManager::preview_animation_model`]) and asset name, up to
    /// `limit` matches. An empty query matches every model.
    fn animation_preview_models(&self, query: &str, limit: usize) -> Vec<(u64, String)> {
        let _ = (query, limit);

        Vec::new()
    }

    /// Resolves a visible row index to the model id accepted by
    /// [`AssetManager::preview_animation_model`]. Returns `None` when that row
    /// is not a model that can back an animation preview, which the row context
    /// menu uses to decide whether to offer the option at all.
    fn animation_preview_model_at(&self, index: usize) -> Option<u64> {
        let _ = index;

        None
    }

    /// Requests the given model's data for binding to an animation preview,
    /// by an id returned from [`AssetManager::animation_preview_models`].
    /// Responds with [`crate::AssetPreview::Model`] or
    /// [`crate::AssetPreview::ModelWithMaterials`] for material rendering options.
    fn preview_animation_model(
        &self,
        settings: Settings,
        model: u64,
        request_id: u64,
        controller: Controller,
    ) {
        let _ = (settings, model);

        controller.preview_update(request_id, crate::AssetPreview::PreviewError);
    }
}
