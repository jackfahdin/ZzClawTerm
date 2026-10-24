use std::fmt;
use std::io;
use std::sync::{Arc, Mutex};

use redb::StorageBackend;
use zeroize::Zeroizing;

// The wire format is still a redb database. Keeping its backing bytes in memory
// avoids temporary plaintext files and filesystem work for every cloud check.
#[derive(Clone, Default)]
pub(super) struct SnapshotMemory(Arc<Mutex<Zeroizing<Vec<u8>>>>);

const MAX_DATABASE_BYTES: usize = 64 * 1024 * 1024;

impl SnapshotMemory {
    pub(super) fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_DATABASE_BYTES {
            return Err(invalid_range());
        }
        Ok(Self(Arc::new(Mutex::new(Zeroizing::new(bytes.to_vec())))))
    }

    pub(super) fn bytes(&self) -> Vec<u8> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .to_vec()
    }
}

impl fmt::Debug for SnapshotMemory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnapshotMemory")
            .finish_non_exhaustive()
    }
}

fn invalid_range() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "snapshot database exceeds its bounds",
    )
}

impl StorageBackend for SnapshotMemory {
    fn len(&self) -> io::Result<u64> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len() as u64)
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        let offset = usize::try_from(offset).map_err(|_| invalid_range())?;
        let end = offset.checked_add(out.len()).ok_or_else(invalid_range)?;
        let bytes = self.0.lock().unwrap_or_else(|error| error.into_inner());
        out.copy_from_slice(bytes.get(offset..end).ok_or_else(invalid_range)?);
        Ok(())
    }

    fn set_len(&self, len: u64) -> io::Result<()> {
        let len = usize::try_from(len).map_err(|_| invalid_range())?;
        if len > MAX_DATABASE_BYTES {
            return Err(invalid_range());
        }
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .resize(len, 0);
        Ok(())
    }

    fn sync_data(&self) -> io::Result<()> {
        Ok(())
    }

    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        let offset = usize::try_from(offset).map_err(|_| invalid_range())?;
        let end = offset.checked_add(data.len()).ok_or_else(invalid_range)?;
        let mut bytes = self.0.lock().unwrap_or_else(|error| error.into_inner());
        bytes
            .get_mut(offset..end)
            .ok_or_else(invalid_range)?
            .copy_from_slice(data);
        Ok(())
    }
}
