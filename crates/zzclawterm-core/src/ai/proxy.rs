use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiProxyMode {
    #[default]
    System,
    Direct,
    Custom,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiProxyProtocol {
    #[default]
    Http,
    Socks5,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AiProxySettings {
    pub mode: AiProxyMode,
    pub protocol: AiProxyProtocol,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<crate::SecretString>,
    pub no_proxy: String,
}

impl std::fmt::Debug for AiProxySettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiProxySettings")
            .field("mode", &self.mode)
            .field("protocol", &self.protocol)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("has_credentials", &self.username.is_some())
            .field("has_password", &self.password.is_some())
            .field("no_proxy", &self.no_proxy)
            .finish()
    }
}

impl Default for AiProxySettings {
    fn default() -> Self {
        Self {
            mode: AiProxyMode::System,
            protocol: AiProxyProtocol::Http,
            host: "127.0.0.1".to_string(),
            port: 7890,
            username: None,
            password: None,
            no_proxy: "localhost,127.0.0.1,::1".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AiProxyMode, AiProxySettings};
    use crate::ai::{AiSettings, mask_ai_settings, merge_masked_ai_settings};

    #[test]
    fn legacy_settings_default_to_system_and_proxy_password_can_be_kept_or_cleared() {
        let legacy: AiSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.proxy.mode, AiProxyMode::System);
        assert_eq!(legacy.proxy, AiProxySettings::default());
        let mut settings = legacy;
        settings.proxy.password = Some("fixture-secret".into());
        let masked = mask_ai_settings(settings.clone());
        assert_ne!(masked.proxy.password, settings.proxy.password);
        assert_eq!(
            merge_masked_ai_settings(&settings, masked).proxy.password,
            settings.proxy.password
        );
        let mut cleared = settings.clone();
        cleared.proxy.password = Some("".into());
        assert!(
            merge_masked_ai_settings(&settings, cleared)
                .proxy
                .password
                .is_none()
        );
        let roundtrip: AiSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(roundtrip.proxy, settings.proxy);
        assert!(!format!("{:?}", settings.proxy).contains("fixture-secret"));
    }
}
