use serde::Deserialize;
use serde::Serialize;

/// Control scheme for the preview viewport.
///
/// Serialized variant names match the porter-viewport enum this replaces, so
/// existing saved settings keep working.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum PreviewControlScheme {
    #[default]
    Simple,
    Maya,
    Blender,
    /// Unreal Engine style: hold right mouse to look and fly with WASD/QE.
    Unreal,
}

/// The current key and mouse button state for a viewport interaction.
#[derive(Default)]
pub struct ViewportKeyState {
    pub control_scheme: PreviewControlScheme,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub alt: bool,
    pub shift: bool,
}
