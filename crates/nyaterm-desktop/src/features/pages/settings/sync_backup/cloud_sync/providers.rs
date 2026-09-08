use rust_i18n::t;

use gpui::{AnyElement, Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgb};
use nyaterm_core::truncate_preview;

use crate::features::pages::settings::panel::SettingsPanel;
use crate::models::CloudSyncInputField;

use super::super::super::{settings_form_row, settings_switch_with_enabled};
use super::cloud_sync_action_button;

impl SettingsPanel {
    pub(super) fn cloud_sync_webdav_provider_fields(
        &mut self,
        password: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .grid()
            .grid_cols(2)
            .gap_2()
            .child(self.cloud_sync_input(
                "cloud-webdav-endpoint",
                t!("settings.webdavEndpoint"),
                self.cloud_sync.settings().webdav.endpoint.clone(),
                CloudSyncInputField::WebdavEndpoint,
                cx,
            ))
            .child(self.cloud_sync_input(
                "cloud-webdav-root",
                t!("settings.providerRoot"),
                self.cloud_sync.settings().webdav.root.clone(),
                CloudSyncInputField::WebdavRoot,
                cx,
            ))
            .child(self.cloud_sync_input(
                "cloud-webdav-username",
                t!("dialog.username"),
                self.cloud_sync.settings().webdav.username.clone(),
                CloudSyncInputField::WebdavUsername,
                cx,
            ))
            .child(self.cloud_sync_input(
                "cloud-webdav-password",
                t!("dialog.password"),
                password,
                CloudSyncInputField::WebdavPassword,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn cloud_sync_s3_provider_fields(
        &mut self,
        access_key: String,
        secret_key: String,
        session_token: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme_palette();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_2()
                    .child(self.cloud_sync_input(
                        "cloud-s3-endpoint",
                        t!("settings.s3Endpoint"),
                        self.cloud_sync.settings().s3.endpoint.clone(),
                        CloudSyncInputField::S3Endpoint,
                        cx,
                    ))
                    .child(self.cloud_sync_input(
                        "cloud-s3-bucket",
                        t!("settings.s3Bucket"),
                        self.cloud_sync.settings().s3.bucket.clone(),
                        CloudSyncInputField::S3Bucket,
                        cx,
                    ))
                    .child(self.cloud_sync_input(
                        "cloud-s3-region",
                        t!("settings.s3Region"),
                        self.cloud_sync.settings().s3.region.clone(),
                        CloudSyncInputField::S3Region,
                        cx,
                    ))
                    .child(self.cloud_sync_input(
                        "cloud-s3-root",
                        t!("settings.providerRoot"),
                        self.cloud_sync.settings().s3.root.clone(),
                        CloudSyncInputField::S3Root,
                        cx,
                    ))
                    .child(self.cloud_sync_input(
                        "cloud-s3-access-key",
                        t!("settings.s3AccessKeyId"),
                        access_key,
                        CloudSyncInputField::S3AccessKeyId,
                        cx,
                    ))
                    .child(self.cloud_sync_input(
                        "cloud-s3-secret-key",
                        t!("settings.s3SecretAccessKey"),
                        secret_key,
                        CloudSyncInputField::S3SecretAccessKey,
                        cx,
                    ))
                    .child(self.cloud_sync_input(
                        "cloud-s3-session-token",
                        t!("settings.s3SessionToken"),
                        session_token,
                        CloudSyncInputField::S3SessionToken,
                        cx,
                    )),
            )
            .child(settings_form_row(
                palette,
                t!("settings.s3VirtualHostStyle"),
                Some(SharedString::from(t!("settings.s3VirtualHostStyleDesc"))),
                settings_switch_with_enabled(
                    palette,
                    "cloud-s3-url-style",
                    self.cloud_sync.settings().s3.virtual_host_style,
                    self.cloud_sync_form_enabled(),
                    cx.listener(|this, _, _, cx| {
                        this.toggle_s3_virtual_host_style(cx);
                    }),
                ),
            ))
            .into_any_element()
    }

    pub(super) fn cloud_sync_oauth_provider_fields(
        &mut self,
        provider: &'static str,
        access_token: String,
        refresh_token: String,
        client_secret: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (root, client_id, root_field, access_field, refresh_field, id_field, secret_field) =
            match provider {
                "google_drive" => (
                    self.cloud_sync.settings().google_drive.root.clone(),
                    self.cloud_sync
                        .settings()
                        .google_drive
                        .client_id
                        .clone()
                        .unwrap_or_default(),
                    CloudSyncInputField::GoogleDriveRoot,
                    CloudSyncInputField::GoogleDriveAccessToken,
                    CloudSyncInputField::GoogleDriveRefreshToken,
                    CloudSyncInputField::GoogleDriveClientId,
                    CloudSyncInputField::GoogleDriveClientSecret,
                ),
                _ => (
                    self.cloud_sync.settings().onedrive.root.clone(),
                    self.cloud_sync
                        .settings()
                        .onedrive
                        .client_id
                        .clone()
                        .unwrap_or_default(),
                    CloudSyncInputField::OneDriveRoot,
                    CloudSyncInputField::OneDriveAccessToken,
                    CloudSyncInputField::OneDriveRefreshToken,
                    CloudSyncInputField::OneDriveClientId,
                    CloudSyncInputField::OneDriveClientSecret,
                ),
            };
        let prefix = if provider == "google_drive" {
            "cloud-google-drive"
        } else {
            "cloud-onedrive"
        };

        div()
            .grid()
            .grid_cols(2)
            .gap_2()
            .child(self.cloud_sync_input(
                if provider == "google_drive" {
                    "cloud-google-drive-root"
                } else {
                    "cloud-onedrive-root"
                },
                t!("settings.providerRoot"),
                root,
                root_field,
                cx,
            ))
            .child(self.cloud_sync_input(
                if provider == "google_drive" {
                    "cloud-google-drive-access-token"
                } else {
                    "cloud-onedrive-access-token"
                },
                t!("settings.driveAccessToken"),
                access_token,
                access_field,
                cx,
            ))
            .child(self.cloud_sync_input(
                if provider == "google_drive" {
                    "cloud-google-drive-refresh-token"
                } else {
                    "cloud-onedrive-refresh-token"
                },
                t!("settings.driveRefreshToken"),
                refresh_token,
                refresh_field,
                cx,
            ))
            .child(self.cloud_sync_input(
                if provider == "google_drive" {
                    "cloud-google-drive-client-id"
                } else {
                    "cloud-onedrive-client-id"
                },
                t!("settings.driveClientId"),
                client_id,
                id_field,
                cx,
            ))
            .child(self.cloud_sync_input(
                if provider == "google_drive" {
                    "cloud-google-drive-client-secret"
                } else {
                    "cloud-onedrive-client-secret"
                },
                t!("settings.driveClientSecret"),
                client_secret,
                secret_field,
                cx,
            ))
            .id(SharedString::from(format!("{prefix}-fields")))
            .into_any_element()
    }

    pub(super) fn cloud_sync_aliyun_provider_fields(
        &mut self,
        access_token: String,
        refresh_token: String,
        client_secret: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .grid()
            .grid_cols(2)
            .gap_2()
            .child(self.cloud_sync_input(
                "cloud-aliyun-drive-root",
                t!("settings.providerRoot"),
                self.cloud_sync.settings().aliyun_drive.root.clone(),
                CloudSyncInputField::AliyunDriveRoot,
                cx,
            ))
            .child(self.cloud_sync_input(
                "cloud-aliyun-drive-type",
                t!("settings.aliyunDriveType"),
                self.cloud_sync.settings().aliyun_drive.drive_type.clone(),
                CloudSyncInputField::AliyunDriveType,
                cx,
            ))
            .child(self.cloud_sync_input(
                "cloud-aliyun-drive-access-token",
                t!("settings.driveAccessToken"),
                access_token,
                CloudSyncInputField::AliyunDriveAccessToken,
                cx,
            ))
            .child(self.cloud_sync_input(
                "cloud-aliyun-drive-refresh-token",
                t!("settings.driveRefreshToken"),
                refresh_token,
                CloudSyncInputField::AliyunDriveRefreshToken,
                cx,
            ))
            .child(
                self.cloud_sync_input(
                    "cloud-aliyun-drive-client-id",
                    t!("settings.driveClientId"),
                    self.cloud_sync
                        .settings()
                        .aliyun_drive
                        .client_id
                        .clone()
                        .unwrap_or_default(),
                    CloudSyncInputField::AliyunDriveClientId,
                    cx,
                ),
            )
            .child(self.cloud_sync_input(
                "cloud-aliyun-drive-client-secret",
                t!("settings.driveClientSecret"),
                client_secret,
                CloudSyncInputField::AliyunDriveClientSecret,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn cloud_sync_gitee_provider_fields(
        &mut self,
        token: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .grid()
            .grid_cols(2)
            .gap_2()
            .child(
                self.cloud_sync_input(
                    "cloud-gitee-endpoint",
                    t!("settings.giteeSnippetApiEndpoint"),
                    self.cloud_sync
                        .settings()
                        .gitee_snippet
                        .api_endpoint
                        .clone(),
                    CloudSyncInputField::GiteeEndpoint,
                    cx,
                ),
            )
            .child(self.cloud_sync_input(
                "cloud-gitee-gist",
                t!("settings.giteeSnippetId"),
                self.cloud_sync.settings().gitee_snippet.gist_id.clone(),
                CloudSyncInputField::GiteeGistId,
                cx,
            ))
            .child(self.cloud_sync_input(
                "cloud-gitee-token",
                t!("settings.giteeSnippetAccessToken"),
                token,
                CloudSyncInputField::GiteeToken,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn cloud_sync_github_provider_fields(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme_palette();
        let pending = self.cloud_sync.github_auth().pending;
        let form_enabled = self.cloud_sync_form_enabled();
        let connected = !self.cloud_sync.secret_draft().github_token.is_empty()
            || self
                .cloud_sync
                .settings()
                .github_gist
                .access_token
                .as_deref()
                .is_some_and(|token| !token.trim().is_empty());
        let auth_label = if connected {
            t!("settings.githubGistReconnect")
        } else {
            t!("settings.githubGistConnect")
        };
        let gist_id = self.cloud_sync.settings().github_gist.gist_id.clone();
        let user_code = self.cloud_sync.github_auth().user_code.clone();
        let verification_uri = self.cloud_sync.github_auth().verification_uri.clone();
        let login = self.cloud_sync.github_auth().login.clone();
        let message = self.cloud_sync.github_auth().message.clone();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .opacity(if pending { 0.55 } else { 1.0 })
                    .child(self.cloud_sync_input(
                        "cloud-github-gist",
                        t!("settings.githubGistId"),
                        gist_id.clone(),
                        CloudSyncInputField::GithubGistId,
                        cx,
                    )),
            )
            .child(settings_form_row(
                palette,
                t!("settings.githubGistAuth"),
                Some(SharedString::from(t!("settings.githubGistAuthDesc"))),
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(cloud_sync_action_button(
                        palette,
                        "cloud-github-connect",
                        auth_label,
                        form_enabled && !pending,
                        cx.listener(|this, _, _, cx| {
                            this.start_github_gist_auth(cx);
                        }),
                    ))
                    .when(pending, |this| {
                        this.child(cloud_sync_action_button(
                            palette,
                            "cloud-github-cancel",
                            t!("common.cancel"),
                            true,
                            cx.listener(|this, _, _, cx| {
                                this.cancel_github_gist_auth(cx);
                            }),
                        ))
                    }),
            ))
            .when_some(user_code, |this, code| {
                this.child(
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(rgb(palette.input))
                        .px_4()
                        .py_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(palette.text_muted))
                                .child(t!("settings.githubGistUserCode")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .font_family(crate::features::shell::gpui_code_font_family())
                                        .text_size(px(18.))
                                        .font_weight(FontWeight(600.))
                                        .text_color(rgb(palette.text))
                                        .child(code),
                                )
                                .child(cloud_sync_action_button(
                                    palette,
                                    "cloud-github-copy-code",
                                    t!("settings.copyGithubGistUserCode"),
                                    true,
                                    cx.listener(|this, _, _, cx| {
                                        this.copy_github_gist_user_code(cx);
                                    }),
                                )),
                        )
                        .when_some(verification_uri.clone(), |this, uri| {
                            this.child(
                                div()
                                    .id("cloud-github-verification-url")
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(rgb(palette.link))
                                    .cursor_pointer()
                                    .hover(|this| this.text_color(rgb(palette.primary_hover)))
                                    .child(uri)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_github_gist_verification_url(cx);
                                    })),
                            )
                        }),
                )
            })
            .when(
                login.is_some() || !gist_id.is_empty() || message.is_some(),
                |this| {
                    this.child(
                        div()
                            .min_w_0()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette.border))
                            .bg(rgb(palette.surface))
                            .px_4()
                            .py_3()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .text_size(px(12.))
                            .text_color(rgb(palette.text_muted))
                            .when_some(login, |this, login| {
                                this.child(
                                    t!("settings.githubGistConnectedAs", login = login),
                                )
                            })
                            .when(!gist_id.is_empty(), |this| {
                                this.child(
                                    t!("settings.githubGistCurrentId", gistId = truncate_preview(&gist_id, 10)),
                                )
                            })
                            .when_some(message, |this, message| this.child(message)),
                    )
                },
            )
            .into_any_element()
    }
}
