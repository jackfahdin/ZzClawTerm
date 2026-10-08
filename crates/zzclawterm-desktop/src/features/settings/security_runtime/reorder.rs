use gpui::Context;
use rust_i18n::t;

use crate::features::{ZzClawTermApp, runtime_jobs::await_blocking_job};
use crate::models::SecurityAuthTab;

use super::jobs::{SecurityStoreLocation, load_security_catalog};

impl ZzClawTermApp {
    pub(in crate::features) fn reorder_security_entries(
        &mut self,
        tab: SecurityAuthTab,
        source_id: String,
        target_id: String,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        self.security.clear_drop_target();
        if tab != self.security.auth_tab() {
            cx.notify();
            return;
        }
        let Some(updates) = self
            .security
            .reordered_entries(tab, &source_id, &target_id, after)
        else {
            cx.notify();
            return;
        };
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let Some(request_id) = self.security.begin_reorder_request() else {
            cx.notify();
            return;
        };
        cx.notify();
        let scheduler = self.blocking_jobs.clone();
        cx.spawn(async move |this, cx| {
            let task = scheduler.submit_task("security-reorder", move |_| {
                let store = location.open()?;
                match tab {
                    SecurityAuthTab::Passwords => store.reorder_passwords(&updates),
                    SecurityAuthTab::Keys => store.reorder_ssh_keys(&updates),
                    SecurityAuthTab::Credentials => store.reorder_credentials(&updates),
                    SecurityAuthTab::Otp | SecurityAuthTab::KnownHosts => unreachable!(),
                }
                .map_err(|error| error.to_string())?;
                load_security_catalog(&store)
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if !this.security.finish_reorder_request(request_id) {
                    return;
                }

                let status = match result {
                    Ok(catalog) => {
                        this.security.replace_catalog_state(catalog);
                        this.request_shared_state_refresh(
                            crate::app_shell::SharedStateDomain::Security,
                            cx,
                        );
                        t!(match tab {
                            SecurityAuthTab::Passwords => "passwordManager.reorderSuccess",
                            SecurityAuthTab::Keys => "securityAuth.reorderSuccess",
                            _ => "credentialManager.reorderSuccess",
                        })
                        .to_string()
                    }
                    Err(error) => {
                        format!(
                            "{}: {error}",
                            t!(match tab {
                                SecurityAuthTab::Passwords => "passwordManager.reorderFailed",
                                SecurityAuthTab::Keys => "securityAuth.reorderFailed",
                                _ => "credentialManager.reorderFailed",
                            })
                        )
                    }
                };
                this.security.set_status(status);
                cx.notify();
            });
        })
        .detach();
    }
}
