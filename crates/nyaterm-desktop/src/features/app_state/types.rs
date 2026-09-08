use nyaterm_core::{
    AiSettings, AppSettingsSummary, CloudSyncSettings, KeywordHighlightConfig, TranslationSettings,
};

use crate::models::{CloudSyncSecretDraft, TranslationSecretDraft};

#[derive(Debug, Clone)]
pub(in crate::features) struct SettingsDraftSnapshot {
    pub settings: AppSettingsSummary,
    pub ai_settings: AiSettings,
    pub ai_model_draft: String,
    pub ai_base_url_draft: String,
    pub ai_secret_draft: nyaterm_core::SecretString,
    pub cloud_sync_settings: CloudSyncSettings,
    pub cloud_sync_secret_draft: CloudSyncSecretDraft,
    pub translation_settings: TranslationSettings,
    pub translation_secret_draft: TranslationSecretDraft,
    pub keyword_highlights: KeywordHighlightConfig,
    pub master_password_enabled: bool,
    pub master_password_draft: nyaterm_core::SecretString,
}
