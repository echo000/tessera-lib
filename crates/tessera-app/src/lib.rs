//! Tessera application shell built on [gpui](https://crates.io/crates/gpui).
//!
//! This is an iced-free port of `porter-app`, keeping a similar public api and
//! the same look and feel.

pub(crate) mod assets;
pub(crate) mod audio_player;
pub(crate) mod components;
pub(crate) mod panic_hook;
pub(crate) mod strings;
pub(crate) mod system;
pub(crate) mod update;

/// Shared application theming (dark/light variants).
pub mod theme;

mod app;
mod app_state;
mod asset;
mod asset_column;
mod asset_context;
mod asset_exporter;
mod asset_manager;
mod asset_manager_task;
mod asset_preview;
mod asset_preview_request;
mod asset_state;
mod asset_status;
mod asset_task;
mod asset_task_error;
mod asset_task_options;
mod column_status;
mod controller;
mod icon;
mod search;
mod settings;
mod sort;

pub(crate) use app::TesseraApp;

/// Shared application palette and colors for ui elements.
pub mod palette;

pub use app_state::*;
pub use asset::*;
pub use asset_column::*;
pub use asset_context::*;
pub use asset_exporter::*;
pub use asset_manager::*;
pub use asset_manager_task::*;
pub use asset_preview::*;
pub use asset_preview_request::*;
pub use asset_state::*;
pub use asset_status::*;
pub use asset_task::*;
pub use asset_task_error::*;
pub use asset_task_options::*;
pub use column_status::*;
pub use controller::*;
pub use icon::*;
pub use search::*;
pub use settings::*;
pub use sort::*;
pub use theme::ThemeVariant;

/// Re-exported for use in public interfaces.
pub use gpui::Rgba as Color;
pub use tessera_viewport::MaterialBlend;
pub use tessera_viewport::MaterialSettings;
pub use tessera_viewport::ModelMaterial;

/// Initializes a new app state with the given asset manager.
pub fn initialize<A: AssetManager + 'static>(asset_manager: A) -> AppState {
    AppState::new(asset_manager)
}
