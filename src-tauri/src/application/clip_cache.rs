//! A cache of decoded clips so that triggering a sound again is instant (ADR 0006).
//!
//! It holds at most a byte budget of samples and evicts the least recently used clip first. Clips are decoded for one output
//! format, so a change of the device's format empties the cache.

use std::collections::HashMap;
use std::sync::Arc;

use crate::audio::backend::OutputFormat;
use crate::audio::clip::Clip;

struct Entry {
    clip: Arc<Clip>,
    last_used: u64,
}

pub struct ClipCache {
    budget_bytes: usize,
    used_bytes: usize,
    format: Option<OutputFormat>,
    entries: HashMap<i64, Entry>,
    tick: u64,
}

fn size_of(clip: &Clip) -> usize {
    std::mem::size_of_val(clip.samples())
}

impl ClipCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            used_bytes: 0,
            format: None,
            entries: HashMap::new(),
            tick: 0,
        }
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The cached clip for `sound`, if it was decoded for `format`.
    pub fn get(&mut self, sound: i64, format: OutputFormat) -> Option<Arc<Clip>> {
        self.adopt(format);
        self.tick += 1;
        let tick = self.tick;
        let entry = self.entries.get_mut(&sound)?;
        entry.last_used = tick;
        Some(Arc::clone(&entry.clip))
    }

    /// Stores a clip, evicting older ones to stay within the budget. A clip larger than the whole budget is not stored.
    pub fn insert(&mut self, sound: i64, format: OutputFormat, clip: Arc<Clip>) {
        self.adopt(format);
        let size = size_of(&clip);
        if size > self.budget_bytes {
            return;
        }
        self.remove(sound);
        while self.used_bytes + size > self.budget_bytes {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.remove(oldest);
        }
        self.tick += 1;
        self.used_bytes += size;
        self.entries.insert(
            sound,
            Entry {
                clip,
                last_used: self.tick,
            },
        );
    }

    pub fn remove(&mut self, sound: i64) {
        if let Some(entry) = self.entries.remove(&sound) {
            self.used_bytes -= size_of(&entry.clip);
        }
    }

    fn adopt(&mut self, format: OutputFormat) {
        if self.format != Some(format) {
            self.entries.clear();
            self.used_bytes = 0;
            self.format = Some(format);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEREO_48K: OutputFormat = OutputFormat {
        sample_rate: 48_000,
        channels: 2,
    };
    const MONO_44K: OutputFormat = OutputFormat {
        sample_rate: 44_100,
        channels: 1,
    };

    /// A clip of `kib` kibibytes.
    fn clip(kib: usize, format: OutputFormat) -> Arc<Clip> {
        let samples = kib * 1024 / 4;
        let samples = samples - samples % usize::from(format.channels);
        Clip::new(vec![0.0; samples], format.channels, format.sample_rate).expect("clip")
    }

    #[test]
    fn a_stored_clip_is_returned_and_accounted() {
        let mut cache = ClipCache::new(10 * 1024);
        assert!(cache.get(1, STEREO_48K).is_none());
        cache.insert(1, STEREO_48K, clip(4, STEREO_48K));
        assert!(cache.get(1, STEREO_48K).is_some());
        assert_eq!((cache.len(), cache.used_bytes()), (1, 4 * 1024));
    }

    #[test]
    fn the_least_recently_used_clip_goes_first_when_the_budget_is_exceeded() {
        let mut cache = ClipCache::new(10 * 1024);
        cache.insert(1, STEREO_48K, clip(4, STEREO_48K));
        cache.insert(2, STEREO_48K, clip(4, STEREO_48K));
        assert!(
            cache.get(1, STEREO_48K).is_some(),
            "touching 1 makes 2 the oldest"
        );
        cache.insert(3, STEREO_48K, clip(4, STEREO_48K));

        assert!(cache.get(2, STEREO_48K).is_none(), "2 was evicted");
        assert!(cache.get(1, STEREO_48K).is_some());
        assert!(cache.get(3, STEREO_48K).is_some());
        assert!(cache.used_bytes() <= 10 * 1024);
    }

    #[test]
    fn a_clip_larger_than_the_budget_is_not_stored_and_evicts_nothing() {
        let mut cache = ClipCache::new(10 * 1024);
        cache.insert(1, STEREO_48K, clip(4, STEREO_48K));
        cache.insert(2, STEREO_48K, clip(11, STEREO_48K));
        assert!(cache.get(2, STEREO_48K).is_none());
        assert!(cache.get(1, STEREO_48K).is_some());
    }

    #[test]
    fn replacing_a_clip_does_not_double_count_it() {
        let mut cache = ClipCache::new(10 * 1024);
        cache.insert(1, STEREO_48K, clip(4, STEREO_48K));
        cache.insert(1, STEREO_48K, clip(6, STEREO_48K));
        assert_eq!((cache.len(), cache.used_bytes()), (1, 6 * 1024));
    }

    #[test]
    fn a_new_output_format_empties_the_cache() {
        let mut cache = ClipCache::new(10 * 1024);
        cache.insert(1, STEREO_48K, clip(4, STEREO_48K));
        assert!(
            cache.get(1, MONO_44K).is_none(),
            "clips decoded for another format are useless"
        );
        assert_eq!((cache.len(), cache.used_bytes()), (0, 0));
        assert!(
            cache.get(1, STEREO_48K).is_none(),
            "and they are gone, not parked"
        );
    }

    #[test]
    fn remove_frees_the_budget() {
        let mut cache = ClipCache::new(10 * 1024);
        cache.insert(1, STEREO_48K, clip(4, STEREO_48K));
        cache.remove(1);
        cache.remove(1);
        assert!(cache.is_empty());
        assert_eq!(cache.used_bytes(), 0);
    }
}
