//! A test pattern that makes orientation and partial updates visible.

use super::frame::{Frame, Rect, Rgba};
use super::geometry::Size;

/// Corner marker colors, clockwise from the top-left: red, green, white, blue.
pub const CORNERS: [Rgba; 4] = [
    Rgba::opaque(255, 0, 0),
    Rgba::opaque(0, 200, 0),
    Rgba::WHITE,
    Rgba::opaque(0, 80, 255),
];

const BARS: [Rgba; 8] = [
    Rgba::opaque(255, 255, 255),
    Rgba::opaque(255, 255, 0),
    Rgba::opaque(0, 255, 255),
    Rgba::opaque(0, 255, 0),
    Rgba::opaque(255, 0, 255),
    Rgba::opaque(255, 0, 0),
    Rgba::opaque(0, 0, 255),
    Rgba::opaque(0, 0, 0),
];

/// Color bars along the long side, a marker in each corner (see [`CORNERS`]),
/// and a moving strip whose position is `step` — so consecutive steps differ
/// only in a small band, which exercises partial updates.
pub fn test_pattern(size: Size, step: u32) -> Frame {
    let mut frame = Frame::filled(size, Rgba::opaque(24, 24, 32));
    let landscape = size.width > size.height;
    let long = if landscape { size.width } else { size.height };
    let short = if landscape { size.height } else { size.width };
    let band = long / BARS.len() as u32;
    for (i, color) in BARS.iter().enumerate() {
        let start = i as u32 * band;
        let rect = if landscape {
            Rect::new(start, short / 4, band, short / 2)
        } else {
            Rect::new(short / 4, start, short / 2, band)
        };
        frame.fill_rect(rect, *color);
    }
    let m = (short / 8).max(4);
    let corners = [
        (0, 0),
        (size.width - m, 0),
        (size.width - m, size.height - m),
        (0, size.height - m),
    ];
    for ((x, y), color) in corners.into_iter().zip(CORNERS) {
        frame.fill_rect(Rect::new(x, y, m, m), color);
    }
    let strip = (m / 2).max(2);
    let travel = long.saturating_sub(strip).max(1);
    let pos = (step * strip) % travel;
    let rect = if landscape {
        Rect::new(pos, 0, strip, m / 2)
    } else {
        Rect::new(0, pos, m / 2, strip)
    };
    frame.fill_rect(rect, Rgba::opaque(255, 200, 0));
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::frame::dirty_rects;

    #[test]
    fn corners_are_where_the_legend_says() {
        let size = Size::new(480, 1920);
        let f = test_pattern(size, 0);
        assert_eq!(f.pixel(40, 40), Some(CORNERS[0]));
        assert_eq!(f.pixel(479, 10), Some(CORNERS[1]));
        assert_eq!(f.pixel(479, 1919), Some(CORNERS[2]));
        assert_eq!(f.pixel(0, 1919), Some(CORNERS[3]));
        assert_eq!(f.pixel(240, 0), Some(BARS[0]));
        let wide = test_pattern(size.transposed(), 3);
        assert_eq!(wide.pixel(1919, 0), Some(CORNERS[1]));
        assert_eq!(wide.pixel(0, 240), Some(BARS[0]));
    }

    #[test]
    fn consecutive_steps_change_a_small_area() {
        let size = Size::new(480, 1920);
        let changed: u64 = dirty_rects(&test_pattern(size, 5), &test_pattern(size, 6))
            .iter()
            .map(|r| r.area())
            .sum();
        assert!(changed > 0 && changed < size.area() / 50, "{changed}");
        let tiny = test_pattern(Size::new(8, 8), 1_000_000);
        assert_eq!(tiny.size(), Size::new(8, 8));
    }
}
