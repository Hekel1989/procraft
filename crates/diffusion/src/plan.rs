//! The sizes to generate at: the area of the canvas is brought to a target size (the model's own,
//! half the area's, or the whole area's), capped at the largest picture, then made in one pass at
//! that size, as krita-ai-diffusion does within a model's range. Each pass's sides are multiples of
//! 16 (what the models' latents need); results are scaled to the area after.

/// Each side is a multiple of this.
pub const MULTIPLE: u32 = 16;
/// How much of a selection's size surrounds it on each side, and the least that does, in canvas
/// pixels.
const CONTEXT_SHARE: u32 = 2;
const CONTEXT_MIN: u32 = 64;
/// The shortest and longest side generated at the model's own size.
pub const MIN_SIDE: u32 = 512;
pub const MAX_SIDE: u32 = 2048;

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
    /// A second pass refining the first at a larger size. The planner makes one pass (composing at
    /// the target, as Krita does); the graphs still take a second.
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

/// The context box around a selection's `bounds` (`x, y, width, height`) on a canvas of `canvas`
/// (`width, height`): grown by half the bounds' width and height on each side, at least
/// [`CONTEXT_MIN`] pixels, and kept on the canvas. Gives `x, y, width, height`.
pub fn context_box(bounds: [u32; 4], canvas: (u32, u32)) -> [u32; 4] {
    let [x, y, width, height] = bounds;
    let grow_x = (width / CONTEXT_SHARE).max(CONTEXT_MIN);
    let grow_y = (height / CONTEXT_SHARE).max(CONTEXT_MIN);
    let left = x.saturating_sub(grow_x).min(canvas.0);
    let top = y.saturating_sub(grow_y).min(canvas.1);
    let right = x.saturating_add(width).saturating_add(grow_x).min(canvas.0).max(left);
    let bottom = y.saturating_add(height).saturating_add(grow_y).min(canvas.1).max(top);
    [left, top, right.saturating_sub(left), bottom.saturating_sub(top)]
}

/// The size a model makes pictures at: about `pixels`, each side between `min_side` and
/// `max_side`. A family's own range is wide (Anima composes well up to 2048 a side, so its pictures
/// are one pass at the target); a style's best size narrows it to within 20 % of that side, as
/// Krita does, and a target clearly bigger than that is made in two passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelSize {
    pub pixels: u32,
    pub min_side: u32,
    pub max_side: u32,
    /// A target clearly bigger than the range is made at the model's size first, then refined at
    /// the target.
    pub second_pass: bool,
}

impl ModelSize {
    /// A family's own: about `pixels`, sides from [`MIN_SIDE`] to [`MAX_SIDE`], one pass.
    pub const fn family(pixels: u32) -> ModelSize {
        ModelSize { pixels, min_side: MIN_SIDE, max_side: MAX_SIDE, second_pass: false }
    }

    /// A style's best size, `side` pixels a side: its square's pixels, sides within 20 % of it.
    pub fn best(side: u32) -> ModelSize {
        let side = side.clamp(MULTIPLE * 16, MAX_SIDE);
        let offset = ((f64::from(side) * 0.2 / f64::from(MULTIPLE)).round() as u32) * MULTIPLE;
        ModelSize { pixels: side * side, min_side: side - offset, max_side: side + offset, second_pass: true }
    }
}

/// The model's own plan for an area: about `pixels`, shape kept, sides on the grid and between
/// [`MIN_SIDE`] and [`MAX_SIDE`].
pub fn plan(width: u32, height: u32, pixels: u32) -> Plan {
    let size = native(width, height, ModelSize::family(pixels));
    Plan { first: size, second: None, size, target: size, capped: false }
}

/// The plan for an area of `width × height` canvas pixels and a model that works best at
/// `pixels`: `resolution` chooses the target, `largest` (megapixels) caps it, and `refine` makes
/// one pass at the target from the picture there is.
pub fn plan_for(width: u32, height: u32, pixels: u32, resolution: Resolution, largest: f32, refine: bool) -> Plan {
    plan_at(width, height, ModelSize::family(pixels), resolution, largest, refine)
}

