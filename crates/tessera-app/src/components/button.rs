use gpui::AnyElement;
use gpui::AnyView;
use gpui::App;
use gpui::AppContext;
use gpui::ClickEvent;
use gpui::Component;
use gpui::Context;
use gpui::Div;
use gpui::ElementId;
use gpui::Hsla;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::ParentElement;
use gpui::Refineable;
use gpui::Render;
use gpui::RenderOnce;
use gpui::SharedString;
use gpui::Stateful;
use gpui::StatefulInteractiveElement;
use gpui::StyleRefinement;
use gpui::Styled;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder;
use gpui::relative;

use crate::components::Disableable;
use crate::components::Icon;
use crate::components::IconName;
use crate::components::Selectable;
use crate::components::Sizable;
use crate::components::Size;
use crate::components::h_flex;
use crate::palette;
use crate::theme::ActiveTesseraTheme;
use crate::theme::TesseraTheme;

/// The look of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonVariant {
    /// Filled with the primary accent.
    Primary,
    /// A bordered, lightly filled button.
    Outline,
    /// No border or fill until hovered.
    Ghost,
}

/// Colors for one state of a button.
#[derive(Clone, Copy)]
struct ButtonColors {
    bg: Hsla,
    border: Hsla,
    fg: Hsla,
}

impl ButtonVariant {
    fn normal(self, theme: &TesseraTheme) -> ButtonColors {
        match self {
            ButtonVariant::Primary => ButtonColors {
                bg: theme.primary.into(),
                border: theme.primary.into(),
                fg: theme.on_primary.into(),
            },
            ButtonVariant::Outline => ButtonColors {
                bg: palette::with_alpha(theme.border_strong, 0.3).into(),
                border: theme.border_strong.into(),
                fg: theme.text.into(),
            },
            ButtonVariant::Ghost => ButtonColors {
                bg: gpui::transparent_black(),
                border: gpui::transparent_black(),
                fg: theme.text.into(),
            },
        }
    }

    fn hovered(self, theme: &TesseraTheme) -> ButtonColors {
        match self {
            ButtonVariant::Primary => ButtonColors {
                bg: theme.primary_hover.into(),
                ..self.normal(theme)
            },
            ButtonVariant::Outline => ButtonColors {
                bg: palette::with_alpha(theme.border_strong, 0.5).into(),
                ..self.normal(theme)
            },
            ButtonVariant::Ghost => ButtonColors {
                bg: palette::with_alpha(theme.border, 0.5).into(),
                ..self.normal(theme)
            },
        }
    }

    fn active(self, theme: &TesseraTheme) -> ButtonColors {
        match self {
            ButtonVariant::Primary => ButtonColors {
                bg: theme.primary_active.into(),
                ..self.normal(theme)
            },
            ButtonVariant::Outline => ButtonColors {
                bg: palette::with_alpha(theme.border_strong, 0.7).into(),
                ..self.normal(theme)
            },
            ButtonVariant::Ghost => ButtonColors {
                bg: theme.border_strong.into(),
                ..self.normal(theme)
            },
        }
    }

    fn selected(self, theme: &TesseraTheme) -> ButtonColors {
        match self {
            ButtonVariant::Outline => ButtonColors {
                fg: theme.text.into(),
                ..self.active(theme)
            },
            _ => self.active(theme),
        }
    }

    fn disabled(self, theme: &TesseraTheme) -> ButtonColors {
        let fg = palette::with_alpha(theme.text_secondary, 0.5).into();

        match self {
            ButtonVariant::Primary => ButtonColors {
                bg: palette::with_alpha(theme.primary, 0.15).into(),
                border: palette::with_alpha(theme.primary, 0.15).into(),
                fg,
            },
            ButtonVariant::Outline => ButtonColors {
                bg: palette::with_alpha(theme.border_strong, 0.15).into(),
                border: palette::with_alpha(theme.border_strong, 0.5).into(),
                fg,
            },
            ButtonVariant::Ghost => ButtonColors {
                bg: gpui::transparent_black(),
                border: gpui::transparent_black(),
                fg,
            },
        }
    }
}

/// The click handler of a button without one.
type NoClick = fn(&ClickEvent, &mut Window, &mut App);

/// A button with an optional icon and label.
pub(crate) struct Button<F = NoClick> {
    base: Stateful<Div>,
    label: Option<SharedString>,
    icon: Option<Icon>,
    tooltip: Option<SharedString>,
    variant: ButtonVariant,
    size: Size,
    selected: bool,
    disabled: bool,
    children: Vec<AnyElement>,
    on_click: Option<F>,
}

impl Button {
    /// Constructs an outline button.
    pub(crate) fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id),
            label: None,
            icon: None,
            tooltip: None,
            variant: ButtonVariant::Outline,
            size: Size::Medium,
            selected: false,
            disabled: false,
            children: Vec::new(),
            on_click: None,
        }
    }
}

