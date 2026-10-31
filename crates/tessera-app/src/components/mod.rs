mod asset_filter;
mod asset_table;
mod button;
mod controls;
mod icon;
mod layout;
mod popover;
mod preview_pane;
mod progress;
mod root;
mod scrollbar;
mod settings_pane;
mod table;
mod text_input;
mod text_view;
pub(crate) mod title_bar;
mod traits;

pub(crate) use asset_filter::*;
pub(crate) use asset_table::*;
pub(crate) use button::*;
pub(crate) use controls::*;
pub(crate) use icon::*;
pub(crate) use layout::*;
pub(crate) use popover::*;
pub(crate) use preview_pane::*;
pub(crate) use progress::*;
pub(crate) use root::*;
pub(crate) use scrollbar::*;
pub(crate) use table::*;
pub(crate) use text_input::Input;
pub(crate) use text_input::InputEvent;
pub(crate) use text_input::InputState;
pub(crate) use text_view::*;
pub(crate) use traits::*;

/// Registers the key bindings of the components.
pub(crate) fn bind_keys(cx: &mut gpui::App) {
    text_input::bind_keys(cx);
}
