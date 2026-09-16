//! Decode terminal graphics payloads into GPUI `RenderImage`s.
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;

use gpui::RenderImage;
use image::{Frame, ImageReader, Limits};
use smallvec::SmallVec;

/// Process-wide decode cache keyed by payload fingerprint.
static DECODE_CACHE: Mutex<Option<DecodeCache>> = Mutex::new(None);
static DECODE_WORKERS: OnceLock<DecodeWorkerPool> = OnceLock::new();

const MAX_DECODE_CACHE_ENTRIES: usize = 128;
const MAX_DECODE_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PENDING_DECODE_ENTRIES: usize = 32;
const MAX_PENDING_DECODE_BYTES: u64 = 16 * 1024 * 1024;
const DECODE_WORKER_COUNT: usize = 2;
const DECODE_QUEUE_CAP: usize = MAX_PENDING_DECODE_ENTRIES;
const MAX_DECODED_IMAGE_DIMENSION: u32 = 4096;
const MAX_DECODED_IMAGE_BYTES: u64 = 4 * 1024 * 1024;

struct DecodeCache {
    entries: HashMap<DecodeCacheKey, CachedDecode>,
    lru: VecDeque<DecodeCacheKey>,
    decoded_bytes: u64,
    pending_count: usize,
    pending_bytes: u64,
    completed_tx: mpsc::Sender<CompletedDecode>,
    completed_rx: mpsc::Receiver<CompletedDecode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DecodeCacheKey {
    content_id: u64,
}

struct CachedDecode {
    state: DecodeCacheEntryState,
    decoded_bytes: u64,
    input_bytes: u64,
}

enum DecodeCacheEntryState {
    Pending,
    Ready(Option<Arc<RenderImage>>),
}

struct CompletedDecode {
    key: DecodeCacheKey,
    image: Option<Arc<RenderImage>>,
}

struct DecodeJob {
    key: DecodeCacheKey,
    data: Arc<[u8]>,
    completed_tx: mpsc::Sender<CompletedDecode>,
}

struct DecodeWorkerPool {
    jobs: mpsc::SyncSender<DecodeJob>,
}

#[derive(Clone)]
pub enum CachedRenderImage {
    Ready(Arc<RenderImage>),
    Pending,
    Failed,
}

impl Default for DecodeCache {
    fn default() -> Self {
        let (completed_tx, completed_rx) = mpsc::channel();
        Self {
            entries: HashMap::new(),
            lru: VecDeque::new(),
            decoded_bytes: 0,
            pending_count: 0,
            pending_bytes: 0,
            completed_tx,
            completed_rx,
        }
    }
}

impl DecodeCache {
    fn pump_completed(&mut self) {
        while let Ok(completed) = self.completed_rx.try_recv() {
            self.insert_ready(completed.key, completed.image);
        }
    }

    fn get(&mut self, key: DecodeCacheKey) -> Option<CachedRenderImage> {
        self.pump_completed();
        let state = match &self.entries.get(&key)?.state {
            DecodeCacheEntryState::Pending => CachedRenderImage::Pending,
            DecodeCacheEntryState::Ready(Some(image)) => CachedRenderImage::Ready(image.clone()),
            DecodeCacheEntryState::Ready(None) => CachedRenderImage::Failed,
        };
        self.touch(key);
        Some(state)
    }

    fn insert_pending(&mut self, key: DecodeCacheKey, input_bytes: u64) -> bool {
        if self.entries.contains_key(&key) {
            return false;
        }
        if self.pending_count >= MAX_PENDING_DECODE_ENTRIES
            || self.pending_bytes.saturating_add(input_bytes) > MAX_PENDING_DECODE_BYTES
        {
            return false;
        }
        while self.entries.len() >= MAX_DECODE_CACHE_ENTRIES {
            if !self.evict_oldest_ready() {
                return false;
            }
        }
        self.entries.insert(
            key,
            CachedDecode {
                state: DecodeCacheEntryState::Pending,
                decoded_bytes: 0,
                input_bytes,
            },
        );
        self.pending_count = self.pending_count.saturating_add(1);
        self.pending_bytes = self.pending_bytes.saturating_add(input_bytes);
        self.touch(key);
        true
    }

