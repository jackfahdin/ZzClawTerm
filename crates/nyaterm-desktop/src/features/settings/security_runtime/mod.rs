use gpui::Context;

use crate::features::NyaTermApp;

mod credentials;
mod delete;
mod jobs;
mod keys;
mod otp;
mod passwords;
mod unlock;

impl NyaTermApp {
    /// Apply an edit from one of the security editors' inputs.
    ///
    /// `id` is what follows `security.editor.` in the field id, which names the
    /// editor and the field: `key-name`, `otp-secret`, `cred-pass-re`.
    pub(in crate::features) fn apply_security_editor_input(
        &mut self,
        id: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if self.security.editor_busy() {
            return;
        }
        if self.security.apply_editor_input(id, text) {
            match id {
                "key-data" => self.reset_text_input("security.editor.key-path", "", cx),
                "key-path" => self.reset_text_input("security.editor.key-data", "", cx),
                "key-cert-data" => self.reset_text_input("security.editor.key-cert-path", "", cx),
                "key-cert-path" => self.reset_text_input("security.editor.key-cert-data", "", cx),
                _ => {}
            }
            cx.notify();
        }
    }
}
