//! CPU pixel ownership and the application-wide remote desktop budget.
use std::fmt;
use std::io;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

pub const PIXEL_MEMORY_LIMIT: usize = 1024 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct PixelBudget {
    used: AtomicUsize,
    limit: usize,
}

impl PixelBudget {
    #[cfg(test)]
    pub(crate) fn test(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            used: AtomicUsize::new(0),
            limit,
        })
    }
    #[cfg(test)]
    pub(crate) fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }

    pub(crate) fn shared() -> &'static Arc<Self> {
        static BUDGET: OnceLock<Arc<PixelBudget>> = OnceLock::new();
        BUDGET.get_or_init(|| {
            Arc::new(Self {
                used: AtomicUsize::new(0),
                limit: PIXEL_MEMORY_LIMIT,
            })
        })
    }

    pub(crate) fn reserve(self: &Arc<Self>, bytes: usize) -> io::Result<PixelLease> {
        let mut used = self.used.load(Ordering::Acquire);
        loop {
            let Some(total) = used.checked_add(bytes).filter(|total| *total <= self.limit) else {
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "remote desktop CPU pixel memory budget exhausted",
                ));
            };
            match self
                .used
                .compare_exchange_weak(used, total, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(current) => used = current,
            }
        }
        Ok(PixelLease {
            budget: Arc::clone(self),
            bytes,
        })
    }
}

#[derive(Debug)]
pub(crate) struct PixelLease {
    budget: Arc<PixelBudget>,
    bytes: usize,
}

impl Drop for PixelLease {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

struct Storage {
    pixels: Vec<u8>,
    _lease: Option<Arc<PixelLease>>,
}

/// Cloning shares immutable pixels and their single allocation lease.
#[derive(Clone)]
pub struct PixelBuffer {
    storage: Arc<Storage>,
    offset: usize,
}

impl PixelBuffer {
    pub(crate) fn skip(mut self, bytes: usize) -> Self {
        self.offset += bytes;
        self
    }

    pub(crate) fn with_lease(pixels: Vec<u8>, lease: Option<Arc<PixelLease>>) -> Self {
        Self {
            storage: Arc::new(Storage {
                pixels,
                _lease: lease,
            }),
            offset: 0,
        }
    }
}

// Helper-owned decoded pixels retain their existing per-process limits. Parent
// IPC reception uses with_lease before a server-controlled allocation is made.
impl From<Vec<u8>> for PixelBuffer {
    fn from(pixels: Vec<u8>) -> Self {
        Self::with_lease(pixels, None)
    }
}
impl Default for PixelBuffer {
    fn default() -> Self {
        Vec::new().into()
    }
}
impl Deref for PixelBuffer {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.storage.pixels[self.offset..]
    }
}
impl AsRef<[u8]> for PixelBuffer {
    fn as_ref(&self) -> &[u8] {
        self
    }
}
impl PartialEq for PixelBuffer {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl Eq for PixelBuffer {}
impl<const N: usize> PartialEq<[u8; N]> for PixelBuffer {
    fn eq(&self, other: &[u8; N]) -> bool {
        self.as_ref() == other
    }
}
impl PartialEq<Vec<u8>> for PixelBuffer {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.as_ref() == other
    }
}
impl fmt::Debug for PixelBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PixelBuffer")
            .field("bytes", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{PixelBudget, PixelBuffer};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[test]
    fn ownership_moves_and_clones_retain_one_lease_until_last_drop() {
        let budget = Arc::new(PixelBudget {
            used: AtomicUsize::new(0),
            limit: 16,
        });
        let pixels =
            PixelBuffer::with_lease(vec![0; 16], Some(Arc::new(budget.reserve(16).unwrap())));
        assert!(budget.reserve(1).is_err());
        let retained = pixels.clone();
        drop(pixels);
        assert_eq!(budget.used.load(Ordering::Acquire), 16);
        drop(retained);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
        assert!(budget.reserve(16).is_ok());
    }
}