    fn insert_ready(&mut self, key: DecodeCacheKey, image: Option<Arc<RenderImage>>) {
        self.insert_ready_with_limits(key, image, MAX_DECODE_CACHE_ENTRIES, MAX_DECODE_CACHE_BYTES);
    }

    fn insert_ready_with_limits(
        &mut self,
        key: DecodeCacheKey,
        image: Option<Arc<RenderImage>>,
        max_entries: usize,
        max_bytes: u64,
    ) {
        let decoded_bytes = image
            .as_deref()
            .map(render_image_decoded_bytes)
            .unwrap_or(0);
        if let Some(replaced) = self.entries.insert(
            key,
            CachedDecode {
                state: DecodeCacheEntryState::Ready(image),
                decoded_bytes,
                input_bytes: 0,
            },
        ) {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(replaced.decoded_bytes);
            if matches!(replaced.state, DecodeCacheEntryState::Pending) {
                self.pending_count = self.pending_count.saturating_sub(1);
                self.pending_bytes = self.pending_bytes.saturating_sub(replaced.input_bytes);
            }
        }
        self.decoded_bytes = self.decoded_bytes.saturating_add(decoded_bytes);
        self.touch(key);
        while self.entries.len() > max_entries || self.decoded_bytes > max_bytes {
            if !self.evict_oldest_ready() {
                break;
            }
        }
    }

    fn remove(&mut self, key: DecodeCacheKey) {
        if let Some(removed) = self.entries.remove(&key) {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(removed.decoded_bytes);
            if matches!(removed.state, DecodeCacheEntryState::Pending) {
                self.pending_count = self.pending_count.saturating_sub(1);
                self.pending_bytes = self.pending_bytes.saturating_sub(removed.input_bytes);
            }
        }
        self.lru.retain(|candidate| *candidate != key);
    }

    fn evict_oldest_ready(&mut self) -> bool {
        let candidates = self.lru.len();
        for _ in 0..candidates {
            let Some(key) = self.lru.pop_front() else {
                return false;
            };
            let is_pending = self
                .entries
                .get(&key)
                .is_some_and(|entry| matches!(entry.state, DecodeCacheEntryState::Pending));
            if is_pending {
                self.lru.push_back(key);
                continue;
            }
            if let Some(removed) = self.entries.remove(&key) {
                self.decoded_bytes = self.decoded_bytes.saturating_sub(removed.decoded_bytes);
                return true;
            }
        }
        false
    }

    fn touch(&mut self, key: DecodeCacheKey) {
        self.lru.retain(|candidate| *candidate != key);
        self.lru.push_back(key);
    }

    fn completed_sender(&self) -> mpsc::Sender<CompletedDecode> {
        self.completed_tx.clone()
    }
}

impl DecodeWorkerPool {
    fn new() -> Self {
        let (jobs, receiver) = mpsc::sync_channel::<DecodeJob>(DECODE_QUEUE_CAP);
        let receiver = Arc::new(Mutex::new(receiver));
        for worker_index in 0..DECODE_WORKER_COUNT {
            let receiver = Arc::clone(&receiver);
            thread::Builder::new()
                .name(format!("zzclawterm-image-decode-{worker_index}"))
                .spawn(move || {
                    loop {
                        let job = receiver
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .recv();
                        let Ok(job) = job else {
                            break;
                        };
                        let image = decode_render_image(&job.data);
                        let _ = job.completed_tx.send(CompletedDecode {
                            key: job.key,
                            image,
                        });
                    }
                })
                .expect("spawn terminal image decode worker");
        }
        Self { jobs }
    }

    fn try_schedule(&self, job: DecodeJob) -> Result<(), DecodeJob> {
        self.jobs.try_send(job).map_err(|error| match error {
            mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job) => job,
        })
    }
}

fn render_image_decoded_bytes(image: &RenderImage) -> u64 {
    (0..image.frame_count())
        .filter_map(|frame| {
            let size = image.size(frame);
            decoded_rgba_bytes(u32::from(size.width), u32::from(size.height))
        })
        .fold(0u64, u64::saturating_add)
}

fn cache() -> std::sync::MutexGuard<'static, Option<DecodeCache>> {
    DECODE_CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

