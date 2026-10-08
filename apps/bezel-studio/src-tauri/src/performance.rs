//! Transient measurements: never serialized into a theme.
use bezel_core::domain::frame::{Frame, Rect, Rgba};
use bezel_core::ports::TransferStats;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Corner {
    TopLeft,
    TopRight,
    #[default]
    BottomLeft,
    BottomRight,
}
impl Corner {
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Self::TopLeft,
            1 => Self::TopRight,
            3 => Self::BottomRight,
            _ => Self::BottomLeft,
        }
    }
    fn bounds(self, size: bezel_core::domain::geometry::Size, height: u32) -> Rect {
        let width = size.width.min(480);
        let height = height.min(size.height);
        let right = matches!(self, Self::TopRight | Self::BottomRight);
        let bottom = matches!(self, Self::BottomLeft | Self::BottomRight);
        Rect::new(
            if right { size.width - width } else { 0 },
            if bottom { size.height - height } else { 0 },
            width,
            height,
        )
    }
}

#[derive(Default)]
pub struct Performance {
    last: Option<Instant>,
    intervals: VecDeque<f64>,
    overlay_at: Option<Instant>,
    lines: Vec<String>,
}

pub struct Timings {
    pub render_ms: f64,
    pub render_reused: bool,
    pub render_stats: bezel_core::ports::RenderStats,
    pub present_ms: f64,
    pub handoff_ms: f64,
    pub sensors_ms: f64,
    pub refresh_ms: f64,
    pub animating: bool,
}

impl Performance {
    pub fn completed(
        &mut self,
        timings: Timings,
        transfer: Option<TransferStats>,
        screen: &str,
        success: bool,
        now: Instant,
    ) {
        let Timings {
            render_ms,
            render_reused,
            render_stats,
            present_ms,
            handoff_ms,
            sensors_ms,
            refresh_ms,
            animating,
        } = timings;
        let gap_ms = self
            .last
            .map(|last| now.saturating_duration_since(last).as_secs_f64() * 1000.0);
        if gap_ms.is_some_and(|gap| gap > 300.0) {
            self.intervals.clear();
        }
        if success && transfer.is_none_or(|t| t.bytes > 0) {
            if let Some(gap) = gap_ms {
                if gap > 300.0 {
                    self.intervals.clear();
                } else {
                    self.intervals.push_back(gap);
                }
                if self.intervals.len() > 20 {
                    self.intervals.pop_front();
                }
            }
            self.last = Some(now);
        }
        let fps = if self.intervals.is_empty() {
            None
        } else {
            Some(1000.0 * self.intervals.len() as f64 / self.intervals.iter().sum::<f64>())
        };
        tracing::debug!(target: "bezel_studio::performance", screen, success, animating, render_reused, render_ms, present_ms, handoff_ms,
            sensors_ms, refresh_ms, render_stats = ?render_stats, fps = ?fps, gap_ms = ?gap_ms, transfer = ?transfer,
            "frame delivery (host completion; not panel refresh)");
        if self
            .overlay_at
            .is_none_or(|last| now.saturating_duration_since(last).as_millis() >= 250)
        {
            self.overlay_at = Some(now);
            let fps = fps.map_or_else(|| "--".into(), |v| format!("{v:.1}"));
            self.lines = vec![
                format!("SEND FPS {fps}  GAP {:.0} MS", gap_ms.unwrap_or(0.0)),
                format!("RENDER {render_ms:.1}  PRESENT {present_ms:.1} MS"),
            ];
            if let Some(t) = transfer {
                self.lines.push(format!(
                    "C {:.1} D {:.1} W {:.1} A {:.1} MS",
                    t.conversion_ms, t.diff_ms, t.write_ms, t.reply_ms
                ));
                self.lines.push(format!(
                    "{:.1} KB {} S {sensors_ms:.1}/{refresh_ms:.0} MS",
                    t.bytes as f64 / 1024.0,
                    t.kind.to_uppercase()
                ));
            } else {
                self.lines
                    .push(format!("TRANSFER N/A SENSOR {sensors_ms:.1} MS"));
            }
        }
    }

