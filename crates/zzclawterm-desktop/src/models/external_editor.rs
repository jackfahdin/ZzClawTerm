use std::time::SystemTime;

/// A stable content version captured before launching an external editor.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ExternalEditorFileVersion {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub digest: [u8; 32],
}

impl std::fmt::Debug for ExternalEditorFileVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalEditorFileVersion")
            .field("len", &self.len)
            .field("digest", &"<redacted>")
            .finish_non_exhaustive()
    }
}
