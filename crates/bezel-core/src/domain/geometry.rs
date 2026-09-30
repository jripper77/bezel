//! Pixel geometry shared by devices, themes and frames.

/// A width x height pair in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Size {
    /// Builds a size.
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// The same size with width and height swapped.
    pub const fn transposed(self) -> Self {
        Self::new(self.height, self.width)
    }

    /// Number of pixels.
    pub const fn area(self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// The size of a canvas in `orientation` for a panel whose portrait size is `self`.
    ///
    /// `self` is expected in portrait form (`width <= height`); landscape
    /// orientations swap the axes.
    pub const fn in_orientation(self, orientation: Orientation) -> Self {
        if orientation.is_landscape() {
            self.transposed()
        } else {
            self
        }
    }

    /// The portrait form of this size (`width <= height`).
    pub const fn portrait(self) -> Self {
        if self.width <= self.height {
            self
        } else {
            self.transposed()
        }
    }
}

/// How the user looks at the screen. Values follow the numbering used by the
/// reference implementations (0 = portrait, 1 = reverse portrait,
/// 2 = landscape, 3 = reverse landscape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Orientation {
    /// Upright, taller than wide.
    #[default]
    Portrait,
    /// Portrait rotated 180 degrees.
    ReversePortrait,
    /// Wider than tall.
    Landscape,
    /// Landscape rotated 180 degrees.
    ReverseLandscape,
}

impl Orientation {
    /// All orientations in protocol order.
    pub const ALL: [Orientation; 4] = [
        Orientation::Portrait,
        Orientation::ReversePortrait,
        Orientation::Landscape,
        Orientation::ReverseLandscape,
    ];

    /// True for the two landscape orientations.
    pub const fn is_landscape(self) -> bool {
        matches!(self, Orientation::Landscape | Orientation::ReverseLandscape)
    }

    /// The protocol index (0..=3) used by the reference implementations.
    pub const fn index(self) -> u8 {
        match self {
            Orientation::Portrait => 0,
            Orientation::ReversePortrait => 1,
            Orientation::Landscape => 2,
            Orientation::ReverseLandscape => 3,
        }
    }

    /// Clockwise rotation, in quarter turns (0..=3), that maps a canvas drawn in
    /// `self` onto a panel whose native orientation is `native`.
    pub const fn quarter_turns_to(self, native: Orientation) -> u8 {
        (native.clockwise_quarters() + 4 - self.clockwise_quarters()) % 4
    }

    /// Clockwise quarter turns of this orientation relative to portrait.
    const fn clockwise_quarters(self) -> u8 {
        match self {
            Orientation::Portrait => 0,
            Orientation::Landscape => 1,
            Orientation::ReversePortrait => 2,
            Orientation::ReverseLandscape => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_swaps_axes() {
        let panel = Size::new(480, 1920);
        assert_eq!(
            panel.in_orientation(Orientation::Landscape),
            Size::new(1920, 480)
        );
        assert_eq!(panel.in_orientation(Orientation::ReversePortrait), panel);
        assert_eq!(Size::new(800, 480).portrait(), Size::new(480, 800));
        assert_eq!(panel.area(), 921_600);
    }

    #[test]
    fn quarter_turns_between_orientations() {
        use Orientation::*;
        assert_eq!(Portrait.quarter_turns_to(Portrait), 0);
        assert_eq!(Portrait.quarter_turns_to(ReversePortrait), 2);
        assert_eq!(ReversePortrait.quarter_turns_to(Portrait), 2);
        assert_eq!(Landscape.quarter_turns_to(Portrait), 3);
        assert_eq!(Portrait.quarter_turns_to(Landscape), 1);
        for o in Orientation::ALL {
            assert_eq!(o.quarter_turns_to(o), 0);
            assert_eq!(Orientation::ALL[o.index() as usize], o);
        }
    }
}
