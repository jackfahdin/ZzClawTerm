use rust_i18n::t;

use gpui::{Context, Window};

use crate::features::{NyaTermApp, runtime_jobs::await_blocking_job};
use crate::models::SecurityAuthTab;

use super::jobs::{SecurityStoreLocation, load_security_catalog};

impl NyaTermApp {
    pub(in crate::features) fn open_security_delete_dialog(
        &mut self,
        kind: SecurityAuthTab,
        id: String,
        label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (title_key, description_key) = match kind {
            SecurityAuthTab::Keys => ("settings.deleteKey", "settings.deleteKeyConfirm"),
            SecurityAuthTab::Passwords => (
                "passwordManager.deleteTitle",
                "passwordManager.deleteConfirm",
            ),
            SecurityAuthTab::Credentials => (
                "credentialManager.deleteTitle",
                "credentialManager.deleteConfirm",
            ),
            SecurityAuthTab::Otp => ("otpManager.deleteTitle", "otpManager.deleteConfirm"),
        };
        let message = t!(description_key, name = label).to_string();
        self.open_confirm_dialog(
            (
                t!(title_key).to_string(),
                message,
                t!("common.delete").to_string(),
                true,
                move |app, window, cx| {
                    app.delete_security_item(kind, id.clone(), label.clone(), window, cx)
                },
            ),
            window,
            cx,
        );
    }

    fn delete_security_item(
        &mut self,
        kind: SecurityAuthTab,
        id: String,
        label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let request_item_id = id.clone();
        let request_id = self.security.begin_delete_request(id.clone());
        let scheduler = self.blocking_jobs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let task = scheduler.submit_task("security-item-delete", move |_| {
                let store = location.open()?;
                match kind {
                    SecurityAuthTab::Keys => store.delete_ssh_key(&id),
                    SecurityAuthTab::Passwords => store.delete_password(&id),
                    SecurityAuthTab::Credentials => store.delete_credential(&id),
                    SecurityAuthTab::Otp => store.delete_otp_entry(&id),
                }
                .map_err(|error| error.to_string())?;
                load_security_catalog(&store)
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if !this
                    .security
                    .finish_delete_request(request_id, &request_item_id)
                {
                    return;
                }
                match result {
                    Ok(catalog) => {
                        this.security
                            .clear_revealed_for_deleted(kind, &request_item_id);
                        this.security.replace_catalog_state(catalog);
                        let status = format!("{label} deleted");
                        this.security.set_status(status.clone());
                        this.shell.set_status(status);
                    }
                    Err(error) => this.security.set_status(error),
                }
                cx.notify();
            });
        })
        .detach();
        true
    }
}
