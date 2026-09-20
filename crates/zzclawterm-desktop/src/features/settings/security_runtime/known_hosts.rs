use rust_i18n::t;

use gpui::{Context, Window};

use crate::features::{ZzClawTermApp, runtime_jobs::await_blocking_job};

use super::jobs::SecurityStoreLocation;

impl ZzClawTermApp {
    pub(in crate::features) fn refresh_security_known_hosts(&mut self, cx: &mut Context<Self>) {
        if self.security.known_hosts_busy() {
            return;
        }
        let generation = self.security.begin_known_hosts_load();
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let scheduler = self.blocking_jobs.clone();
        cx.spawn(async move |this, cx| {
            let task = scheduler.submit_task("known-hosts-list", move |_| {
                let store = location.open()?;
                store.list_known_hosts().map_err(|error| error.to_string())
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(entries) => {
                        this.security.apply_known_hosts_load(generation, entries);
                    }
                    Err(error) => {
                        if this
                            .security
                            .fail_known_hosts_load(generation, error.clone())
                        {
                            this.shell.set_status(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::features) fn request_delete_security_known_host(
        &mut self,
        id: String,
        label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_confirm_dialog(
            (
                t!("securityAuth.deleteKnownHost").to_string(),
                t!("securityAuth.deleteKnownHostConfirm", name = label).to_string(),
                t!("common.delete").to_string(),
                true,
                move |app, window, cx| app.delete_security_known_host(id.clone(), window, cx),
            ),
            window,
            cx,
        );
    }

    fn delete_security_known_host(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(request_id) = self.security.begin_known_host_delete(id.clone()) else {
            return false;
        };
        let request_item_id = id.clone();
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let scheduler = self.blocking_jobs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let task = scheduler.submit_task("known-host-delete", move |_| {
                let store = location.open()?;
                store
                    .delete_known_host(&id)
                    .map_err(|error| error.to_string())?;
                store.list_known_hosts().map_err(|error| error.to_string())
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if !this
                    .security
                    .finish_known_host_delete(request_id, &request_item_id)
                {
                    return;
                }
                let generation = this.security.begin_known_hosts_load();
                match result {
                    Ok(entries) => {
                        this.security.apply_known_hosts_load(generation, entries);
                        this.security
                            .set_status(t!("securityAuth.knownHostDeleted").to_string());
                        this.shell
                            .set_status(t!("securityAuth.knownHostDeleted").to_string());
                    }
                    Err(error) => {
                        this.security
                            .fail_known_hosts_load(generation, error.clone());
                        this.shell.set_status(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(in crate::features) fn request_clear_security_known_hosts(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_confirm_dialog(
            (
                t!("securityAuth.clearKnownHosts").to_string(),
                t!("securityAuth.clearKnownHostsConfirm").to_string(),
                t!("common.delete").to_string(),
                true,
                move |app, window, cx| app.clear_security_known_hosts(window, cx),
            ),
            window,
            cx,
        );
    }

    fn clear_security_known_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(request_id) = self.security.begin_known_hosts_clear() else {
            return false;
        };
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let scheduler = self.blocking_jobs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let task = scheduler.submit_task("known-hosts-clear", move |_| {
                let store = location.open()?;
                store
                    .clear_known_hosts()
                    .map_err(|error| error.to_string())?;
                store.list_known_hosts().map_err(|error| error.to_string())
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if !this.security.finish_known_hosts_clear(request_id) {
                    return;
                }
                let generation = this.security.begin_known_hosts_load();
                match result {
                    Ok(entries) => {
                        this.security.apply_known_hosts_load(generation, entries);
                        this.security
                            .set_status(t!("securityAuth.knownHostsCleared").to_string());
                        this.shell
                            .set_status(t!("securityAuth.knownHostsCleared").to_string());
                    }
                    Err(error) => {
                        this.security
                            .fail_known_hosts_load(generation, error.clone());
                        this.shell.set_status(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        true
    }
}
