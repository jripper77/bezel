//! Renders the showcase theme (480x1920) with fixed sensor values to a PNG
//! and prints the render time.
//!
//! ```text
//! cargo run --release -p bezel-render --example showcase -- out.png
//! ```

#[path = "../tests/support/showcase.rs"]
mod showcase;

use std::time::{Duration, Instant};

use bezel_core::ports::FrameRenderer;
use bezel_render::SkiaRenderer;

/// Frames timed after the first one.
const FRAMES: u32 = 30;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "showcase.png".to_string());
    let theme = showcase::theme().ok_or("invalid showcase theme")?;
    let scene = showcase::Scene::new(&theme).ok_or("invalid showcase scene")?;

    let started = Instant::now();
    let mut renderer = SkiaRenderer::new();
    let fonts = started.elapsed();
    let started = Instant::now();
    let frame = renderer.render(&theme, &scene.assets, scene.context())?;
    let first = started.elapsed();
    let mut total = Duration::ZERO;
    for _ in 0..FRAMES {
        let started = Instant::now();
        renderer.render(&theme, &scene.assets, scene.context())?;
        total += started.elapsed();
    }

    let size = frame.size();
    let image = image::RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec())
        .ok_or("frame size does not match its bytes")?;
    image.save(&out)?;
    println!(
        "{out}: {}x{}, {} elements; fonts {fonts:.1?}, first frame {first:.1?}, then {:.2?} per frame",
        size.width,
        size.height,
        theme.elements.len(),
        total / FRAMES
    );
    Ok(())
}
