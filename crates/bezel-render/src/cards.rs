//! Card transition timing and whole-face poses, driven by the caller's clock.
use bezel_core::domain::theme::{
    BoxF, CardDirection, CardEffect, CardTransition, ElementId, Theme,
};
use std::collections::HashMap;
use std::time::Duration;
use tiny_skia::Transform;

#[derive(Clone)]
struct State {
    names: Vec<String>,
    active: usize,
    from: usize,
    started: Duration,
    settings: Option<CardTransition>,
}
#[derive(Default)]
pub(crate) struct Cards {
    states: HashMap<ElementId, State>,
    last: Duration,
    pub next: Option<Duration>,
    pub motion: bool,
}
#[derive(Clone, Copy)]
pub(crate) struct Transition {
    pub from: usize,
    pub to: usize,
    pub progress: f32,
    pub settings: CardTransition,
}
#[derive(Clone, Copy)]
pub(crate) struct Pose {
    pub face: usize,
    pub transform: Transform,
    pub alpha: f32,
    pub shade: f32,
    pub projection: Option<Projection>,
}
impl Cards {
    pub fn new() -> Self {
        Self {
            motion: true,
            ..Self::default()
        }
    }
    pub fn update(&mut self, theme: &Theme, now: Duration) -> HashMap<ElementId, Transition> {
        if now < self.last {
            self.states.clear();
        }
        self.last = now;
        self.next = None;
        self.states
            .retain(|id, _| theme.element(*id).is_some_and(|e| e.card.is_some()));
        let mut active = HashMap::new();
        for e in &theme.elements {
            let Some(card) = &e.card else {
                continue;
            };
            let state = self.states.entry(e.id).or_insert_with(|| State {
                names: card.faces.clone(),
                active: card.active_face,
                from: card.active_face,
                started: now,
                settings: card.transition,
            });
            if state.names != card.faces || !e.visible || !self.motion {
                *state = State {
                    names: card.faces.clone(),
                    active: card.active_face,
                    from: card.active_face,
                    started: now,
                    settings: card.transition,
                };
            }
            if state.settings != card.transition {
                state.from = state.active;
                state.settings = card.transition;
            }
            if state.active != card.active_face {
                // Interrupt rather than queue rapid changes; keep the latest request.
                let visible = if state.settings.is_some_and(|t| {
                    t.effect == CardEffect::Flip
                        && now.saturating_sub(state.started).as_millis()
                            < u128::from(t.duration_ms / 2)
                }) {
                    state.from
                } else {
                    state.active
                };
                state.from = visible;
                state.active = card.active_face;
                state.started = now;
            }
            let Some(settings) = card.transition.filter(|t| t.effect != CardEffect::None) else {
                continue;
            };
            let elapsed = now.saturating_sub(state.started);
            if state.from == state.active
                || elapsed >= Duration::from_millis(u64::from(settings.duration_ms))
            {
                state.from = state.active;
                continue;
            }
            let progress = elapsed.as_secs_f32() / (settings.duration_ms as f32 / 1000.0);
            active.insert(
                e.id,
                Transition {
                    from: state.from,
                    to: state.active,
                    progress,
                    settings,
                },
            );
            self.next = Some(now + Duration::from_millis(33));
        }
        active
    }
}
impl Transition {
    pub fn poses(self, b: BoxF) -> Vec<Pose> {
        let p = self.progress.clamp(0.0, 1.0);
        let p = p * p * (3.0 - 2.0 * p); // Smooth acceleration and deceleration.
        let horizontal = matches!(
            self.settings.direction,
            CardDirection::Left | CardDirection::Right
        );
        let sign = if matches!(
            self.settings.direction,
            CardDirection::Left | CardDirection::Up
        ) {
            -1.0
        } else {
            1.0
        };
        let identity = |face, alpha| Pose {
            face,
            alpha,
            transform: Transform::identity(),
            shade: 1.0,
            projection: None,
        };
        match self.settings.effect {
            CardEffect::None => vec![identity(self.to, 1.0)],
            CardEffect::Fade => vec![identity(self.from, 1.0 - p), identity(self.to, p)],
            CardEffect::Slide => [(self.from, p), (self.to, p - 1.0)]
                .into_iter()
                .map(|(face, travel)| Pose {
                    transform: Transform::from_translate(
                        if horizontal {
                            sign * travel * b.width
                        } else {
                            0.0
                        },
                        if horizontal {
                            0.0
                        } else {
                            sign * travel * b.height
                        },
                    ),
                    ..identity(face, 1.0)
                })
                .collect(),
            CardEffect::Flip => {
                let scale = (std::f32::consts::PI * p).cos().abs();
                let center = (b.x + b.width / 2.0, b.y + b.height / 2.0);
                let depth = sign
                    * (std::f32::consts::PI * p).sin()
                    * 0.22
                    * if p < 0.5 { 1.0 } else { -1.0 };
                let transform = if horizontal {
                    Transform::from_row(
                        scale.max(0.001),
                        0.0,
                        0.0,
                        1.0,
                        center.0 * (1.0 - scale),
                        0.0,
                    )
                } else {
                    Transform::from_row(
                        1.0,
                        0.0,
                        0.0,
                        scale.max(0.001),
                        0.0,
                        center.1 * (1.0 - scale),
                    )
                };
                vec![Pose {
                    face: if p < 0.5 { self.from } else { self.to },
                    transform,
                    shade: 0.84 + scale * 0.16,
                    projection: Some(Projection {
                        horizontal,
                        center,
                        half: if horizontal {
                            b.width / 2.0
                        } else {
                            b.height / 2.0
                        },
                        scale,
                        depth,
                    }),
                    alpha: if scale < 0.002 { 0.0 } else { 1.0 },
                }]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn transition(effect: CardEffect, progress: f32) -> Transition {
        Transition {
            from: 0,
            to: 1,
            progress,
            settings: CardTransition {
                effect,
                direction: CardDirection::Left,
                duration_ms: 600,
                include_base: false,
            },
        }
    }
    #[test]
    fn flip_never_mirrors_text_and_switches_at_the_edge() {
        let b = BoxF::new(10.0, 20.0, 100.0, 80.0);
        for i in 0..=100 {
            let pose = transition(CardEffect::Flip, i as f32 / 100.0).poses(b)[0];
            assert!(pose.transform.sx > 0.0);
            assert_eq!(pose.face, usize::from(i >= 50));
        }
        assert_eq!(transition(CardEffect::Flip, 0.5).poses(b)[0].alpha, 0.0);
    }
    #[test]
    fn perspective_has_unequal_edges_fixed_center_and_exact_endpoints() {
        let b = BoxF::new(20.0, 20.0, 100.0, 80.0);
        for direction in [
            CardDirection::Left,
            CardDirection::Right,
            CardDirection::Up,
            CardDirection::Down,
        ] {
            let mut t = transition(CardEffect::Flip, 0.25);
            t.settings.direction = direction;
            let projection = t.poses(b)[0].projection.unwrap();
            assert_eq!(projection.project(70.0, 60.0), (70.0, 60.0));
            let a = projection.project(20.0, 20.0);
            let opposite = projection.project(120.0, 100.0);
            if projection.horizontal {
                assert!((a.1 - 20.0).abs() > 1.0);
            } else {
                assert!((a.0 - 20.0).abs() > 1.0);
            }
            assert!(a.0 < opposite.0 && a.1 < opposite.1);
            for progress in [0.0, 1.0] {
                t.progress = progress;
                let p = t.poses(b)[0].projection.unwrap().project(20.0, 20.0);
                assert!((p.0 - 20.0).abs() < 0.001 && (p.1 - 20.0).abs() < 0.001);
            }
        }
    }
    #[test]
    fn inverse_warp_preserves_identity_and_transparent_pixels() {
        let mut source = tiny_skia::Pixmap::new(32, 24).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(120, 80, 40, 128));
        let mut target = tiny_skia::Pixmap::new(32, 24).unwrap();
        Projection {
            horizontal: true,
            center: (16.0, 12.0),
            half: 16.0,
            scale: 1.0,
            depth: 0.0,
        }
        .draw(&source, &mut target, 1.0, 1.0);
        assert_eq!(source.data(), target.data());
    }
    #[test]
    fn fade_weights_sum_to_one_and_slide_finishes_on_target() {
        let b = BoxF::new(0.0, 0.0, 100.0, 100.0);
        let poses = transition(CardEffect::Fade, 0.3).poses(b);
        assert!((poses.iter().map(|p| p.alpha).sum::<f32>() - 1.0).abs() < 0.001);
        assert_eq!(
            transition(CardEffect::Slide, 1.0).poses(b)[1].transform,
            Transform::identity()
        );
    }
}

/// Perspective around the card's central axis. The visible back face is
/// turned back toward the viewer, so text is never mirrored.
#[derive(Clone, Copy)]
pub(crate) struct Projection {
    pub horizontal: bool,
    pub center: (f32, f32),
    pub half: f32,
    pub scale: f32,
    pub depth: f32,
}
impl Projection {
    pub fn project(self, x: f32, y: f32) -> (f32, f32) {
        let (u, v) = (x - self.center.0, y - self.center.1);
        let d = 1.0 + self.depth * if self.horizontal { u } else { v } / self.half.max(1.0);
        (
            self.center.0 + u * if self.horizontal { self.scale } else { 1.0 } / d,
            self.center.1 + v * if self.horizontal { 1.0 } else { self.scale } / d,
        )
    }
    pub fn draw(
        self,
        source: &tiny_skia::Pixmap,
        target: &mut tiny_skia::Pixmap,
        shade: f32,
        alpha: f32,
    ) {
        if alpha == 0.0 || self.scale < 0.002 {
            return;
        }
        let (w, h) = (source.width() as usize, source.height() as usize);
        // Restrict the inverse warp to painted pixels, including strokes and shadows.
        let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
        for (i, pixel) in source.data().as_chunks::<4>().0.iter().enumerate() {
            if pixel[3] != 0 {
                left = left.min(i % w);
                right = right.max(i % w + 1);
                top = top.min(i / w);
                bottom = bottom.max(i / w + 1);
            }
        }
        if left >= right {
            return;
        }
        let corners = [(left, top), (right, top), (left, bottom), (right, bottom)]
            .map(|(x, y)| self.project(x as f32, y as f32));
        let x0 = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let x1 = corners
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(w as f32) as usize;
        let y0 = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let y1 = corners
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(h as f32) as usize;
        let pixels = source.data();
        let out = target.data_mut();
        for y in y0..y1 {
            for x in x0..x1 {
                let (u, v) = (
                    x as f32 + 0.5 - self.center.0,
                    y as f32 + 0.5 - self.center.1,
                );
                let axis = if self.horizontal { u } else { v };
                let divisor = self.scale - self.depth * axis / self.half.max(1.0);
                if divisor <= 0.0 {
                    continue;
                }
                let original = axis / divisor;
                let d = 1.0 + self.depth * original / self.half.max(1.0);
                let (sx, sy) = if self.horizontal {
                    (original + self.center.0 - 0.5, v * d + self.center.1 - 0.5)
                } else {
                    (u * d + self.center.0 - 0.5, original + self.center.1 - 0.5)
                };
                let (ix, iy) = (sx.floor() as i32, sy.floor() as i32);
                let (fx, fy) = (sx - sx.floor(), sy - sy.floor());
                let samples = [
                    (ix, iy, (1.0 - fx) * (1.0 - fy)),
                    (ix + 1, iy, fx * (1.0 - fy)),
                    (ix, iy + 1, (1.0 - fx) * fy),
                    (ix + 1, iy + 1, fx * fy),
                ];
                for c in 0..4 {
                    let mut value = 0.0;
                    for (px, py, weight) in samples {
                        if px >= 0 && py >= 0 && px < w as i32 && py < h as i32 {
                            value +=
                                f32::from(pixels[(py as usize * w + px as usize) * 4 + c]) * weight;
                        }
                    }
                    out[(y * w + x) * 4 + c] =
                        (value * alpha * if c == 3 { 1.0 } else { shade }).round() as u8;
                }
            }
        }
    }
}
