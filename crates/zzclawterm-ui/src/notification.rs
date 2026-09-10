use gpui::{App, SharedString, Window};
use gpui_component::{
    WindowExt as _,
    notification::{Notification, NotificationType},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZzClawNotificationKind {
    Success,
    Warning,
    Error,
}

/// In-app operation feedback. Stable operation keys replace duplicate toasts.
pub trait ZzClawNotificationWindowExt {
    fn notify_operation(
        &mut self,
        key: impl Into<SharedString>,
        kind: ZzClawNotificationKind,
        message: impl Into<SharedString>,
        cx: &mut App,
    );
}

impl ZzClawNotificationWindowExt for Window {
    fn notify_operation(
        &mut self,
        key: impl Into<SharedString>,
        kind: ZzClawNotificationKind,
        message: impl Into<SharedString>,
        cx: &mut App,
    ) {
        let notification = Notification::new()
            .id1::<ZzClawNotificationKind>(key.into())
            .message(message)
            .with_type(match kind {
                ZzClawNotificationKind::Success => NotificationType::Success,
                ZzClawNotificationKind::Warning => NotificationType::Warning,
                ZzClawNotificationKind::Error => NotificationType::Error,
            })
            .autohide(kind == ZzClawNotificationKind::Success);
        self.push_notification(notification, cx);
    }
}
