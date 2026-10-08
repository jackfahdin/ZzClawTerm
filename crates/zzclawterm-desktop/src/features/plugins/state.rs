use crate::features::ZzClawTermApp;
use crate::features::plugins::process::PluginProcess;
use crate::features::plugins::view::PluginPanel;
use gpui::{AppContext, Context, Entity, WeakEntity};

pub(in crate::features) struct PluginFeatureState {
    // Keep the editing state alive when the side panel is hidden or moved.
    pub panel: Entity<PluginPanel>,
}

impl PluginFeatureState {
    pub fn new(
        app: WeakEntity<ZzClawTermApp>,
        process: Entity<PluginProcess>,
        cx: &mut Context<ZzClawTermApp>,
    ) -> Self {
        let panel = cx.new(|cx| PluginPanel::new(app, process, cx));
        Self { panel }
    }

    pub fn shutdown(&mut self, cx: &mut Context<ZzClawTermApp>) {
        self.panel.update(cx, |panel, cx| panel.cancel(cx));
    }
}