/// As [`plan_for`], for a model that makes pictures at `model`'s size (a style's best size, or its
/// family's). With a best size, a new picture whose target is clearly bigger than the model's range
/// (a side beyond it, and more than about 10 % bigger each way) is made at the model's size and
/// refined at the target, as Krita does; `refine` and selections stay one pass, at the model's size
/// when the target is bigger.
pub fn plan_at(width: u32, height: u32, model: ModelSize, resolution: Resolution, largest: f32, refine: bool) -> Plan {
    let area = Size::new(width.max(1), height.max(1));
    let native = native(area.width, area.height, model);
    let chosen = match resolution {
        Resolution::Draft => native,
        Resolution::Normal => {
            let half = Size::new((area.width / 2).max(1), (area.height / 2).max(1));
            if half.pixels() < native.pixels() { native } else { half }
        }
        Resolution::Quality => area,
    };
    let (target, capped) = capped(chosen, largest);
    if model.second_pass {
        let bigger = target.pixels() > native.pixels();
        if refine || !bigger {
            return Plan { first: native, second: None, size: native, target, capped };
        }
        let scale = (native.pixels() as f64 / target.pixels() as f64).sqrt();
        if scale < 0.9 && target.width.max(target.height) > model.max_side {
            let second = grid(target);
            return Plan { first: native, second: Some(second), size: second, target, capped };
        }
        let size = grid(target);
        return Plan { first: size, second: None, size, target, capped };
    }
    if refine {
        let size = if target.pixels() < native.pixels() { native } else { grid(target) };
        return Plan { first: size, second: None, size, target, capped };
    }
    // one pass at the target: the model composes at the size it is made at
    if target.pixels() > native.pixels() || capped {
        let size = grid(target);
        Plan { first: size, second: None, size, target, capped }
    } else {
        Plan { first: native, second: None, size: native, target, capped }
    }
}

/// The model's own size for an area: about its pixels with the area's shape kept.
fn native(width: u32, height: u32, model: ModelSize) -> Size {
    let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let scale = (f64::from(model.pixels) / (w * h)).sqrt();
    let side = |s: f64| {
        let rounded = ((s / f64::from(MULTIPLE)).round() as u32).max(1) * MULTIPLE;
        rounded.clamp(model.min_side, model.max_side.max(model.min_side))
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
    fn a_context_box_grows_a_selection_in_the_middle() {
        assert_eq!(context_box([400, 300, 200, 100], (1000, 800)), [300, 236, 400, 228]);
    }

    #[test]
    fn a_context_box_stops_at_the_canvas_edge() {
        assert_eq!(context_box([0, 700, 100, 100], (1000, 800)), [0, 636, 164, 164]);
    }

    #[test]
    fn a_tiny_selection_gets_at_least_64_pixels_of_context() {
        assert_eq!(context_box([500, 500, 4, 4], (1000, 1000)), [436, 436, 132, 132]);
        assert_eq!(context_box([0, 0, 1, 1], (100, 100)), [0, 0, 65, 65]);
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
    fn quality_gets_the_area_in_one_pass() {
        let p = at(2800, 2000, Resolution::Quality, 6.0);
        assert_eq!(p.target, Size::new(2800, 2000));
        assert_eq!(p.target.pixels(), 5_600_000);
        assert_eq!(p.passes(), 1);
        assert_eq!(p.first, Size::new(2800, 2000));
        assert_eq!(p.second, None);
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
        assert_eq!((big.target, big.passes(), big.first), (Size::new(2000, 1500), 1, Size::new(2000, 1504)));
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
        assert_eq!((p.passes(), p.first), (1, Size::new(2448, 2448)));
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
    fn a_best_size_makes_its_square_and_refines_a_bigger_target() {
        let best = ModelSize::best(1024);
        assert_eq!((best.min_side, best.max_side), (816, 1232));
        let draft = plan_at(2800, 2000, best, Resolution::Draft, 6.0, false);
        assert_eq!((draft.first, draft.passes()), (Size::new(1216, 864), 1));
        // Quality on a big canvas: the model's size first, then the target
        let quality = plan_at(2800, 2000, best, Resolution::Quality, 6.0, false);
        assert_eq!(quality.first, Size::new(1216, 864));
        assert_eq!(quality.second, Some(Size::new(2800, 2000)));
        assert_eq!(quality.size, Size::new(2800, 2000));
        // a little bigger than the model's size: one pass at the target
        let near = plan_at(1200, 1000, best, Resolution::Quality, 6.0, false);
        assert_eq!((near.first, near.passes()), (Size::new(1200, 1008), 1));
        // refining and selections: one pass at the model's size
        let refine = plan_at(2800, 2000, best, Resolution::Quality, 6.0, true);
        assert_eq!((refine.first, refine.passes(), refine.target), (Size::new(1216, 864), 1, Size::new(2800, 2000)));
        // a small best size keeps sides near it
        let small = plan_at(3000, 1000, ModelSize::best(512), Resolution::Draft, 6.0, false);
        assert_eq!(small.first.width, 608, "within 20 % of 512");
    }

    #[test]
    fn without_a_best_size_nothing_changes() {
        for resolution in [Resolution::Draft, Resolution::Normal, Resolution::Quality] {
            for refine in [false, true] {
                assert_eq!(
                    plan_at(2800, 2000, ModelSize::family(MP), resolution, 4.2, refine),
                    plan_for(2800, 2000, MP, resolution, 4.2, refine)
                );
            }
        }
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
