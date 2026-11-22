//! Per-video cache: remembers thumbnails already decoded and keeps a simple
//! LRU so repeat-scrubbing the same spot never re-decodes.
//!
//! This is pure bookkeeping — no network or decode calls here. [`source`]
//! and [`codec`] (not yet wired up) are what actually produce a thumbnail;
//! this just decides whether that work is needed at all.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub struct ThumbnailCache {
    max_entries: usize,
    // Insertion order, oldest first, for O(1) eviction without a real LRU
    // structure — fine at the sizes this runs at (see [`Settings::cache`]).
    order: Vec<u64>,
    entries: HashMap<u64, Vec<u8>>,
}

/// Groups nearby scrub times into the same cache slot, so dragging by a
/// pixel doesn't miss a thumbnail decoded a moment ago for almost the same
/// spot. 1 second buckets is plenty finer than any seek bar's resolution.
const BUCKET_MS: u64 = 1000;

fn bucket(time_ms: u64) -> u64 {
    time_ms / BUCKET_MS
}

impl ThumbnailCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            order: Vec::new(),
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, time_ms: u64) -> Option<&[u8]> {
        self.entries.get(&bucket(time_ms)).map(|v| v.as_slice())
    }

    pub fn put(&mut self, time_ms: u64, jpeg: Vec<u8>) {
        let key = bucket(time_ms);
        if !self.entries.contains_key(&key) {
            self.order.push(key);
            if self.order.len() > self.max_entries {
                let oldest = self.order.remove(0);
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, jpeg);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

const MAX_ENTRIES_PER_URL: usize = 64;

static CACHES: OnceLock<Mutex<HashMap<String, ThumbnailCache>>> = OnceLock::new();

/// Looks up `time_ms` in the cache for `key` (one cache per URL, so
/// different videos' timestamps never collide). On a miss, runs `decode`,
/// caches a successful result, and returns it — on a hit, `decode` never
/// runs at all.
pub fn cached_or(key: &str, time_ms: u64, decode: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let caches = CACHES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = caches
        .lock()
        .unwrap()
        .get(key)
        .and_then(|c| c.get(time_ms))
        .map(|b| b.to_vec())
    {
        return Some(hit);
    }
    let result = decode()?;
    caches
        .lock()
        .unwrap()
        .entry(key.to_string())
        .or_insert_with(|| ThumbnailCache::new(MAX_ENTRIES_PER_URL))
        .put(time_ms, result.clone());
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn cached_or_only_decodes_once_per_time() {
        let key = "cached_or_only_decodes_once_per_time"; // unique: shares the global cache
        let calls = Cell::new(0);
        let decode = || {
            calls.set(calls.get() + 1);
            Some(vec![9u8])
        };
        assert_eq!(cached_or(key, 1_000, decode), Some(vec![9u8]));
        assert_eq!(cached_or(key, 1_000, decode), Some(vec![9u8]));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn hits_the_same_bucket_for_nearby_times() {
        let mut cache = ThumbnailCache::new(8);
        cache.put(5_200, vec![1, 2, 3]);
        assert_eq!(cache.get(5_200), Some(&[1u8, 2, 3][..]));
        assert_eq!(cache.get(5_600), Some(&[1u8, 2, 3][..])); // same 1s bucket
        assert_eq!(cache.get(6_200), None); // next bucket
    }

    #[test]
    fn evicts_the_oldest_entry_past_capacity() {
        let mut cache = ThumbnailCache::new(2);
        cache.put(0, vec![0]);
        cache.put(1_000, vec![1]);
        cache.put(2_000, vec![2]);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(0), None);
        assert_eq!(cache.get(2_000), Some(&[2u8][..]));
    }
}
