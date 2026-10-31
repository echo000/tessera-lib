use std::borrow::Cow;

use gpui::AssetSource;
use gpui::Result;
use gpui::SharedString;

use crate::components::IconName;

/// Asset source serving the svg icons embedded in tessera-app.
pub struct TesseraAssets;

impl AssetSource for TesseraAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(IconName::ALL
            .iter()
            .find(|icon| icon.path() == path)
            .map(|icon| Cow::Borrowed(icon.bytes())))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(IconName::ALL
            .iter()
            .map(|icon| icon.path())
            .filter(|icon| icon.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}
