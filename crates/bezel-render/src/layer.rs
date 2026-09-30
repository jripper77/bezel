//! A drawing target covering part of the canvas. Elements draw in canvas
//! coordinates; the layer translates them to its own pixels, clips to its
//! bounds and is then composited onto the canvas with the element opacity.

use tiny_skia::{
    FillRule, FilterQuality, Mask, Paint, Path, Pattern, PixmapMut, PixmapRef, Rect, SpreadMode,
    Stroke, Transform,
};

use crate::composite::{self, Sprite, Target};

/// Part of the canvas an element draws into.
pub(crate) struct Layer<'a> {
    /// The layer pixels (premultiplied).
    pub pixmap: PixmapMut<'a>,
    /// Canvas x of the layer's left column.
    pub x: i32,
    /// Canvas y of the layer's top row.
    pub y: i32,
}

impl Layer<'_> {
    /// Canvas → layer coordinates.
    pub fn transform(&self) -> Transform {
        Transform::from_translate(-(self.x as f32), -(self.y as f32))
    }

    /// Fills a canvas-space path.
    pub fn fill(&mut self, path: &Path, paint: &Paint<'_>) {
        let ts = self.transform();
        self.pixmap
            .fill_path(path, paint, FillRule::Winding, ts, None);
    }

    /// Strokes a canvas-space path.
    pub fn stroke(&mut self, path: &Path, paint: &Paint<'_>, stroke: &Stroke) {
        let ts = self.transform();
        self.pixmap.stroke_path(path, paint, stroke, ts, None);
    }

    /// Coverage of a canvas-space path, as a mask the size of the layer.
    pub fn mask_of(&self, path: &Path) -> Option<Mask> {
        let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height())?;
        mask.fill_path(path, FillRule::Winding, true, self.transform());
        Some(mask)
    }

    /// Fills a canvas-space rectangle through `mask`.
    pub fn fill_rect_masked(&mut self, rect: Rect, paint: &Paint<'_>, mask: &Mask) {
        let ts = self.transform();
        self.pixmap.fill_rect(rect, paint, ts, Some(mask));
    }

    /// Draws a premultiplied image at canvas `(x, y)`, 1:1.
    pub fn blit(&mut self, image: PixmapRef<'_>, x: i32, y: i32) {
        let (width, height) = (self.pixmap.width(), self.pixmap.height());
        let mut target = Target {
            data: self.pixmap.data_mut(),
            width,
            height,
        };
        let sprite = Sprite {
            data: image.data(),
            width: image.width(),
            height: image.height(),
            x: x - self.x,
            y: y - self.y,
            opaque: false,
        };
        composite::blend(&mut target, &sprite, 255);
    }

    /// Draws an image through `placement` (image pixels → canvas), smoothly
    /// resampled and with anti-aliased edges.
    pub fn draw_transformed(&mut self, image: PixmapRef<'_>, placement: Transform) {
        let Some(rect) = Rect::from_xywh(0.0, 0.0, image.width() as f32, image.height() as f32)
        else {
            return;
        };
        let paint = Paint {
            shader: Pattern::new(
                image,
                SpreadMode::Pad,
                FilterQuality::Bicubic,
                1.0,
                Transform::identity(),
            ),
            anti_alias: true,
            ..Paint::default()
        };
        let ts = placement.post_concat(self.transform());
        self.pixmap.fill_rect(rect, &paint, ts, None);
    }
}