    pub fn paint(&self, frame: &mut Frame, corner: Corner) {
        let bounds = corner.bounds(frame.size(), self.lines.len().max(1) as u32 * 18 + 8);
        let (x, y) = (bounds.x, bounds.y);
        frame.fill_rect(bounds, Rgba::opaque(5, 9, 15));
        let initial = vec!["DEBUG: WAITING FOR FIRST FRAME".to_owned()];
        let lines = if self.lines.is_empty() {
            &initial
        } else {
            &self.lines
        };
        for (row, text) in lines.iter().enumerate() {
            for (column, ch) in text
                .chars()
                .take((bounds.width / 12).saturating_sub(1) as usize)
                .enumerate()
            {
                let glyph = glyph(ch);
                for (gy, bits) in glyph.iter().enumerate() {
                    for gx in 0..5 {
                        if bits & (1 << (4 - gx)) != 0 {
                            frame.fill_rect(
                                Rect::new(
                                    x + 4 + column as u32 * 12 + gx * 2,
                                    y + 4 + row as u32 * 18 + gy as u32 * 2,
                                    2,
                                    2,
                                )
                                .clip(frame.size()),
                                Rgba::opaque(130, 255, 200),
                            );
                        }
                    }
                }
            }
        }
    }
}

fn glyph(ch: char) -> [u8; 7] {
    match ch {
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '.' => [0, 0, 0, 0, 0, 6, 6],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        ':' => [0, 6, 6, 0, 6, 6, 0],
        _ => [0; 7],
    }
}

/// Refresh work and its wait for the editor, distinct from serial I/O.
pub fn refresh_tick(elapsed_ms: f64, lock_ms: f64, age_ms: f64, frames: usize) {
    tracing::debug!(target: "bezel_studio::performance", elapsed_ms, lock_ms, age_ms, frames,
        "refresh tick (editor wait and frame preparation; not serial completion)");
}

/// Preview preparation including initial editor-lock wait and output copy.
pub fn preview_render(elapsed_ms: f64, lock_ms: f64, render_reused: bool) {
    tracing::debug!(target: "bezel_studio::performance", elapsed_ms, lock_ms, render_reused,
        "preview render (includes editor wait and output copy)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::geometry::Size;
    #[test]
    fn overlay_anchors_to_all_four_corners_and_clips_to_canvas() {
        let p = Performance::default();
        for corner in [
            Corner::TopLeft,
            Corner::TopRight,
            Corner::BottomLeft,
            Corner::BottomRight,
        ] {
            let mut frame = Frame::filled(Size::new(1920, 480), Rgba::WHITE);
            p.paint(&mut frame, corner);
            let bounds = corner.bounds(frame.size(), 26);
            assert_eq!(
                frame.pixel(bounds.x, bounds.y),
                Some(Rgba::opaque(5, 9, 15))
            );
            assert_eq!(frame.pixel(960, 240), Some(Rgba::WHITE));
        }
        for corner in [
            Corner::TopLeft,
            Corner::TopRight,
            Corner::BottomLeft,
            Corner::BottomRight,
        ] {
            let bounds = corner.bounds(Size::new(40, 20), 80);
            assert_eq!(bounds, Rect::new(0, 0, 40, 20));
        }
    }
    #[test]
    fn overlay_stays_inside_small_frame() {
        let mut frame = Frame::filled(Size::new(40, 20), Rgba::WHITE);
        Performance::default().paint(&mut frame, Corner::BottomLeft);
        assert_ne!(frame.pixel(0, 0), Some(Rgba::WHITE));
    }
    #[test]
    fn idle_gap_resets_animation_fps() {
        let mut p = Performance::default();
        let now = Instant::now();
        p.completed(
            Timings {
                render_reused: false,
                render_stats: bezel_core::ports::RenderStats::default(),
                render_ms: 1.,
                present_ms: 2.,
                handoff_ms: 0.,
                sensors_ms: 3.,
                refresh_ms: 1000.,
                animating: true,
            },
            None,
            "test",
            true,
            now,
        );
        p.completed(
            Timings {
                render_reused: false,
                render_stats: bezel_core::ports::RenderStats::default(),
                render_ms: 1.,
                present_ms: 2.,
                handoff_ms: 0.,
                sensors_ms: 3.,
                refresh_ms: 1000.,
                animating: true,
            },
            None,
            "test",
            true,
            now + std::time::Duration::from_millis(50),
        );
        assert_eq!(p.intervals.len(), 1);
        p.completed(
            Timings {
                render_reused: false,
                render_stats: bezel_core::ports::RenderStats::default(),
                render_ms: 1.,
                present_ms: 2.,
                handoff_ms: 0.,
                sensors_ms: 3.,
                refresh_ms: 1000.,
                animating: true,
            },
            None,
            "test",
            true,
            now + std::time::Duration::from_secs(1),
        );
        assert!(p.intervals.is_empty());
    }
}
