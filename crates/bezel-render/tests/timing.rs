//! Render budget: a typical 480x1920 theme (the showcase, ~30 elements) must
//! render in 15 ms or less per frame in a release build, with the renderer
//! reused across frames.
//!
//! ```text
//! cargo test --release -p bezel-render --test timing -- --ignored
//! ```

#[path = "support/showcase.rs"]
mod showcase;

use std::time::{Duration, Instant};

use bezel_core::ports::FrameRenderer;
use bezel_render::SkiaRenderer;

const BUDGET: Duration = Duration::from_millis(15);
const FRAMES: u32 = 40;

#[test]
#[ignore = "timing; run in release: cargo test --release -p bezel-render --test timing -- --ignored"]
fn typical_portrait_theme_renders_within_budget() {
    let theme = showcase::theme().expect("theme");
    let scene = showcase::Scene::new(&theme).expect("scene");
    let mut renderer = SkiaRenderer::new();
    for _ in 0..3 {
        renderer
            .render(&theme, &scene.assets, scene.context())
            .expect("warm-up");
    }
    let mut times = Vec::with_capacity(FRAMES as usize);
    for _ in 0..FRAMES {
        let started = Instant::now();
        let frame = renderer
            .render(&theme, &scene.assets, scene.context())
            .expect("frame");
        times.push(started.elapsed());
        assert_eq!(frame.size(), theme.canvas);
    }
    times.sort();
    let median = times[times.len() / 2];
    let mean = times.iter().sum::<Duration>() / FRAMES;
    eprintln!(
        "{} elements: median {median:.2?}, mean {mean:.2?}, max {:.2?}",
        theme.elements.len(),
        times[times.len() - 1]
    );
    if cfg!(debug_assertions) {
        eprintln!("debug build: the {BUDGET:?} budget applies to release builds");
        return;
    }
    assert!(mean <= BUDGET, "mean {mean:?} over the {BUDGET:?} budget");
}
