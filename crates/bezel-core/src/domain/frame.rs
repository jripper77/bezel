//! Frames: what gets shown on a screen, and how two frames differ.
//!
//! A [`Frame`] is RGBA8 with straight (non-premultiplied) alpha, laid out in
//! the orientation the user looks at (the theme's canvas). Device adapters
//! rotate and convert pixels to their panel's native format.

use super::geometry::Size;

/// An RGBA color with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (255 = opaque).
    pub a: u8,
}

impl Rgba {
    /// Opaque black.
    pub const BLACK: Rgba = Rgba::opaque(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Rgba = Rgba::opaque(255, 255, 255);

    /// An opaque color.
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// An axis-aligned rectangle in pixels. Empty when width or height is zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

impl Rect {
    /// Builds a rectangle.
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The rectangle covering a whole canvas.
    pub const fn of(size: Size) -> Self {
        Self::new(0, 0, size.width, size.height)
    }

    /// True when it covers no pixel.
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// One past the right edge.
    pub const fn right(self) -> u32 {
        self.x + self.width
    }

    /// One past the bottom edge.
    pub const fn bottom(self) -> u32 {
        self.y + self.height
    }

    /// Its size.
    pub const fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    /// The part inside `bounds` (empty when disjoint).
    pub fn clip(self, bounds: Size) -> Rect {
        let right = self.right().min(bounds.width);
        let bottom = self.bottom().min(bounds.height);
        if self.x >= right || self.y >= bottom {
            return Rect::default();
        }
        Rect::new(self.x, self.y, right - self.x, bottom - self.y)
    }

    /// Smallest rectangle containing both.
    pub fn union(self, other: Rect) -> Rect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    /// Number of pixels.
    pub const fn area(self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// A full canvas of RGBA8 pixels.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    size: Size,
    pixels: Vec<u8>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// Bytes per RGBA8 pixel.
pub const RGBA_BYTES: usize = 4;

impl Frame {
    /// A frame filled with one color.
    pub fn filled(size: Size, color: Rgba) -> Self {
        let pixels = [color.r, color.g, color.b, color.a].repeat(size.area() as usize);
        Self { size, pixels }
    }

    /// Wraps RGBA8 bytes; `None` when the length does not match the size.
    pub fn from_rgba(size: Size, pixels: Vec<u8>) -> Option<Self> {
        (pixels.len() as u64 == size.area() * RGBA_BYTES as u64).then_some(Self { size, pixels })
    }

    /// Canvas size.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Raw RGBA8 bytes, row-major, top-left first.
    pub fn as_rgba(&self) -> &[u8] {
        &self.pixels
    }

    /// Mutable raw bytes (renderers draw into them).
    pub fn as_rgba_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// The color at `(x, y)`, `None` outside the canvas.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba> {
        if x >= self.size.width || y >= self.size.height {
            return None;
        }
        let i = (y as usize * self.size.width as usize + x as usize) * RGBA_BYTES;
        let p = &self.pixels[i..i + RGBA_BYTES];
        Some(Rgba {
            r: p[0],
            g: p[1],
            b: p[2],
            a: p[3],
        })
    }

    /// Paints a rectangle with a solid color (clipped to the canvas).
    pub fn fill_rect(&mut self, rect: Rect, color: Rgba) {
        let rect = rect.clip(self.size);
        let px = [color.r, color.g, color.b, color.a];
        for y in rect.y..rect.bottom() {
            let row = self.row_range(y, rect.x, rect.width);
            for chunk in self.pixels[row].as_chunks_mut::<RGBA_BYTES>().0 {
                *chunk = px;
            }
        }
    }

    /// Copies a sub-rectangle out as its own frame (clipped to the canvas).
    pub fn crop(&self, rect: Rect) -> Frame {
        let rect = rect.clip(self.size);
        let mut pixels = Vec::with_capacity(rect.area() as usize * RGBA_BYTES);
        for y in rect.y..rect.bottom() {
            pixels.extend_from_slice(&self.pixels[self.row_range(y, rect.x, rect.width)]);
        }
        Frame {
            size: rect.size(),
            pixels,
        }
    }

    /// This frame turned clockwise by `quarter_turns` × 90° (lossless).
    pub fn rotated(&self, quarter_turns: u8) -> Frame {
        let turns = quarter_turns % 4;
        if turns == 0 {
            return self.clone();
        }
        let (w, h) = (self.size.width as usize, self.size.height as usize);
        let size = if turns == 2 {
            self.size
        } else {
            self.size.transposed()
        };
        let mut pixels = vec![0u8; self.pixels.len()];
        for y in 0..h {
            for x in 0..w {
                let (nx, ny) = match turns {
                    1 => (h - 1 - y, x),
                    2 => (w - 1 - x, h - 1 - y),
                    _ => (y, w - 1 - x),
                };
                let src = (y * w + x) * RGBA_BYTES;
                let dst = (ny * size.width as usize + nx) * RGBA_BYTES;
                pixels[dst..dst + RGBA_BYTES].copy_from_slice(&self.pixels[src..src + RGBA_BYTES]);
            }
        }
        Frame { size, pixels }
    }

    fn row_range(&self, y: u32, x: u32, width: u32) -> std::ops::Range<usize> {
        let start = (y as usize * self.size.width as usize + x as usize) * RGBA_BYTES;
        start..start + width as usize * RGBA_BYTES
    }
}

/// Side of the square tiles [`dirty_rects`] compares.
pub const DIFF_TILE: u32 = 16;

/// The rectangles that changed from `previous` to `next`, as a short list of
/// non-overlapping rectangles aligned to [`DIFF_TILE`] tiles (clipped to the
/// canvas). Frames of different sizes differ everywhere.
///
/// Changed tiles are merged into horizontal runs per tile row, and runs with
/// the same span on consecutive rows are merged vertically, which turns a
/// changing text or bar into one rectangle.
pub fn dirty_rects(previous: &Frame, next: &Frame) -> Vec<Rect> {
    if previous.size != next.size {
        return vec![Rect::of(next.size)];
    }
    let size = next.size;
    let cols = size.width.div_ceil(DIFF_TILE);
    let rows = size.height.div_ceil(DIFF_TILE);
    let mut open: Vec<Rect> = Vec::new();
    let mut done: Vec<Rect> = Vec::new();

    for row in 0..rows {
        let runs = changed_runs(previous, next, row, cols);
        let (continued, closed): (Vec<Rect>, Vec<Rect>) = open
            .into_iter()
            .partition(|r| runs.iter().any(|&(x, w)| r.x == x && r.width == w));
        done.extend(closed);
        let mut next_open = Vec::with_capacity(runs.len());
        for (x, w) in runs {
            match continued.iter().find(|r| r.x == x && r.width == w) {
                Some(r) => next_open.push(Rect::new(r.x, r.y, r.width, r.height + DIFF_TILE)),
                None => next_open.push(Rect::new(x, row * DIFF_TILE, w, DIFF_TILE)),
            }
        }
        open = next_open;
    }
    done.extend(open);
    let mut rects: Vec<Rect> = done.into_iter().map(|r| r.clip(size)).collect();
    rects.sort_by_key(|r| (r.y, r.x));
    rects
}

/// Changed tiles of one tile row, merged into `(x, width)` pixel spans.
fn changed_runs(previous: &Frame, next: &Frame, tile_row: u32, cols: u32) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for col in 0..cols {
        if !tile_changed(previous, next, col, tile_row) {
            continue;
        }
        let x = col * DIFF_TILE;
        match runs.last_mut() {
            Some((start, width)) if *start + *width == x => *width += DIFF_TILE,
            _ => runs.push((x, DIFF_TILE)),
        }
    }
    runs
}

fn tile_changed(previous: &Frame, next: &Frame, col: u32, row: u32) -> bool {
    let tile = Rect::new(col * DIFF_TILE, row * DIFF_TILE, DIFF_TILE, DIFF_TILE).clip(next.size);
    (tile.y..tile.bottom()).any(|y| {
        let range = next.row_range(y, tile.x, tile.width);
        previous.pixels[range.clone()] != next.pixels[range]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgba = Rgba::opaque(255, 0, 0);

    #[test]
    fn rect_clip_union_area() {
        let bounds = Size::new(100, 50);
        assert_eq!(
            Rect::new(90, 40, 20, 20).clip(bounds),
            Rect::new(90, 40, 10, 10)
        );
        assert!(Rect::new(100, 0, 5, 5).clip(bounds).is_empty());
        assert_eq!(
            Rect::new(0, 0, 10, 10).union(Rect::new(20, 5, 5, 10)),
            Rect::new(0, 0, 25, 15)
        );
        assert_eq!(
            Rect::default().union(Rect::new(1, 2, 3, 4)),
            Rect::new(1, 2, 3, 4)
        );
        assert_eq!(
            Rect::new(1, 2, 3, 4).union(Rect::default()),
            Rect::new(1, 2, 3, 4)
        );
        assert_eq!(Rect::of(bounds).area(), 5000);
    }

    #[test]
    fn frame_fill_crop_and_pixels() {
        let mut f = Frame::filled(Size::new(8, 4), Rgba::BLACK);
        f.fill_rect(Rect::new(6, 2, 10, 10), RED);
        assert_eq!(f.pixel(7, 3), Some(RED));
        assert_eq!(f.pixel(5, 3), Some(Rgba::BLACK));
        assert_eq!(f.pixel(8, 0), None);
        let c = f.crop(Rect::new(6, 2, 2, 2));
        assert_eq!(c.size(), Size::new(2, 2));
        assert!(c.as_rgba().chunks(4).all(|p| p == [255, 0, 0, 255]));
        assert!(Frame::from_rgba(Size::new(2, 2), vec![0; 15]).is_none());
        assert!(Frame::from_rgba(Size::new(2, 2), vec![0; 16]).is_some());
        f.as_rgba_mut()[0] = 9;
        assert_eq!(f.pixel(0, 0).map(|p| p.r), Some(9));
        assert!(format!("{f:?}").contains("Frame"));
    }

    #[test]
    fn rotation_moves_the_corners_clockwise() {
        // 3x2: red at top-left, blue at bottom-right.
        let mut f = Frame::filled(Size::new(3, 2), Rgba::BLACK);
        f.fill_rect(Rect::new(0, 0, 1, 1), RED);
        f.fill_rect(Rect::new(2, 1, 1, 1), Rgba::opaque(0, 0, 255));
        let r1 = f.rotated(1);
        assert_eq!(r1.size(), Size::new(2, 3));
        assert_eq!(r1.pixel(1, 0), Some(RED), "top-left goes to top-right");
        assert_eq!(r1.pixel(0, 2), Some(Rgba::opaque(0, 0, 255)));
        let r2 = f.rotated(2);
        assert_eq!(r2.pixel(2, 1), Some(RED), "top-left goes to bottom-right");
        let r3 = f.rotated(3);
        assert_eq!(r3.pixel(0, 2), Some(RED), "top-left goes to bottom-left");
        assert_eq!(f.rotated(4), f);
        assert_eq!(r1.rotated(3), f);
    }

    #[test]
    fn identical_frames_have_no_dirty_rects() {
        let a = Frame::filled(Size::new(480, 1920), Rgba::WHITE);
        assert!(dirty_rects(&a, &a.clone()).is_empty());
    }

    #[test]
    fn a_changed_block_becomes_one_tile_aligned_rect() {
        let a = Frame::filled(Size::new(480, 1920), Rgba::BLACK);
        let mut b = a.clone();
        b.fill_rect(Rect::new(20, 40, 50, 30), RED);
        assert_eq!(dirty_rects(&a, &b), vec![Rect::new(16, 32, 64, 48)]);
    }

    #[test]
    fn separate_changes_stay_separate_and_edges_are_clipped() {
        let a = Frame::filled(Size::new(100, 40), Rgba::BLACK);
        let mut b = a.clone();
        b.fill_rect(Rect::new(0, 0, 1, 1), RED);
        b.fill_rect(Rect::new(99, 39, 1, 1), RED);
        assert_eq!(
            dirty_rects(&a, &b),
            vec![Rect::new(0, 0, 16, 16), Rect::new(96, 32, 4, 8)]
        );
    }

    #[test]
    fn an_l_shape_splits_where_the_span_changes() {
        let a = Frame::filled(Size::new(64, 64), Rgba::BLACK);
        let mut b = a.clone();
        b.fill_rect(Rect::new(0, 0, 32, 16), RED);
        b.fill_rect(Rect::new(0, 16, 16, 16), RED);
        assert_eq!(
            dirty_rects(&a, &b),
            vec![Rect::new(0, 0, 32, 16), Rect::new(0, 16, 16, 16)]
        );
    }

    #[test]
    fn different_sizes_differ_everywhere() {
        let a = Frame::filled(Size::new(4, 4), Rgba::BLACK);
        let b = Frame::filled(Size::new(8, 4), Rgba::BLACK);
        assert_eq!(dirty_rects(&a, &b), vec![Rect::new(0, 0, 8, 4)]);
    }
}
