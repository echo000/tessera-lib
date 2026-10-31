/// The size of a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Size {
    XSmall,
    Small,
    #[default]
    Medium,
}

/// Controls that come in several sizes.
pub(crate) trait Sizable: Sized {
    /// Sets the size of the control.
    fn with_size(self, size: Size) -> Self;

    /// Sets the extra small size.
    fn xsmall(self) -> Self {
        self.with_size(Size::XSmall)
    }

    /// Sets the small size.
    fn small(self) -> Self {
        self.with_size(Size::Small)
    }
}

/// Controls that can render a selected state.
pub(crate) trait Selectable: Sized {
    /// Sets whether the control is selected.
    fn selected(self, selected: bool) -> Self;
}

/// Controls that can be disabled.
pub(crate) trait Disableable: Sized {
    /// Sets whether the control is disabled.
    fn disabled(self, disabled: bool) -> Self;
}
