//! The raster cache: a [`Spec`] is rasterised once and its buffer kept
//! (keyed by the spec's content hash and the raster scale), so an
//! unchanged HUD costs a hash per element and nothing else. The same buffer
//! (same Smithay id) comes back on a hit, so the damage tracker sees no
//! change. LRU, at most [`MAX_ENTRIES`] buffers and [`MAX_BYTES`] of pixels.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

use super::draw::{self, Fonts, Px, Spec};

pub const MAX_ENTRIES: usize = 64;
pub const MAX_BYTES: usize = 24 << 20;

struct Entry {
    buf: MemoryRenderBuffer,
    /// logical px
    size: (i32, i32),
    buf_scale: f64,
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

/// The cache key of a spec at a raster scale.
pub fn key(spec: &Spec, scale: Px) -> u64 {
    let mut h = DefaultHasher::new();
    spec.hash(&mut h);
    spec.scale(scale).key().hash(&mut h);
    h.finish()
}

/// How a raster at scale `s` becomes a buffer: (the buffer's own integer
/// scale, `OverlayElem.buf_scale`). 1× and 2× keep today's integer buffer
/// (`buf_scale` 1); a fractional scale is a scale-1 buffer of physical px
/// the hub draws at `buf px / buf_scale` logical px.
pub fn buffer_scale(s: Px) -> (i32, f64) {
    let k = s.key();
    if k >= 120 && k % 120 == 0 {
        (k / 120, 1.0)
    } else {
        (1, s.0 as f64)
    }
}

/// One cached raster as the frame uses it.
#[derive(Clone, Debug)]
pub struct Got {
    pub buf: MemoryRenderBuffer,
    /// logical px
    pub size: (i32, i32),
    pub buf_scale: f64,
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

    /// The buffer of `spec` at the output's raster scale `s` (the spec may
    /// pick its own, [`Spec::scale`]), its logical size and buffer scale;
    /// rasterised on a miss.
    pub fn get(&mut self, spec: &Spec, s: Px, fonts: &mut Fonts) -> Got {
        self.clock += 1;
        let k = key(spec, s);
        if let Some(e) = self.map.get_mut(&k) {
            e.used = self.clock;
            self.hits += 1;
            return Got {
                buf: e.buf.clone(),
                size: e.size,
                buf_scale: e.buf_scale,
            };
        }
        self.rasters += 1;
        let s = spec.scale(s);
        let r = draw::raster(spec, fonts, s);
        let (int, buf_scale) = buffer_scale(s);
        let buf = MemoryRenderBuffer::from_slice(
            &r.px,
            Fourcc::Abgr8888,
            (r.w as i32, r.h as i32),
            int,
            Transform::Normal,
            None,
        );
        let logical = |n: u32| (n as f64 / int as f64 / buf_scale).round() as i32;
        let size = (logical(r.w), logical(r.h));
        let bytes = r.px.len();
        self.bytes += bytes;
        self.map.insert(
            k,
            Entry {
                buf: buf.clone(),
                size,
                buf_scale,
                bytes,
                used: self.clock,
            },
        );
        self.evict(k);
        Got {
            buf,
            size,
            buf_scale,
        }
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

    const ONE: Px = Px(1.0);

    /// A buffer's Smithay id (its Debug starts with it).
    fn id(b: &MemoryRenderBuffer) -> String {
        let s = format!("{b:?}");
        s[..s.find("inner").unwrap_or(s.len())].to_string()
    }

    #[test]
    fn hits_and_misses() {
        let mut f = Fonts::new();
        let mut c = Cache::new();
        let a = c.get(&label("NAVEGAR"), ONE, &mut f);
        let b = c.get(&label("NAVEGAR"), ONE, &mut f);
        assert_eq!((c.rasters, c.hits), (1, 1));
        assert_eq!(id(&a.buf), id(&b.buf), "the same buffer on a hit");
        assert_eq!(a.size, b.size);
        assert_eq!(a.buf_scale, 1.0);
        c.get(&label("ANDAR"), ONE, &mut f);
        c.get(&label("NAVEGAR"), Px(2.0), &mut f);
        assert_eq!((c.rasters, c.hits), (3, 1));
        let s2 = c.get(&label("NAVEGAR"), Px(2.0), &mut f);
        assert!((s2.size.0 - a.size.0).abs() <= 8, "logical size kept at 2×");
        assert_eq!(s2.buf_scale, 1.0, "2×: an integer buffer, as before");
        assert_eq!(c.len(), 3);
    }

    /// 1.25: its own raster (physical px), a scale-1 buffer with
    /// `buf_scale` 1.25, the same logical size as at 1×; hits after that.
    #[test]
    fn fractional_scale_is_its_own_entry() {
        let mut f = Fonts::new();
        let mut c = Cache::new();
        let a = c.get(&label("ANDAR"), ONE, &mut f);
        let q = c.get(&label("ANDAR"), Px(1.25), &mut f);
        assert_eq!(c.rasters, 2, "a miss at the new scale");
        assert_eq!(q.buf_scale, 1.25);
        assert!(
            (q.size.0 - a.size.0).abs() <= 4,
            "{:?} {:?}",
            q.size,
            a.size
        );
        assert!((q.size.1 - a.size.1).abs() <= 2);
        let q2 = c.get(&label("ANDAR"), Px(1.25), &mut f);
        assert_eq!((c.rasters, c.hits), (2, 1));
        assert_eq!(id(&q.buf), id(&q2.buf));
        // the vignette draws at ¼ whatever the output scale: one entry
        let v = Spec::Vignette {
            w: 1280,
            h: 720,
            a16: 5,
        };
        let v1 = c.get(&v, ONE, &mut f);
        let v2 = c.get(&v, Px(1.25), &mut f);
        assert_eq!(id(&v1.buf), id(&v2.buf));
        assert_eq!(v1.buf_scale, 0.25);
        assert_eq!(v1.size, (1280, 720));
    }

    #[test]
    fn buffer_scales() {
        let rows = [
            (1.0, (1, 1.0)),
            (2.0, (2, 1.0)),
            (3.0, (3, 1.0)),
            (1.25, (1, 1.25)),
            (1.5, (1, 1.5)),
            (1.75, (1, 1.75)),
            (2.5, (1, 2.5)),
            (0.25, (1, 0.25)),
        ];
        for (s, want) in rows {
            assert_eq!(buffer_scale(Px(s)), want, "{s}");
        }
    }

    #[test]
    fn lru_keeps_at_most_64() {
        let mut f = Fonts::new();
        let mut c = Cache::new();
        for i in 0..80 {
            c.get(&label(&format!("{i}")), ONE, &mut f);
            // keep the first one hot
            c.get(&label("0"), ONE, &mut f);
        }
        assert_eq!(c.len(), MAX_ENTRIES);
        let before = c.rasters;
        c.get(&label("0"), ONE, &mut f);
        assert_eq!(c.rasters, before, "the hot one survived");
        c.get(&label("1"), ONE, &mut f);
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
            c.get(&spec, ONE, &mut f);
        }
        assert!(c.bytes() <= MAX_BYTES, "{}", c.bytes());
        assert!(c.len() < 8);
    }
}
