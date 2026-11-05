//! The size to generate at: an area of the canvas scaled to about the model's own pixel count,
//! keeping its shape, each side a multiple of 16 (what the models' latents need). Results are
//! scaled back to the area's size after.

/// Each side is a multiple of this.
pub const MULTIPLE: u32 = 16;
/// The shortest and longest side generated.
pub const MIN_SIDE: u32 = 512;
pub const MAX_SIDE: u32 = 2048;

/// The size to generate at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub width: u32,
    pub height: u32,
}

/// The size to generate an area of `width × height` canvas pixels at, for a model that works best
/// at `pixels`.
pub fn plan(width: u32, height: u32, pixels: u32) -> Plan {
    let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let scale = (f64::from(pixels) / (w * h)).sqrt();
    let side = |s: f64| {
        let rounded = ((s / f64::from(MULTIPLE)).round() as u32).max(1) * MULTIPLE;
        rounded.clamp(MIN_SIDE, MAX_SIDE)
    };
    Plan { width: side(w * scale), height: side(h * scale) }
}

impl Plan {
    /// Whether results are scaled up by more than `factor` to fill an area of `width × height`.
    pub fn stretched(self, width: u32, height: u32, factor: f64) -> bool {
        f64::from(width) / f64::from(self.width) > factor || f64::from(height) / f64::from(self.height) > factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MP: u32 = 1024 * 1024;

    #[test]
    fn squares_land_on_the_native_size() {
        assert_eq!(plan(4096, 4096, MP), Plan { width: 1024, height: 1024 });
        assert_eq!(plan(300, 300, MP), Plan { width: 1024, height: 1024 });
    }

    #[test]
    fn shapes_are_kept_on_the_grid() {
        let p = plan(2800, 2000, MP);
        assert_eq!((p.width % MULTIPLE, p.height % MULTIPLE), (0, 0));
        assert_eq!(p, Plan { width: 1216, height: 864 });
        let ratio = f64::from(p.width) / f64::from(p.height);
        assert!((ratio - 1.4).abs() < 0.02, "{ratio}");
    }

    #[test]
    fn long_thin_areas_are_clamped() {
        assert_eq!(plan(10000, 100, MP), Plan { width: MAX_SIDE, height: MIN_SIDE });
    }

    #[test]
    fn stretching_is_measured() {
        let p = Plan { width: 1024, height: 1024 };
        assert!(p.stretched(4096, 4096, 2.0));
        assert!(!p.stretched(1500, 1500, 2.0));
    }
}
