use gpui::Context;

use crate::features::NyaTermApp;

mod helpers;

mod draft;
mod general_interaction;
mod recording_transfer;
mod search_engines;
mod terminal_remote;
mod window;

pub(in crate::features) use general_interaction::SettingsSaveKind;

use helpers::open_external_url_simple;

impl NyaTermApp {
    pub(in crate::features) fn open_external_url_for_ui(
        &mut self,
        url: &str,
        cx: &mut Context<Self>,
    ) {
        match open_external_url_simple(url) {
            Ok(()) => self.shell.set_status(format!("opened URL: {url}")),
            Err(error) => self
                .shell
                .set_status(format!("failed to open URL: {error}")),
        }
        cx.notify();
    }

    pub(in crate::features) fn open_documentation(&mut self, cx: &mut Context<Self>) {
        const DOCS_URL: &str = "https://nyaterm.app/docs/";
        self.open_external_url_for_ui(DOCS_URL, cx);
    }
}
