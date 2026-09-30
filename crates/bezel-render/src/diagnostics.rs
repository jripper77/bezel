//! Problems found while drawing (missing assets, unknown fonts, bad
//! images). A frame never fails for one element: the renderer draws what it
//! can and logs each distinct problem once, not on every refresh.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Distinct messages remembered before the memory is reset.
const MEMORY: usize = 4096;

/// Logs each distinct warning once.
#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    seen: HashSet<String>,
}

impl Diagnostics {
    /// Logs `message` with `tracing` unless it was already logged.
    pub fn warn(&mut self, message: String) {
        if self.seen.len() >= MEMORY {
            self.seen.clear();
        }
        if !self.seen.contains(&message) {
            tracing::warn!(target: "bezel_render", "{message}");
            self.seen.insert(message);
        }
    }

    /// Number of distinct problems logged (tests).
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.seen.len()
    }

    /// True when a logged message contains `needle` (tests).
    #[cfg(test)]
    pub fn mentions(&self, needle: &str) -> bool {
        self.seen.iter().any(|m| m.contains(needle))
    }
}

/// Identity of asset bytes, cheap enough to check on every frame: the
/// buffer's address and length plus a strided sample decide in the common
/// case (the same map passed again); when they differ the full contents are
/// hashed, so bytes that merely moved are not decoded again and replaced
/// bytes always are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp {
    addr: usize,
    len: usize,
    sample: u64,
    content: u64,
}

fn sample_hash(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    let edge = bytes.len().min(256);
    bytes[..edge].hash(&mut h);
    bytes[bytes.len() - edge..].hash(&mut h);
    let step = (bytes.len() / 256).max(1);
    for b in bytes.iter().step_by(step) {
        b.hash(&mut h);
    }
    h.finish()
}

fn content_hash(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

impl Stamp {
    /// Stamps `bytes`.
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            addr: bytes.as_ptr() as usize,
            len: bytes.len(),
            sample: sample_hash(bytes),
            content: content_hash(bytes),
        }
    }

    /// True when `bytes` hold the stamped contents (re-stamping bytes that
    /// moved to another buffer).
    pub fn matches(&mut self, bytes: &[u8]) -> bool {
        let sample = sample_hash(bytes);
        if (bytes.as_ptr() as usize, bytes.len(), sample) == (self.addr, self.len, self.sample) {
            return true;
        }
        let fresh = Self::new(bytes);
        let same = (fresh.len, fresh.content) == (self.len, self.content);
        if same {
            *self = fresh;
        }
        same
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_once_per_message() {
        let mut d = Diagnostics::default();
        d.warn("a".into());
        d.warn("a".into());
        d.warn("b".into());
        assert_eq!(d.count(), 2);
        assert!(d.mentions("b"));
        for i in 0..MEMORY + 1 {
            d.warn(format!("m{i}"));
        }
        assert!(d.count() <= MEMORY);
    }

    #[test]
    fn stamps_follow_contents_not_buffers() {
        let a = vec![1u8; 10_000];
        let mut stamp = Stamp::new(&a);
        assert!(stamp.matches(&a), "same buffer");
        let moved = a.clone();
        assert!(stamp.matches(&moved), "same bytes elsewhere");
        assert!(stamp.matches(&moved), "re-stamped");
        let mut edited = a.clone();
        edited[5001] = 2;
        assert!(!stamp.matches(&edited), "an edit off the sample");
        assert!(!Stamp::new(&[]).matches(&[0]));
    }
}
