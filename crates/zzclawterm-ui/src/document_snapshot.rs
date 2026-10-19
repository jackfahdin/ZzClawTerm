use std::fmt;
use std::sync::{Arc, OnceLock};

use ropey::Rope;

/// Clones share immutable rope content and any lazily materialized text.
/// Append/pop replace the snapshot while preserving all existing clones.
#[derive(Clone)]
pub struct ZzClawDocumentSnapshot(Arc<DocumentSnapshot>);

struct DocumentSnapshot {
    text: Rope,
    value: OnceLock<String>,
}

impl ZzClawDocumentSnapshot {
    pub(crate) fn from_rope(text: &Rope) -> Self {
        Self(Arc::new(DocumentSnapshot {
            text: text.clone(),
            value: OnceLock::new(),
        }))
    }

    pub fn same_version(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn len(&self) -> usize {
        self.0.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_str(&self) -> &str {
        self.0.value.get_or_init(|| self.0.text.to_string())
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.as_str().as_bytes()
    }

    pub(crate) fn text(&self) -> &Rope {
        &self.0.text
    }

    pub fn push_str(&mut self, text: &str) {
        let mut rope = self.0.text.clone();
        rope.insert(rope.len(), text);
        *self = Self::from_rope(&rope);
    }

    pub fn push(&mut self, ch: char) {
        self.push_str(ch.encode_utf8(&mut [0; 4]));
    }

    pub fn pop(&mut self) -> Option<char> {
        let ch = self.0.text.chars_at(self.len()).prev()?;
        let mut rope = self.0.text.clone();
        rope.remove(rope.len() - ch.len_utf8()..);
        *self = Self::from_rope(&rope);
        Some(ch)
    }
}

impl Default for ZzClawDocumentSnapshot {
    fn default() -> Self {
        Self::from_rope(&Rope::new())
    }
}

impl From<String> for ZzClawDocumentSnapshot {
    fn from(value: String) -> Self {
        let text = Rope::from_str(&value);
        Self(Arc::new(DocumentSnapshot {
            text,
            value: OnceLock::from(value),
        }))
    }
}

impl From<&str> for ZzClawDocumentSnapshot {
    fn from(value: &str) -> Self {
        Self::from_rope(&Rope::from_str(value))
    }
}

impl PartialEq for ZzClawDocumentSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.same_version(other) || self.0.text == other.0.text
    }
}

impl Eq for ZzClawDocumentSnapshot {}

impl PartialEq<&str> for ZzClawDocumentSnapshot {
    fn eq(&self, other: &&str) -> bool {
        self.0.text == *other
    }
}

impl fmt::Display for ZzClawDocumentSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.text, formatter)
    }
}

impl fmt::Debug for ZzClawDocumentSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ZzClawDocumentSnapshot")
            .field("bytes", &self.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use ropey::Rope;

    use super::ZzClawDocumentSnapshot;

    #[test]
    fn snapshots_share_content_without_materializing_or_following_later_edits() {
        let mut text = Rope::from_str(&"中🙂\r\n".repeat(32_000));
        let snapshot = ZzClawDocumentSnapshot::from_rope(&text);
        let clone = snapshot.clone();
        assert!(snapshot.same_version(&clone));
        assert!(snapshot.0.value.get().is_none());
        text.insert(0, "new");
        assert_ne!(snapshot.text(), &text);
        assert_eq!(snapshot.as_str().as_ptr(), clone.as_str().as_ptr());
    }

    #[test]
    fn fallback_edits_preserve_utf8_and_existing_snapshots() {
        let mut content = ZzClawDocumentSnapshot::from("a中🙂\r\n");
        let original = content.clone();
        assert_eq!(content.pop(), Some('\n'));
        assert_eq!(content.pop(), Some('\r'));
        assert_eq!(content.pop(), Some('🙂'));
        content.push('猫');
        content.push_str("\n");
        assert_eq!(content, "a中猫\n");
        assert_eq!(original, "a中🙂\r\n");
        assert!(!original.same_version(&content));
    }
}