impl<F: Fn(&ClickEvent, &mut Window, &mut App) + 'static> Button<F> {
    /// Sets the label.
    pub(crate) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the icon, shown before the label.
    pub(crate) fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(Icon::new(icon));
        self
    }

    /// Sets the tooltip shown on hover.
    pub(crate) fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Uses the primary style.
    pub(crate) fn primary(mut self) -> Self {
        self.variant = ButtonVariant::Primary;
        self
    }

    /// Uses the outline style.
    pub(crate) fn outline(mut self) -> Self {
        self.variant = ButtonVariant::Outline;
        self
    }

    /// Uses the ghost style.
    pub(crate) fn ghost(mut self) -> Self {
        self.variant = ButtonVariant::Ghost;
        self
    }

    /// Sets the click handler.
    pub(crate) fn on_click<G: Fn(&ClickEvent, &mut Window, &mut App) + 'static>(
        self,
        on_click: G,
    ) -> Button<G> {
        Button {
            base: self.base,
            label: self.label,
            icon: self.icon,
            tooltip: self.tooltip,
            variant: self.variant,
            size: self.size,
            selected: self.selected,
            disabled: self.disabled,
            children: self.children,
            on_click: Some(on_click),
        }
    }
}

impl<F> Styled for Button<F> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl<F> ParentElement for Button<F> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl<F> Sizable for Button<F> {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl<F> Selectable for Button<F> {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl<F> Disableable for Button<F> {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl<F: Fn(&ClickEvent, &mut Window, &mut App) + 'static> IntoElement for Button<F> {
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl<F: Fn(&ClickEvent, &mut Window, &mut App) + 'static> RenderOnce for Button<F> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let variant = self.variant;
        let colors = if self.disabled {
            variant.disabled(&theme)
        } else if self.selected {
            variant.selected(&theme)
        } else {
            variant.normal(&theme)
        };
        let interactive = !self.disabled && !self.selected;
        let icon_only = self.label.is_none() && self.children.is_empty();
        let bordered = variant == ButtonVariant::Outline;
        let size = self.size;

        let content = h_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .justify_center()
            .map(|this| match size {
                Size::XSmall => this.gap_1().text_xs(),
                Size::Small => this.gap_1().text_sm(),
                Size::Medium => this.gap_2().text_base(),
            })
            .when_some(self.icon, |this, icon| this.child(icon.with_size(size)))
            .when_some(self.label, |this, label| {
                this.child(
                    div()
                        .min_w_0()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
            })
            .children(self.children);

        let mut base = self.base;
        let instance_style = base.style().clone();

        base.flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .cursor_default()
            .map(|this| match (icon_only, size) {
                (true, Size::XSmall) => this.size_5(),
                (true, Size::Small) => this.size_6(),
                (true, Size::Medium) => this.size_8(),
                (false, Size::XSmall) => this.h_5().px_1(),
                (false, Size::Small) => this.h_6().px_2(),
                (false, Size::Medium) => this.h_8().px_2p5(),
            })
            .rounded(theme.radius)
            .when(bordered, |this| this.border_1())
            .border_color(colors.border)
            .bg(colors.bg)
            .text_color(colors.fg)
            .line_height(relative(1.25))
            .when(interactive, |this| {
                let hovered = variant.hovered(&theme);
                let active = variant.active(&theme);

                this.hover(move |this| {
                    this.bg(hovered.bg)
                        .border_color(hovered.border)
                        .text_color(hovered.fg)
                })
                .active(move |this| {
                    this.bg(active.bg)
                        .border_color(active.border)
                        .text_color(active.fg)
                })
            })
            .map(|mut this| {
                // Caller styles (`flex_1`, widths) win over the defaults above.
                this.style().refine(&instance_style);
                this
            })
            .when(!self.disabled, |this| {
                // Pressing a button must not move focus away from e.g. the search box.
                this.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                |this, on_click| {
                    this.on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        on_click(event, window, cx);
                    })
                },
            )
            .when_some(self.tooltip, |this, tooltip| {
                this.tooltip(move |_, cx| Tooltip::view(tooltip.clone(), cx))
            })
            .child(content)
    }
}

/// A short text tooltip.
pub(crate) struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    /// Builds a tooltip view for gpui's tooltip hooks.
    pub(crate) fn view(text: SharedString, cx: &mut App) -> AnyView {
        cx.new(|_| Tooltip { text }).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.tessera_theme();

        div().pl_2().pt_2().child(
            div()
                .bg(theme.element)
                .text_color(theme.text)
                .border_1()
                .border_color(theme.border_strong)
                .shadow_md()
                .rounded(theme.radius)
                .py_0p5()
                .px_2()
                .text_sm()
                .child(self.text.clone()),
        )
    }
}
