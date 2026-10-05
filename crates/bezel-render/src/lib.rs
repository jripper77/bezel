//! Bezel's frame renderer: draws a theme into a frame of its canvas size.
//!
//! One renderer serves the studio preview and the screens, so what the
//! editor shows is what the panel gets (D-2026-09-30-render-engine-2):
//! tiny-skia for anti-aliased paths, gradients and compositing, cosmic-text
//! for shaping with the theme's bundled fonts first and the installed fonts
//! after, and image for PNG, JPEG and animated GIF decoding.
//!
//! Elements are drawn bottom to top, each into a layer the size of its box
//! (which clips it) that is then composited with the element's opacity. A
//! missing asset, font or unreadable image never fails the frame: the
//! element is drawn as far as possible and the problem is logged once with
//! `tracing`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod composite;
mod diagnostics;
mod fonts;
mod gauges;
mod graph;
mod images;
mod layer;
mod paint;
mod path;
mod renderer;
mod shape;
mod svg;
mod text;

#[cfg(test)]
mod golden;
#[cfg(test)]
mod testkit;

pub use fonts::font_files;
pub use renderer::SkiaRenderer;
pub use svg::{svg_size, svg_thumbnail};
pub use text::SystemFonts;

#[cfg(test)]
mod tests {
    use crate::{golden, testkit};

    #[test]
    fn every_element_kind_matches_its_golden() {
        let mut renderer = testkit::renderer();
        golden::backgrounds(&mut renderer);
        golden::texts(&mut renderer);
        golden::images(&mut renderer);
        golden::shapes(&mut renderer);
        golden::bars(&mut renderer);
        golden::rings(&mut renderer);
        golden::needles(&mut renderer);
        golden::graphs(&mut renderer);
        golden::composition(&mut renderer);
    }
}
