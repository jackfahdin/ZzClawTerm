use gpui::{Context, IntoElement, Render, Subscription, WeakEntity, Window, div};

use crate::features::ZzClawTermApp;

/// Render boundary only; the feature state remains the authoritative session owner.
pub(super) struct RemoteDesktopSurface {
    app: WeakEntity<ZzClawTermApp>,
    session_id: String,
    _app_observation: Option<Subscription>,
}

impl RemoteDesktopSurface {
    pub(super) fn new(
        app: WeakEntity<ZzClawTermApp>,
        session_id: String,
        cx: &mut Context<Self>,
    ) -> Self {
        // Root state changes (theme, dialogs, session options) invalidate this
        // child. Frame updates notify only the child, never the reverse.
        let observation = app
            .upgrade()
            .map(|app| cx.observe(&app, |_, _, cx| cx.notify()));
        Self {
            app,
            session_id,
            _app_observation: observation,
        }
    }
}

impl Render for RemoteDesktopSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session_id = self.session_id.clone();
        self.app
            .update(cx, |app, cx| {
                app.prepare_remote_desktop_textures(&session_id, window);
                app.remote_desktop_view_content(session_id, cx)
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}
