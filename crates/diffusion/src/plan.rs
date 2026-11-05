//! The sizes to generate at: the area of the canvas is brought to a target size (the model's own,
//! half the area's, or the whole area's), then made in one pass at about 1 MP or in two passes
//! with the first at about 1 MP and the second at the target. Each pass's sides are multiples of 16
//! (what the models' latents need); results are scaled to the area after the last pass.

/// Each side is a multiple of this.
pub const MULTIPLE: u32 = 16;
/// The shortest and longest side generated at the model's own size.
pub const MIN_SIDE: u32 = 512;
pub const MAX_SIDE: u32 = 2048;

/// A target more than this many times the model's pixels is refined at the target after composing
/// at the model's size. The design's "about 1.5×".
const TWO_PASS_FACTOR: f64 = 1.5;
/// A megapixel here is a million pixels, as people mean by "6 MP", not 2²⁰.
const MEGAPIXEL: f64 = 1_000_000.0;

/// How large a picture to make: the model's own size, half the area each way (never below the
/// model's size), or the area's own size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resolution {
    /// The model's own size (about 1 MP), scaled to the area after.
    Draft,
    /// Half the area's size each way, but never fewer pixels than the model's own size.
    Normal,
    /// The area's own size.
    Quality,
}

/// A width and height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    /// Its pixel count.
    pub fn pixels(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Whether results of this size are scaled up by more than `factor` to fill an area of
    /// `width × height`.
    pub fn stretched(self, width: u32, height: u32, factor: f64) -> bool {
        f64::from(width) / f64::from(self.width.max(1)) > factor
            || f64::from(height) / f64::from(self.height.max(1)) > factor
    }
}

/// How a run makes its picture: the first pass's size, the second's if it has one, and the size
/// results come back at before the final scale to the area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The first pass: the model's own size for a new picture, the target when refining.
    pub first: Size,
    /// The second pass, when the target is worth refining.
    pub second: Option<Size>,
    /// The size results come back at (the last pass's size).
    pub size: Size,
    /// The chosen target, after the largest-picture cap: what the small text shows.
    pub target: Size,
    /// The target was brought down to the largest picture.
    pub capped: bool,
}

impl Plan {
    /// How many passes it makes.
    pub fn passes(self) -> u32 {
        if self.second.is_some() { 2 } else { 1 }
    }

    /// Whether results of this plan are scaled up by more than `factor` to fill an area.
    pub fn stretched(self, width: u32, height: u32, factor: f64) -> bool {
        self.size.stretched(width, height, factor)
    }
}

/// The largest picture in megapixels a GPU's memory can take: 6 at 22 GiB and up, 4 at 14, 3 at
/// 10, else 2 (unknown memory included).
pub fn largest_for_vram(bytes: u64) -> f32 {
    const GIB: u64 = 1 << 30;
    if bytes >= 22 * GIB {
        6.0
    } else if bytes >= 14 * GIB {
        4.0
    } else if bytes >= 10 * GIB {
        3.0
    } else {
        2.0
    }
}

/// The model's own plan for an area: about `pixels`, shape kept, sides on the grid and between
/// [`MIN_SIDE`] and [`MAX_SIDE`].
pub fn plan(width: u32, height: u32, pixels: u32) -> Plan {
    let size = native(width, height, pixels);
    Plan { first: size, second: None, size, target: size, capped: false }
}

