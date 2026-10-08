//! Ephemeral draft provenance. Never persists command contents or plugin input.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftOrigin {
    User,
    Plugin {
        plugin_id: String,
        action_id: String,
        revision: u64,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftProvenance {
    pub origins: Vec<DraftOrigin>,
    pub edited: bool,
}

impl DraftProvenance {
    pub fn fill(&mut self, origin: DraftOrigin, replace: bool, was_empty: bool) {
        if replace || was_empty {
            self.origins.clear();
            self.edited = false;
        } else if self.origins.is_empty() {
            self.origins.push(DraftOrigin::User);
        }
        if !self.origins.contains(&origin) {
            self.origins.push(origin);
        }
    }

    pub fn edit(&mut self, empty: bool) {
        if empty {
            *self = Self::default();
        } else if self.origins.is_empty() {
            self.origins.push(DraftOrigin::User);
        } else {
            self.edited = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::command_draft::{DraftOrigin, DraftProvenance};

    fn plugin(id: &str) -> DraftOrigin {
        DraftOrigin::Plugin {
            plugin_id: id.into(),
            action_id: "generate".into(),
            revision: 7,
        }
    }

    #[test]
    fn append_retains_all_sources_and_edits_until_explicit_replacement_or_clear() {
        let mut provenance = DraftProvenance::default();
        provenance.fill(plugin("first"), false, false);
        provenance.edit(false);
        provenance.fill(plugin("second"), false, false);
        assert_eq!(
            provenance.origins,
            vec![DraftOrigin::User, plugin("first"), plugin("second")]
        );
        assert!(provenance.edited);
        provenance.fill(plugin("second"), true, false);
        assert_eq!(provenance.origins, vec![plugin("second")]);
        assert!(!provenance.edited);
        provenance.edit(true);
        assert_eq!(provenance, DraftProvenance::default());
    }
}
