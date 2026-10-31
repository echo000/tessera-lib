use std::time::Duration;

use crate::components::box_shadow;
use gpui::AnyElement;
use gpui::AnyView;
use gpui::App;
use gpui::BoxShadow;
use gpui::Context;
use gpui::FontWeight;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::ParentElement;
use gpui::Render;
use gpui::RenderOnce;
use gpui::SharedString;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::Window;
use gpui::div;
use gpui::hsla;
use gpui::point;
use gpui::prelude::FluentBuilder;
use gpui::px;
use gpui::relative;

use crate::components::Button;
use crate::components::Icon;
use crate::components::IconName;
use crate::components::Sizable;
use crate::components::h_flex;
use crate::components::title_bar::TITLE_BAR_HEIGHT;
use crate::components::v_flex;
use crate::theme::ActiveTesseraTheme;
use crate::theme::TesseraTheme;

/// How long a notification stays up.
const NOTIFICATION_DURATION: Duration = Duration::from_secs(5);
/// The most notifications kept at once; older ones are dropped.
const NOTIFICATION_LIMIT: usize = 10;
/// The width of the notification stack.
const NOTIFICATION_WIDTH: f32 = 382.0;
/// How far each notification behind the newest one peeks out from under the one before it.
const NOTIFICATION_PEEK: f32 = 14.0;
/// How many notifications show while the stack is collapsed.
const NOTIFICATION_COLLAPSED_VISIBLE: usize = 3;
/// How much narrower each collapsed notification is than the one in front of it.
const NOTIFICATION_SCALE_STEP: f32 = 0.05;
/// The gap between notifications while the stack is expanded.
const NOTIFICATION_GAP: f32 = 14.0;
/// The width of the dialog panel.
const DIALOG_WIDTH: f32 = 448.0;

/// The kind of a notification, which picks its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationKind {
    Info,
    Success,
    Warning,
    Error,
}

/// A transient message shown in the top right corner of the window.
#[derive(Debug, Clone)]
pub(crate) struct Notification {
    kind: NotificationKind,
    message: SharedString,
}

impl Notification {
    /// An informational notification.
    pub(crate) fn info(message: impl Into<SharedString>) -> Self {
        Self::new(NotificationKind::Info, message)
    }

    /// A success notification.
    pub(crate) fn success(message: impl Into<SharedString>) -> Self {
        Self::new(NotificationKind::Success, message)
    }

    /// A warning notification.
    pub(crate) fn warning(message: impl Into<SharedString>) -> Self {
        Self::new(NotificationKind::Warning, message)
    }

    /// An error notification.
    pub(crate) fn error(message: impl Into<SharedString>) -> Self {
        Self::new(NotificationKind::Error, message)
    }

    fn new(kind: NotificationKind, message: impl Into<SharedString>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn icon(&self, theme: &TesseraTheme) -> Icon {
        match self.kind {
            NotificationKind::Info => Icon::new(IconName::Info).text_color(theme.info),
            NotificationKind::Success => Icon::new(IconName::CircleCheck).text_color(theme.success),
            NotificationKind::Warning => {
                Icon::new(IconName::TriangleAlert).text_color(theme.warning)
            }
            NotificationKind::Error => Icon::new(IconName::CircleX).text_color(theme.danger),
        }
    }
}

/// The window's root view: hosts the app view plus the dialog and notification layers.
pub(crate) struct Root {
    view: AnyView,
    dialog: Option<AnyView>,
    /// Shown notifications with their ids, oldest first.
    notifications: Vec<(u64, Notification)>,
    next_notification: u64,
    notifications_hovered: bool,
}

impl Root {
    /// Constructs a root hosting the given view.
    pub(crate) fn new(view: impl Into<AnyView>) -> Self {
        Self {
            view: view.into(),
            dialog: None,
            notifications: Vec::new(),
            next_notification: 0,
            notifications_hovered: false,
        }
    }

