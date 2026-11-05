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

/// What a layer's lines are like: clean line art or rough scribbles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlKind {
    LineArt,
    Scribble,
}

/// How closely a control layer is followed: from a loose shaping of the picture's start to lines
/// held to the last step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ControlStep {
    Loose,
    #[default]
    Balanced,
    Close,
    Strict,
}

/// A layer's pixels to guide generation with: its picture at the request's area (straight RGBA,
/// white lines on black), its kind, how strongly to follow it (0..1) and how far through sampling
/// that applies (0..1).
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub kind: ControlKind,
    pub strength: f32,
    pub end_percent: f32,
    pub picture: Picture,
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
    /// The layers' lines to follow, each with its strength (none: no control layers).
    pub controls: Vec<Control>,
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
