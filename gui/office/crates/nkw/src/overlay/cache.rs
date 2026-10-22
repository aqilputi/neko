//! The raster cache: a [`Spec`] is rasterised once and its buffer kept
//! (keyed by the spec's content hash and the buffer scale), so an
//! unchanged HUD costs a hash per element and nothing else. The same buffer
//! (same Smithay id) comes back on a hit, so the damage tracker sees no
//! change. LRU, at most [`MAX_ENTRIES`] buffers and [`MAX_BYTES`] of pixels.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

use super::draw::{self, Fonts, Spec};

pub const MAX_ENTRIES: usize = 64;
pub const MAX_BYTES: usize = 24 << 20;

struct Entry {
    buf: MemoryRenderBuffer,
    /// logical px
    size: (i32, i32),
    bytes: usize,
    used: u64,
}

#[derive(Default)]
pub struct Cache {
    map: HashMap<u64, Entry>,
    clock: u64,
    bytes: usize,
    /// rasters made (misses)
    pub rasters: u64,
    pub hits: u64,
}

/// The cache key of a spec at a buffer scale.
pub fn key(spec: &Spec, scale: i32) -> u64 {
    let mut h = DefaultHasher::new();
    spec.hash(&mut h);
    scale.hash(&mut h);
    h.finish()
}

impl Cache {
    pub fn new() -> Cache {
        Cache::default()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The buffer of `spec` at buffer scale `s` and its logical size;
    /// rasterised on a miss.
    pub fn get(
        &mut self,
        spec: &Spec,
        s: i32,
        fonts: &mut Fonts,
    ) -> (MemoryRenderBuffer, (i32, i32)) {
        self.clock += 1;
        let k = key(spec, s);
        if let Some(e) = self.map.get_mut(&k) {
            e.used = self.clock;
            self.hits += 1;
            return (e.buf.clone(), e.size);
        }
        self.rasters += 1;
        let r = draw::raster(spec, fonts, s);
        let buf = MemoryRenderBuffer::from_slice(
            &r.px,
            Fourcc::Abgr8888,
            (r.w as i32, r.h as i32),
            s,
            Transform::Normal,
            None,
        );
        let size = (r.w as i32 / s, r.h as i32 / s);
        let bytes = r.px.len();
        self.bytes += bytes;
        self.map.insert(
            k,
            Entry {
                buf: buf.clone(),
                size,
                bytes,
                used: self.clock,
            },
        );
        self.evict(k);
        (buf, size)
    }

    /// Drop the least recently used entries over the limits (never `keep`).
    fn evict(&mut self, keep: u64) {
        while self.map.len() > MAX_ENTRIES || (self.bytes > MAX_BYTES && self.map.len() > 1) {
            let Some(old) = self
                .map
                .iter()
                .filter(|(k, _)| **k != keep)
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| *k)
            else {
                return;
            };
            if let Some(e) = self.map.remove(&old) {
                self.bytes -= e.bytes;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(t: &str) -> Spec {
        Spec::Label { text: t.into() }
    }

    #[test]
    fn hits_and_misses() {
        let mut f = Fonts::new();
        let mut c = Cache::new();
        let (a, sa) = c.get(&label("NAVEGAR"), 1, &mut f);
        let (b, sb) = c.get(&label("NAVEGAR"), 1, &mut f);
        assert_eq!((c.rasters, c.hits), (1, 1));
        assert_eq!(format!("{a:?}").len(), format!("{b:?}").len());
        assert_eq!(sa, sb);
        c.get(&label("ANDAR"), 1, &mut f);
        c.get(&label("NAVEGAR"), 2, &mut f);
        assert_eq!((c.rasters, c.hits), (3, 1));
        let (_, s2) = c.get(&label("NAVEGAR"), 2, &mut f);
        assert!((s2.0 - sa.0).abs() <= 8, "logical size kept at scale 2");
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn lru_keeps_at_most_64() {
        let mut f = Fonts::new();
        let mut c = Cache::new();
        for i in 0..80 {
            c.get(&label(&format!("{i}")), 1, &mut f);
            // keep the first one hot
            c.get(&label("0"), 1, &mut f);
        }
        assert_eq!(c.len(), MAX_ENTRIES);
        let before = c.rasters;
        c.get(&label("0"), 1, &mut f);
        assert_eq!(c.rasters, before, "the hot one survived");
        c.get(&label("1"), 1, &mut f);
        assert_eq!(c.rasters, before + 1, "a cold one was evicted");
    }

    #[test]
    fn byte_budget_evicts_big_sheets() {
        let mut f = Fonts::new();
        let mut c = Cache::new();
        for i in 0..8 {
            let spec = Spec::Map {
                w: 1280,
                h: 720,
                title: format!("{i}"),
                help: String::new(),
                boxes: vec![],
            };
            c.get(&spec, 1, &mut f);
        }
        assert!(c.bytes() <= MAX_BYTES, "{}", c.bytes());
        assert!(c.len() < 8);
    }
}
