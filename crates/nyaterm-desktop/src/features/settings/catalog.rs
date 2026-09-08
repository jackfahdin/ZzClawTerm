//! Compatibility-sensitive settings state shared by the settings feature owner.

pub(super) struct SettingsMasterPasswordState {
    pub(super) enabled: bool,
    pub(super) draft: nyaterm_core::SecretString,
}

impl SettingsMasterPasswordState {
    pub fn new(has_stored_password: bool) -> Self {
        Self {
            enabled: has_stored_password,
            draft: nyaterm_core::SecretString::default(),
        }
    }

    pub fn reset(&mut self, has_stored_password: bool) {
        self.enabled = has_stored_password;
        self.draft.expose_secret_mut().clear();
    }

    pub fn toggle(&mut self, cloud_sync_enabled: bool) -> Result<bool, &'static str> {
        if cloud_sync_enabled && self.enabled {
            return Err("disable cloud sync before removing the master password");
        }
        self.enabled = !self.enabled;
        self.draft.expose_secret_mut().clear();
        Ok(self.enabled)
    }

    pub fn edit_draft(&mut self, text: String) -> bool {
        if !self.enabled {
            return false;
        }
        self.draft = text.into();
        true
    }
}

#[derive(Debug, Clone)]
pub(super) struct StoreStatus {
    pub(super) path: String,
    pub(super) message: String,
    pub(super) ready: bool,
}

#[cfg(test)]
mod tests {
    use super::SettingsMasterPasswordState;

    #[test]
    fn master_password_reset_rebases_enabled_state_and_clears_secret_draft() {
        let mut state = SettingsMasterPasswordState::new(false);
        state.enabled = true;
        state.draft = "staged secret".to_string().into();

        state.reset(false);

        assert!(!state.enabled);
        assert!(state.draft.is_empty());
    }

    #[test]
    fn master_password_toggle_preserves_cloud_sync_requirement() {
        let mut state = SettingsMasterPasswordState::new(true);
        state.draft = "staged secret".to_string().into();

        assert_eq!(
            state.toggle(true),
            Err("disable cloud sync before removing the master password")
        );
        assert!(state.enabled);
        assert_eq!(state.draft.expose_secret(), "staged secret");

        assert_eq!(state.toggle(false), Ok(false));
        assert!(state.draft.is_empty());
    }

    #[test]
    fn master_password_draft_is_editable_only_while_enabled() {
        let mut state = SettingsMasterPasswordState::new(false);
        assert!(!state.edit_draft("ignored".to_string()));
        assert!(state.draft.is_empty());

        assert_eq!(state.toggle(false), Ok(true));
        assert!(state.edit_draft("replacement".to_string()));
        assert_eq!(state.draft.expose_secret(), "replacement");
    }
}