    fn push_notification(&mut self, notification: Notification, cx: &mut Context<Self>) {
        let id = self.next_notification;

        self.next_notification += 1;
        self.notifications.push((id, notification));

        if self.notifications.len() > NOTIFICATION_LIMIT {
            self.notifications.remove(0);
        }

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTIFICATION_DURATION).await;

            let _ = this.update(cx, |root, cx| root.dismiss_notification(id, cx));
        })
        .detach();

        cx.notify();
    }

    fn dismiss_notification(&mut self, id: u64, cx: &mut Context<Self>) {
        self.notifications.retain(|(entry, _)| *entry != id);

        if self.notifications.is_empty() {
            self.notifications_hovered = false;
        }

        cx.notify();
    }

    fn render_notification(
        &self,
        id: u64,
        notification: &Notification,
        theme: &TesseraTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let group = SharedString::from(format!("notification-{id}"));

        h_flex()
            .id(("notification", id))
            .group(group.clone())
            .occlude()
            .relative()
            .w_full()
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.element)
            .rounded(theme.radius_lg)
            .shadow(toast_shadow())
            .py_3p5()
            .px_4()
            .gap_3()
            .child(
                div()
                    .absolute()
                    .top(px(18.0))
                    .left_4()
                    .child(notification.icon(theme)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .pl_6()
                    .child(div().text_sm().child(notification.message.clone())),
            )
            .child(
                div()
                    .absolute()
                    .top_1()
                    .right_1()
                    .invisible()
                    .group_hover(group, |this| this.visible())
                    .child(
                        Button::new(("notification-close", id))
                            .icon(IconName::X)
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(move |root, _, _, cx| {
                                root.dismiss_notification(id, cx);
                            })),
                    ),
            )
    }

    /// Renders the notifications: collapsed to the newest with the next two peeking out from under
    /// it, and expanded into a list while hovered.
    fn render_notifications(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.tessera_theme();
        let expanded = self.notifications_hovered;

        let mut stack = v_flex()
            .id("notifications")
            .absolute()
            .top(TITLE_BAR_HEIGHT + px(16.0))
            .right(px(16.0))
            .w(px(NOTIFICATION_WIDTH))
            .on_hover(cx.listener(|root, hovered: &bool, _, cx| {
                root.notifications_hovered = *hovered;
                cx.notify();
            }));

        if expanded {
            stack = stack.gap(px(NOTIFICATION_GAP));

            for (id, notification) in self.notifications.iter().rev() {
                stack = stack.child(self.render_notification(*id, notification, &theme, cx));
            }

            return stack.into_any_element();
        }

        let Some(((front_id, front), behind)) = self.notifications.split_last() else {
            return stack.into_any_element();
        };

        // Older notifications sit behind the newest, each a step lower and narrower, painted
        // oldest first so the newest ends up on top.
        let behind = behind
            .iter()
            .rev()
            .take(NOTIFICATION_COLLAPSED_VISIBLE - 1)
            .enumerate()
            .collect::<Vec<_>>();

        for (index, (id, notification)) in behind.into_iter().rev() {
            let rank = index + 1;
            let inset = px(NOTIFICATION_WIDTH * NOTIFICATION_SCALE_STEP * rank as f32 / 2.0);

            stack = stack.child(
                div()
                    .absolute()
                    .top(px(NOTIFICATION_PEEK * rank as f32))
                    .left(inset)
                    .right(inset)
                    .child(self.render_notification(*id, notification, &theme, cx)),
            );
        }

        stack
            .child(self.render_notification(*front_id, front, &theme, cx))
            .into_any_element()
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.tessera_theme();

        window.set_rem_size(px(14.0));

        div()
            .id("root")
            .relative()
            .size_full()
            .font_family(".SystemUIFont")
            .bg(theme.background)
            .text_color(theme.text)
            .child(self.view.clone())
            .when_some(self.dialog.clone(), |this, dialog| this.child(dialog))
            .when(!self.notifications.is_empty(), |this| {
                this.child(self.render_notifications(cx))
            })
    }
}