fn cache_key(_placement_id: u64, content_id: u64) -> DecodeCacheKey {
    DecodeCacheKey { content_id }
}

fn decode_workers() -> &'static DecodeWorkerPool {
    DECODE_WORKERS.get_or_init(DecodeWorkerPool::new)
}

/// Decode encoded image bytes (NYAR RGBA / PNG/JPEG/GIF/BMP) into a BGRA `RenderImage`.
pub fn decode_render_image(data: &[u8]) -> Option<Arc<RenderImage>> {
    if data.is_empty() {
        return None;
    }
    let mut rgba = if let Some((w, h, raw)) = unpack_nyar(data) {
        image::RgbaImage::from_raw(w, h, raw)?
    } else {
        decode_compressed_image(data)?
    };
    // GPUI atlas expects BGRA.
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let frames = SmallVec::from_elem(Frame::new(rgba), 1);
    Some(Arc::new(RenderImage::new(frames)))
}

fn decode_compressed_image(data: &[u8]) -> Option<image::RgbaImage> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    reader.limits(decode_limits());
    let image = reader.decode().ok()?;
    let width = image.width();
    let height = image.height();
    decoded_rgba_bytes(width, height)?;
    Some(image.into_rgba8())
}

fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODED_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODED_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_IMAGE_BYTES);
    limits
}

fn decoded_rgba_bytes(width: u32, height: u32) -> Option<u64> {
    if width == 0
        || height == 0
        || width > MAX_DECODED_IMAGE_DIMENSION
        || height > MAX_DECODED_IMAGE_DIMENSION
    {
        return None;
    }
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))?
        .checked_mul(4)?;
    (bytes <= MAX_DECODED_IMAGE_BYTES).then_some(bytes)
}

/// ZzClawTerm intermediate raster container produced by the Sixel decoder:
/// `NYAR` + width:u32le + height:u32le + RGBA8 pixels.
fn unpack_nyar(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if data.len() < 12 || &data[..4] != b"ZZCT" {
        return None;
    }
    let width = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let height = u32::from_le_bytes(data[8..12].try_into().ok()?);
    let need = usize::try_from(decoded_rgba_bytes(width, height)?).ok()?;
    if data.len() < 12 + need {
        return None;
    }
    Some((width, height, data[12..12 + need].to_vec()))
}

/// Cached decode for a payload. First access schedules decode work and returns
/// pending; later accesses return ready or failed state.
pub fn cached_render_image(
    placement_id: u64,
    content_id: u64,
    data: Arc<[u8]>,
) -> CachedRenderImage {
    if data.is_empty() {
        return CachedRenderImage::Failed;
    }
    let key = cache_key(placement_id, content_id);
    {
        let mut guard = cache();
        let cache = guard.get_or_insert_with(DecodeCache::default);
        if let Some(hit) = cache.get(key) {
            return hit;
        }
        let input_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
        if !cache.insert_pending(key, input_bytes) {
            return CachedRenderImage::Pending;
        }
        let job = DecodeJob {
            key,
            data,
            completed_tx: cache.completed_sender(),
        };
        if decode_workers().try_schedule(job).is_err() {
            cache.remove(key);
        }
    }

    CachedRenderImage::Pending
}

#[cfg(test)]
fn cached_render_image_poll(placement_id: u64, content_id: u64) -> CachedRenderImage {
    let key = cache_key(placement_id, content_id);
    {
        let mut guard = cache();
        let cache = guard.get_or_insert_with(DecodeCache::default);
        if let Some(hit) = cache.get(key) {
            return hit;
        }
    }
    CachedRenderImage::Pending
}

#[cfg(test)]
mod tests {
    use gpui::RenderImage;

    use super::{
        CachedRenderImage, DecodeCache, MAX_DECODE_CACHE_ENTRIES, MAX_DECODED_IMAGE_BYTES,
        MAX_PENDING_DECODE_BYTES, MAX_PENDING_DECODE_ENTRIES, cache, cache_key,
        cached_render_image, cached_render_image_poll, decode_render_image, decoded_rgba_bytes,
    };
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    static TEST_CACHE_LOCK: Mutex<()> = Mutex::new(());

