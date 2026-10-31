#![allow(clippy::declare_interior_mutable_const)]

use std::fmt;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use gpui::Rgba;

use crate::palette;

/// The status of an asset.
#[repr(transparent)]
#[derive(Debug)]
pub struct AssetStatus {
    status: AtomicUsize,
}

impl AssetStatus {
    /// The asset was loaded.
    pub const LOADED: Self = Self {
        status: AtomicUsize::new(0),
    };

    /// The asset was exported.
    pub const EXPORTED: Self = Self {
        status: AtomicUsize::new(1),
    };

    /// The asset had an error.
    pub const ERROR: Self = Self {
        status: AtomicUsize::new(2),
    };

    /// The asset is a placeholder.
    pub const PLACEHOLDER: Self = Self {
        status: AtomicUsize::new(3),
    };

    /// The asset is exporting.
    pub const EXPORTING: Self = Self {
        status: AtomicUsize::new(4),
    };

    /// The asset is not supported yet.
    pub const NOT_SUPPORTED: Self = Self {
        status: AtomicUsize::new(5),
    };

    /// File metadata is available; the payload has no decoder yet.
    pub const INDEXED: Self = Self {
        status: AtomicUsize::new(6),
    };

    /// All statuses exposed by the app shell.
    pub const ALL: [Self; 7] = [
        Self::LOADED,
        Self::PLACEHOLDER,
        Self::EXPORTING,
        Self::EXPORTED,
        Self::ERROR,
        Self::NOT_SUPPORTED,
        Self::INDEXED,
    ];

    /// Returns true if the status matches.
    #[inline]
    pub fn is(&self, status: Self) -> bool {
        self.status.load(Ordering::Relaxed) == status.status.load(Ordering::Relaxed)
    }

    /// Returns true if the asset is available for export/preview.
    #[inline]
    pub fn is_available(&self) -> bool {
        !self.is(Self::PLACEHOLDER) && !self.is(Self::NOT_SUPPORTED) && !self.is(Self::INDEXED)
    }

    /// Sets the asset status.
    #[inline]
    pub fn set(&self, status: Self) {
        self.status
            .store(status.status.load(Ordering::Relaxed), Ordering::Relaxed)
    }

    /// Gets the color for this status.
    pub fn color(&self) -> Rgba {
        match self.status.load(Ordering::Relaxed) {
            0 => palette::ASSET_STATUS_LOADED,
            1 => palette::ASSET_STATUS_EXPORTED,
            2 => palette::ASSET_STATUS_ERROR,
            3 => palette::ASSET_STATUS_PLACEHOLDER,
            4 => palette::ASSET_STATUS_EXPORTING,
            5 => palette::ASSET_STATUS_NOT_SUPPORTED,
            _ => palette::TEXT_COLOR_DEFAULT,
        }
    }

    /// Returns the stable user-facing label for this status.
    pub fn as_str(&self) -> &'static str {
        match self.status.load(Ordering::Relaxed) {
            0 => "Loaded",
            1 => "Exported",
            2 => "Error",
            3 => "Placeholder",
            4 => "Exporting",
            5 => "Not Supported",
            6 => "Indexed",
            _ => "<unset>",
        }
    }
}

impl fmt::Display for AssetStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