/// The plan for an area of `width × height` canvas pixels and a model that works best at
/// `pixels`: `resolution` chooses the target, `largest` (megapixels) caps it, and `refine` makes
/// one pass at the target from the picture there is.
pub fn plan_for(width: u32, height: u32, pixels: u32, resolution: Resolution, largest: f32, refine: bool) -> Plan {
    let area = Size::new(width.max(1), height.max(1));
    let native = native(area.width, area.height, pixels);
    let chosen = match resolution {
        Resolution::Draft => native,
        Resolution::Normal => {
            let half = Size::new((area.width / 2).max(1), (area.height / 2).max(1));
            if half.pixels() < native.pixels() { native } else { half }
        }
        Resolution::Quality => area,
    };
    let (target, capped) = capped(chosen, largest);
    if refine {
        let size = if target.pixels() < native.pixels() { native } else { grid(target) };
        return Plan { first: size, second: None, size, target, capped };
    }
    let target_pixels = target.pixels();
    let native_pixels = native.pixels();
    // A Normal target is a quarter of the area: when it is still above the model's own plan the
    // area is at least four times the model's size, so it is worth refining even under the cutoff.
    let far = target_pixels as f64 > native_pixels as f64 * TWO_PASS_FACTOR;
    if far {
        let second = grid(target);
        Plan { first: native, second: Some(second), size: second, target, capped }
    } else if target_pixels > native_pixels || capped {
        let size = grid(target);
        Plan { first: size, second: None, size, target, capped }
    } else {
        Plan { first: native, second: None, size: native, target, capped }
    }
}

/// The model's own size for an area: about `pixels` with its shape kept.
fn native(width: u32, height: u32, pixels: u32) -> Size {
    let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let scale = (f64::from(pixels) / (w * h)).sqrt();
    let side = |s: f64| {
        let rounded = ((s / f64::from(MULTIPLE)).round() as u32).max(1) * MULTIPLE;
        rounded.clamp(MIN_SIDE, MAX_SIDE)
    };
    Size::new(side(w * scale), side(h * scale))
}

/// `size` brought under `largest` megapixels, shape kept and sides on the grid; whether it was.
fn capped(size: Size, largest: f32) -> (Size, bool) {
    let largest = if largest.is_finite() && largest > 0.0 { f64::from(largest) } else { 2.0 };
    let most = largest * MEGAPIXEL;
    if size.pixels() as f64 <= most {
        return (size, false);
    }
    let scale = (most / size.pixels() as f64).sqrt();
    let side = |s: u32| {
        let scaled = (f64::from(s) * scale).floor() as u32;
        (scaled / MULTIPLE).max(1) * MULTIPLE
    };
    (Size::new(side(size.width), side(size.height)), true)
}

