//! A request: everything one run needs, as plain data.

use crate::plan::Resolution;
use crate::{Picture, Style};

/// How a selection is generated into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SelectionMode {
    /// Paint the area to fit its surroundings, guided by the prompt.
    Fill,
    /// Start from a neutral fill, so the prompt shapes a new thing in the area.
    Add,
    /// Take what is there away: pre-fill from the surroundings and keep objects out.
    Remove,
    /// Fill transparent edges in from the picture's borders.
    Expand,
}

/// A selection to generate into: its mask cropped to the context box (one byte per pixel, white
/// selected) and the mode.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub mask: Vec<u8>,
    pub mode: SelectionMode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// What to make, as the person typed it (the style wraps it).
    pub prompt: String,
    pub style: Style,
    /// How much to change: 1 makes a new picture, below it refines [`picture`](Self::picture).
    pub strength: f32,
    /// The first picture's seed; each next one adds one.
    pub seed: u64,
    /// How many pictures.
    pub count: u32,
    /// The picture there is (straight RGBA, sRGB), at [`size`](Self::size): what refining starts
    /// from, or the context box's crop for a selection.
    pub picture: Option<Picture>,
    /// The selection to generate into, if any.
    pub selection: Option<Selection>,
    /// The area's size in canvas pixels: results are scaled to this size at the end.
    pub size: (u32, u32),
    /// How large to make the picture before scaling it to [`size`](Self::size).
    pub resolution: Resolution,
    /// The largest picture in megapixels; none: from the server's GPU memory.
    pub largest: Option<f32>,
    /// NSFW is allowed: the style leaves its `safe` rating tag out.
    pub nsfw: bool,
}

impl Request {
    /// A new picture rather than a refined one.
    pub fn is_new(&self) -> bool {
        self.strength >= 1.0 || self.picture.is_none()
    }
}
