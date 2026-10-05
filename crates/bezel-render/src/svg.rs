//! Vector image parsing and thumbnails, without external image resources.

use resvg::usvg;
use tiny_skia::{Pixmap, Transform};

pub(crate) fn tree(bytes: &[u8]) -> Option<usvg::Tree> {
    if bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    usvg::Tree::from_data(bytes, &options).ok()
}

/// Intrinsic dimensions of a valid SVG. External resources are ignored.
pub fn svg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let size = tree(bytes)?.size().to_int_size();
    Some((size.width(), size.height()))
}

/// A transparent PNG thumbnail of a vector image, fitted within `side`.
pub fn svg_thumbnail(bytes: &[u8], side: u32) -> Option<Vec<u8>> {
    if !(1..=1024).contains(&side) {
        return None;
    }
    let tree = tree(bytes)?;
    let size = tree.size();
    let scale = side as f32 / size.width().max(size.height());
    let mut pixels = Pixmap::new(
        (size.width() * scale).round().max(1.0) as u32,
        (size.height() * scale).round().max(1.0) as u32,
    )?;
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixels.as_mut(),
    );
    pixels.encode_png().ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_shadow_renders_color_and_soft_transparency() {
        let png = super::svg_thumbnail(include_bytes!("icon-shadow.svg"), 240).unwrap();
        let pixels = tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert!(
            pixels
                .pixels()
                .iter()
                .any(|p| p.blue() > p.red() && p.alpha() > 200)
        );
        assert!(
            pixels
                .pixels()
                .iter()
                .any(|p| p.red() > p.blue() && p.alpha() > 0 && p.alpha() < 200)
        );
    }
}
