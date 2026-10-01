//! Animated images (GIF): when each of their frames shows, and how often a
//! theme that shows one is drawn again.
//!
//! A theme is sampled and drawn every `refresh_seconds`; a visible animated
//! image adds frames of its own in between, at its frame times, at most
//! [`MAX_ANIMATION_FPS`] a second. Every frame shows the image as it is at
//! the moment the frame is drawn, so a screen slower than the animation
//! skips frames instead of falling behind (T-7.11).

use std::time::Duration;

/// Most frames a second a theme is drawn for its animated images: faster
/// GIFs skip frames.
pub const MAX_ANIMATION_FPS: u32 = 30;

/// Shortest time between two frames drawn for animated images
/// (1 / [`MAX_ANIMATION_FPS`] s).
pub const MIN_FRAME_STEP: Duration = Duration::from_nanos(1_000_000_000 / MAX_ANIMATION_FPS as u64);

/// When each frame of an animation shows: how long each one lasts, in
/// order. The animation loops from its first frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    delays: Vec<Duration>,
    total: Duration,
}

impl Timeline {
    /// The timeline of frames lasting `delays`; `None` for fewer than two
    /// frames or no time at all (a still image).
    pub fn new(delays: Vec<Duration>) -> Option<Self> {
        if delays.len() < 2 {
            return None;
        }
        let total: Duration = delays.iter().sum();
        (!total.is_zero()).then_some(Self { delays, total })
    }

    /// Number of frames.
    pub fn len(&self) -> usize {
        self.delays.len()
    }

    /// Always false: a timeline has at least two frames.
    pub fn is_empty(&self) -> bool {
        self.delays.is_empty()
    }

    /// How long one loop lasts.
    pub fn total(&self) -> Duration {
        self.total
    }

    /// The frame shown `elapsed` after the animation started, and when it
    /// ends (on the same clock as `elapsed`).
    fn locate(&self, elapsed: Duration) -> (usize, Duration) {
        let total = self.total.as_nanos();
        let into = elapsed.as_nanos() % total;
        let loop_start = elapsed.as_nanos() - into;
        let mut start = 0u128;
        for (index, delay) in self.delays.iter().enumerate() {
            let end = start + delay.as_nanos();
            if into < end {
                return (index, nanos(loop_start + end));
            }
            start = end;
        }
        // Unreachable: `into` < the sum of the delays.
        (0, nanos(loop_start + total))
    }

    /// Index of the frame shown `elapsed` after the animation started (it
    /// loops). Frames lasting no time are never shown.
    pub fn frame_at(&self, elapsed: Duration) -> usize {
        self.locate(elapsed).0
    }

    /// When the frame shown at `elapsed` gives way to the next one: the
    /// first moment after `elapsed` at which [`Self::frame_at`] changes.
    pub fn next_change(&self, elapsed: Duration) -> Duration {
        self.locate(elapsed).1
    }
}

/// `n` nanoseconds as a duration, saturating far past any real clock.
fn nanos(n: u128) -> Duration {
    let secs = u64::try_from(n / 1_000_000_000).unwrap_or(u64::MAX);
    // The remainder is below 10^9, so it fits.
    Duration::new(secs, (n % 1_000_000_000) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn timeline(delays: &[u64]) -> Timeline {
        Timeline::new(delays.iter().map(|d| ms(*d)).collect()).expect("animated")
    }

    #[test]
    fn still_images_have_no_timeline() {
        assert_eq!(Timeline::new(vec![]), None);
        assert_eq!(Timeline::new(vec![ms(100)]), None);
        assert_eq!(Timeline::new(vec![ms(0), ms(0)]), None);
        let two = timeline(&[100, 50]);
        assert_eq!(
            (two.len(), two.total(), two.is_empty()),
            (2, ms(150), false)
        );
    }

    #[test]
    fn frames_follow_their_delays_and_loop() {
        let t = timeline(&[100, 50, 250]);
        for (at, frame) in [(0, 0), (99, 0), (100, 1), (149, 1), (150, 2), (399, 2)] {
            assert_eq!(t.frame_at(ms(at)), frame, "at {at} ms");
        }
        assert_eq!(t.frame_at(ms(400)), 0, "loops");
        assert_eq!(t.frame_at(ms(4_000 + 120)), 1);
    }

    #[test]
    fn the_next_change_is_the_end_of_the_frame_shown() {
        let t = timeline(&[100, 50, 250]);
        assert_eq!(t.next_change(ms(0)), ms(100));
        assert_eq!(t.next_change(ms(99)), ms(100));
        assert_eq!(t.next_change(ms(100)), ms(150));
        assert_eq!(t.next_change(ms(390)), ms(400), "the loop starts over");
        assert_eq!(t.next_change(ms(4_000 + 120)), ms(4_150));
        let long = Duration::from_secs(86_400 * 365) + ms(30);
        assert_eq!(
            t.next_change(long),
            Duration::from_secs(86_400 * 365) + ms(100)
        );
    }

    #[test]
    fn frames_lasting_no_time_are_skipped() {
        let t = timeline(&[100, 0, 100]);
        assert_eq!(t.frame_at(ms(100)), 2);
        assert_eq!(t.next_change(ms(50)), ms(100));
    }

    #[test]
    fn at_most_thirty_frames_a_second() {
        assert_eq!(MIN_FRAME_STEP.as_micros(), 33_333);
        assert_eq!(nanos(u128::MAX).as_secs(), u64::MAX);
    }
}