/// Window helpers reaching the [`Root`] layers.
pub(crate) trait WindowExt {
    /// Opens a modal dialog, replacing any open one. The view renders a [`Dialog`].
    fn open_dialog(&mut self, dialog: impl Into<AnyView>, cx: &mut App);

    /// Closes the open dialog.
    fn close_dialog(&mut self, cx: &mut App);

    /// Shows a notification.
    fn push_notification(&mut self, notification: Notification, cx: &mut App);
}

impl WindowExt for Window {
    fn open_dialog(&mut self, dialog: impl Into<AnyView>, cx: &mut App) {
        let dialog = dialog.into();

        if let Some(Some(root)) = self.root::<Root>() {
            root.update(cx, |root, cx| {
                root.dialog = Some(dialog);
                cx.notify();
            });
        }
    }

    fn close_dialog(&mut self, cx: &mut App) {
        if let Some(Some(root)) = self.root::<Root>() {
            root.update(cx, |root, cx| {
                root.dialog = None;
                cx.notify();
            });
        }
    }

    fn push_notification(&mut self, notification: Notification, cx: &mut App) {
        if let Some(Some(root)) = self.root::<Root>() {
            root.update(cx, |root, cx| root.push_notification(notification, cx));
        }
    }
}

/// A modal dialog panel with a title, description and footer, over a click-blocking backdrop.
///
/// Rendered by a dialog view opened with [`WindowExt::open_dialog`].
#[derive(IntoElement)]
pub(crate) struct Dialog {
    title: SharedString,
    description: SharedString,
    footer: Vec<AnyElement>,
}

impl Dialog {
    /// Constructs a dialog.
    pub(crate) fn new(
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            footer: Vec::new(),
        }
    }

    /// Adds a footer element, typically a button.
    pub(crate) fn footer(mut self, element: impl IntoElement) -> Self {
        self.footer.push(element.into_any_element());
        self
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let viewport = window.viewport_size();
        let margin = px(16.0);
        let width = px(DIALOG_WIDTH).min((viewport.width - margin * 2.0).max(px(0.0)));
        let left = (viewport.width - width) / 2.0;
        // Centered vertically on the window.
        let top = (viewport.height / 2.0 - px(90.0)).max(px(40.0));

        div()
            .id("dialog-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(theme.overlay)
            .child(
                v_flex()
                    .id("dialog")
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(width)
                    .max_h((viewport.height - top - margin).max(px(0.0)))
                    .min_h_24()
                    .py_4()
                    .gap_4()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border_strong)
                    .rounded(theme.radius_lg)
                    .shadow(dialog_shadow())
                    .child(
                        v_flex()
                            .px_4()
                            .gap_2()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(relative(1.25))
                                    .child(self.title),
                            )
                            .child(
                                // Wraps short of the right edge, where gpui-component's dialog
                                // wrapped it.
                                div()
                                    .pr(px(48.0))
                                    .text_sm()
                                    .text_color(theme.text_secondary)
                                    .child(self.description),
                            ),
                    )
                    .child(
                        h_flex()
                            .px_4()
                            .gap_2()
                            .justify_end()
                            .line_height(relative(1.25))
                            .children(self.footer),
                    ),
            )
    }
}

/// The elevation of a notification (Tailwind's `shadow-lg`, with gpui's blur radii).
fn toast_shadow() -> Vec<BoxShadow> {
    let ink = hsla(0.0, 0.0, 0.0, 0.1);

    vec![
        box_shadow(px(0.0), px(10.0), px(7.5), px(-3.0), ink),
        box_shadow(px(0.0), px(4.0), px(3.0), px(-4.0), ink),
    ]
}

/// The elevation of a dialog (Tailwind's `shadow-xl`).
fn dialog_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: hsla(0.0, 0.0, 0.0, 0.1),
            offset: point(px(0.0), px(20.0)),
            blur_radius: px(25.0),
            spread_radius: px(-5.0),
        },
        BoxShadow {
            color: hsla(0.0, 0.0, 0.0, 0.1),
            offset: point(px(0.0), px(8.0)),
            blur_radius: px(10.0),
            spread_radius: px(-6.0),
        },
    ]
}