    fn clear_cache() {
        *cache() = None;
    }

    fn tiny_nyar(pixel: [u8; 4]) -> Vec<u8> {
        let mut data = b"ZZCT".to_vec();
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&pixel);
        data
    }

    fn tiny_png() -> Vec<u8> {
        // 1x1 red PNG
        let rgba = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        png_from_rgba(rgba)
    }

    fn png_from_rgba(rgba: image::RgbaImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut bytes);
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .expect("encode png");
        bytes
    }

    fn content_id(data: &[u8]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        data.hash(&mut hasher);
        hasher.finish()
    }

    fn arc_payload(data: &[u8]) -> Arc<[u8]> {
        Arc::from(data.to_vec())
    }

    fn wait_ready(placement_id: u64, data: &[u8]) -> Arc<RenderImage> {
        let id = content_id(data);
        match cached_render_image(placement_id, id, arc_payload(data)) {
            CachedRenderImage::Ready(image) => return image,
            CachedRenderImage::Pending => {}
            CachedRenderImage::Failed => panic!("decode failed before pending"),
        }
        let started = Instant::now();
        loop {
            match cached_render_image_poll(placement_id, id) {
                CachedRenderImage::Ready(image) => return image,
                CachedRenderImage::Pending if started.elapsed() < Duration::from_secs(2) => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                CachedRenderImage::Pending => panic!("decode did not complete"),
                CachedRenderImage::Failed => panic!("decode failed"),
            }
        }
    }

    fn wait_failed(placement_id: u64, data: &[u8]) {
        let id = content_id(data);
        match cached_render_image(placement_id, id, arc_payload(data)) {
            CachedRenderImage::Failed => return,
            CachedRenderImage::Pending => {}
            CachedRenderImage::Ready(_) => panic!("invalid payload decoded"),
        }
        let started = Instant::now();
        loop {
            match cached_render_image_poll(placement_id, id) {
                CachedRenderImage::Failed => return,
                CachedRenderImage::Pending if started.elapsed() < Duration::from_secs(2) => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                CachedRenderImage::Pending => panic!("decode failure did not complete"),
                CachedRenderImage::Ready(_) => panic!("invalid payload decoded"),
            }
        }
    }

    #[test]
    fn decodes_png_to_render_image() {
        let png = tiny_png();
        let image = decode_render_image(&png).expect("decode");
        assert_eq!(image.frame_count(), 1);
        let size = image.size(0);
        assert_eq!(u32::from(size.width), 1);
        assert_eq!(u32::from(size.height), 1);
    }

    #[test]
    fn rejects_png_that_expands_past_decode_budget() {
        let width = 1025;
        let height = 1025;
        assert!(decoded_rgba_bytes(width, height).is_none());

        let rgba = image::RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]));
        let png = png_from_rgba(rgba);
        assert!(png.len() as u64 <= MAX_DECODED_IMAGE_BYTES);
        assert!(decode_render_image(&png).is_none());
    }

    #[test]
    fn rejects_nyar_that_expands_past_decode_budget() {
        let mut data = b"ZZCT".to_vec();
        data.extend_from_slice(&1025u32.to_le_bytes());
        data.extend_from_slice(&1025u32.to_le_bytes());
        assert!(decode_render_image(&data).is_none());
    }

    #[test]
    fn cache_returns_same_arc_for_same_payload() {
        let _guard = TEST_CACHE_LOCK.lock().unwrap();
        clear_cache();
        let png = tiny_png();
        let a = wait_ready(42, &png);
        let b = wait_ready(42, &png);
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn cache_key_reuses_content_across_placements() {
        assert_eq!(cache_key(42, 7), cache_key(42, 7));
        assert_eq!(cache_key(42, 7), cache_key(43, 7));
        assert_ne!(cache_key(42, 7), cache_key(42, 8));
    }

    #[test]
    fn cache_returns_same_arc_for_same_content_across_placements() {
        let _guard = TEST_CACHE_LOCK.lock().unwrap();
        clear_cache();
        let png = tiny_png();
        let a = wait_ready(42, &png);
        let b = wait_ready(43, &png);
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn pending_decode_budget_is_bounded() {
        let mut cache = DecodeCache::default();
        let input_bytes = MAX_PENDING_DECODE_BYTES / MAX_PENDING_DECODE_ENTRIES as u64;
        for content_id in 0..MAX_PENDING_DECODE_ENTRIES as u64 {
            assert!(cache.insert_pending(cache_key(1, content_id), input_bytes));
        }
        assert!(!cache.insert_pending(cache_key(1, u64::MAX), 1));
        assert_eq!(cache.pending_count, MAX_PENDING_DECODE_ENTRIES);
        assert_eq!(cache.pending_bytes, MAX_PENDING_DECODE_BYTES);
    }

    #[test]
    fn cache_prunes_least_recently_used_image() {
        let _guard = TEST_CACHE_LOCK.lock().unwrap();
        clear_cache();
        let first = tiny_nyar([0, 0, 0, 255]);
        let first_image = wait_ready(1, &first);
        let mut second_image = None;
        for placement_id in 2..=MAX_DECODE_CACHE_ENTRIES as u64 {
            let value = placement_id as u8;
            let payload = tiny_nyar([value, 0, 0, 255]);
            let image = wait_ready(placement_id, &payload);
            if placement_id == 2 {
                second_image = Some(image);
            }
        }
        assert!(Arc::ptr_eq(&first_image, &wait_ready(1, &first)));

        let second = tiny_nyar([2, 0, 0, 255]);
        let second_image = second_image.expect("second before eviction");
        let overflow = tiny_nyar([255, 0, 0, 255]);
        wait_ready(999, &overflow);

        assert!(Arc::ptr_eq(&first_image, &wait_ready(1, &first)));
        assert!(!Arc::ptr_eq(&second_image, &wait_ready(2, &second)));
    }

    #[test]
    fn different_content_ids_do_not_share_cache_entries() {
        let _guard = TEST_CACHE_LOCK.lock().unwrap();
        clear_cache();
        let mut a = vec![0u8; 192];
        let mut b = vec![0u8; 192];
        a[96] = 1;
        b[96] = 2;

        assert!(matches!(
            cached_render_image(42, content_id(&a), arc_payload(&a)),
            CachedRenderImage::Pending
        ));
        assert!(matches!(
            cached_render_image(42, content_id(&b), arc_payload(&b)),
            CachedRenderImage::Pending
        ));
        let mut guard = cache();
        let cache = guard.as_mut().expect("cache exists");
        assert!(cache.entries.contains_key(&cache_key(42, content_id(&a))));
        assert!(cache.entries.contains_key(&cache_key(42, content_id(&b))));
    }

    #[test]
    fn cache_remembers_decode_failures() {
        let _guard = TEST_CACHE_LOCK.lock().unwrap();
        clear_cache();
        let invalid = b"not an image";
        let key = cache_key(42, content_id(invalid));

        wait_failed(42, invalid);
        let mut guard = cache();
        let cached = guard
            .as_mut()
            .and_then(|cache| cache.get(key))
            .expect("cached failure entry");
        assert!(matches!(cached, CachedRenderImage::Failed));
    }

    #[test]
    fn cache_prunes_images_to_decoded_byte_budget() {
        let first = decode_render_image(&tiny_nyar([1, 0, 0, 255])).expect("first");
        let second = decode_render_image(&tiny_nyar([2, 0, 0, 255])).expect("second");
        let mut cache = DecodeCache::default();

        cache.insert_ready_with_limits(cache_key(1, 1), Some(first), 10, 4);
        cache.insert_ready_with_limits(cache_key(2, 2), Some(second), 10, 4);

        assert!(!cache.entries.contains_key(&cache_key(1, 1)));
        assert!(cache.entries.contains_key(&cache_key(2, 2)));
        assert_eq!(cache.decoded_bytes, 4);
    }

    #[test]
    fn decodes_nyar_rgba_from_sixel_path() {
        // 1x1 blue pixel packed as NYAR.
        let data = tiny_nyar([0, 0, 255, 255]);
        let image = decode_render_image(&data).expect("decode nyar");
        assert_eq!(image.frame_count(), 1);
        let size = image.size(0);
        assert_eq!(u32::from(size.width), 1);
        assert_eq!(u32::from(size.height), 1);
    }
}
