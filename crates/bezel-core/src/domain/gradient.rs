//! Two-color interpolation with an adjustable midpoint.
use super::frame::Rgba;

/// Color at `position` on the full scale. `transition` positions the 50% blend.
pub fn arc_color(start: Rgba, end: Rgba, position: f32, transition: f32) -> Rgba {
    let x = if position.is_finite() {
        position.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mid = if transition.is_finite() {
        transition.clamp(0.0, 1.0)
    } else {
        0.5
    };
    let t = if x <= 0.0 {
        0.0
    } else if x >= 1.0 {
        1.0
    } else if x <= mid {
        0.5 * x / mid
    } else {
        0.5 + 0.5 * (x - mid) / (1.0 - mid)
    };
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Rgba {
        r: mix(start.r, end.r),
        g: mix(start.g, end.g),
        b: mix(start.b, end.b),
        a: mix(start.a, end.a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_midpoint_and_extreme_transitions() {
        let a = Rgba {
            r: 0,
            g: 0,
            b: 255,
            a: 128,
        };
        let b = Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        for mid in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(arc_color(a, b, 0.0, mid), a);
            assert_eq!(arc_color(a, b, 1.0, mid), b);
        }
        assert_eq!(
            arc_color(a, b, 0.25, 0.25),
            Rgba {
                r: 128,
                g: 0,
                b: 128,
                a: 192
            }
        );
        assert_eq!(arc_color(a, b, f32::NAN, 0.5), a);
        assert_eq!(arc_color(a, b, -1.0, 0.5), a);
        assert_eq!(arc_color(a, b, 2.0, 0.5), b);
    }
}