/// `size` with each side rounded to the nearest multiple of 16 (at least one).
fn grid(size: Size) -> Size {
    let side = |s: u32| {
        let rounded = s.saturating_add(MULTIPLE / 2) / MULTIPLE * MULTIPLE;
        rounded.max(MULTIPLE)
    };
    Size::new(side(size.width), side(size.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MP: u32 = 1024 * 1024;

    fn at(width: u32, height: u32, resolution: Resolution, largest: f32) -> Plan {
        plan_for(width, height, MP, resolution, largest, false)
    }

    #[test]
    fn squares_land_on_the_native_size() {
        assert_eq!(plan(4096, 4096, MP).size, Size::new(1024, 1024));
        assert_eq!(plan(300, 300, MP).size, Size::new(1024, 1024));
    }

    #[test]
    fn shapes_are_kept_on_the_grid() {
        let p = plan(2800, 2000, MP);
        assert_eq!((p.size.width % MULTIPLE, p.size.height % MULTIPLE), (0, 0));
        assert_eq!(p.size, Size::new(1216, 864));
        let ratio = f64::from(p.size.width) / f64::from(p.size.height);
        assert!((ratio - 1.4).abs() < 0.02, "{ratio}");
    }

    #[test]
    fn long_thin_areas_are_clamped() {
        assert_eq!(plan(10000, 100, MP).size, Size::new(MAX_SIDE, MIN_SIDE));
    }

    #[test]
    fn quality_gets_the_area_in_two_passes() {
        let p = at(2800, 2000, Resolution::Quality, 6.0);
        assert_eq!(p.target, Size::new(2800, 2000));
        assert_eq!(p.target.pixels(), 5_600_000);
        assert_eq!(p.passes(), 2);
        assert_eq!(p.first, Size::new(1216, 864));
        assert_eq!(p.second, Some(Size::new(2800, 2000)));
        assert_eq!(p.size, Size::new(2800, 2000));
        assert!(!p.capped);
        assert!(!p.stretched(2800, 2000, 1.5));
    }

    #[test]
    fn normal_gets_half_the_area() {
        let p = at(2800, 2000, Resolution::Normal, 6.0);
        assert_eq!(p.target, Size::new(1400, 1000));
        assert_eq!(p.passes(), 1, "near the model's size: one pass");
        assert_eq!(p.first, Size::new(1408, 1008));
        let big = at(4000, 3000, Resolution::Normal, 6.0);
        assert_eq!((big.target, big.passes()), (Size::new(2000, 1500), 2));
        assert!(p.stretched(2800, 2000, 1.5), "results are scaled up to the area");
    }

    #[test]
    fn draft_is_the_native_plan_scaled_up() {
        let p = at(2800, 2000, Resolution::Draft, 6.0);
        assert_eq!(p.target, Size::new(1216, 864));
        assert_eq!(p.size, Size::new(1216, 864));
        assert_eq!(p.first, Size::new(1216, 864));
        assert_eq!(p.second, None);
        assert_eq!(p.passes(), 1);
        assert!(p.stretched(2800, 2000, 1.5));
    }

    #[test]
    fn the_largest_picture_caps_the_target() {
        let p = at(4096, 4096, Resolution::Quality, 6.0);
        assert!(p.capped);
        assert_eq!(p.target, Size::new(2448, 2448));
        assert_eq!(p.target.width % MULTIPLE, 0);
        assert_eq!(p.target.height % MULTIPLE, 0);
        assert!(p.target.pixels() <= 6_000_000, "{}", p.target.pixels());
        assert!(p.size.pixels() <= 6_000_000, "{}", p.size.pixels());
        assert_eq!(p.passes(), 2);
    }

    #[test]
    fn a_native_sized_area_is_one_pass() {
        let p = at(1024, 1024, Resolution::Quality, 6.0);
        assert_eq!(p.target, Size::new(1024, 1024));
        assert_eq!(p.first, Size::new(1024, 1024));
        assert_eq!(p.second, None);
        assert_eq!(p.passes(), 1);
        assert!(!p.capped);
    }

    #[test]
    fn a_small_area_uses_the_native_plan() {
        let native = plan(600, 400, MP).size;
        for resolution in [Resolution::Normal, Resolution::Quality] {
            let p = at(600, 400, resolution, 6.0);
            assert_eq!(p.size, native, "{resolution:?}");
            assert_eq!(p.first, native, "{resolution:?}");
            assert_eq!(p.passes(), 1, "{resolution:?}");
        }
    }

    #[test]
    fn refining_is_one_pass_at_the_target() {
        let p = plan_for(2800, 2000, MP, Resolution::Quality, 6.0, true);
        assert_eq!(p.target, Size::new(2800, 2000));
        assert_eq!(p.first, Size::new(2800, 2000));
        assert_eq!(p.second, None);
        assert_eq!(p.passes(), 1);
        let small = plan_for(600, 400, MP, Resolution::Quality, 6.0, true);
        assert_eq!(small.size, plan(600, 400, MP).size, "never below the model's size");
    }

    #[test]
    fn the_cap_follows_the_gpu_memory() {
        const GIB: u64 = 1 << 30;
        assert_eq!(largest_for_vram(24 * GIB), 6.0);
        assert_eq!(largest_for_vram(16 * GIB), 4.0);
        assert_eq!(largest_for_vram(12 * GIB), 3.0);
        assert_eq!(largest_for_vram(8 * GIB), 2.0);
        assert_eq!(largest_for_vram(0), 2.0);
    }
}
