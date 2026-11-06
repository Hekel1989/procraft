# The results tray: brainstorm

*Started 2026-11-06. A rework of the results bar (`03-design.md` §A4), which works but is awkward to
use. Sketches: `mockups/results-tray.html`. Names: `names.md`. Brainstorm first, then a design in
`03-design.md`, then code.*

## What's wrong today

1. **It covers the picture.** The bar floats over the bottom of the canvas, so a result being
   looked at is partly hidden.
2. **It can't be tapped away.** Only ✕ gets rid of it. Tapping outside, the instinct in mobile
   apps, does nothing to it.
3. **✕ throws everything away.** ✕ discards the results for good (it deletes them from the
   artwork's folder), so there is no bringing them back. Generating again starts a fresh batch.
4. **Tapping outside paints.** The Generate popover swallows a tap on the canvas (it only closes),
   but the bar doesn't: the tap paints a stroke.

All four come from one choice: the bar is something you *close*. The question is whether it should
be something you *put away* instead.

## Directions considered

- **A · A tray you tuck away** *(chosen)*. A tap on the canvas, ⌄ or Esc tucks the tray into a
  small tab; the tab brings it back. Nothing is lost by tucking. Throwing results away becomes
  deliberate. Fixes all four with one idea, keeps the tray where the thumb already is, and fits
  ProCraft's popovers (a tap outside closes, nothing is painted).
- **B · Results inside the Generate popover** (Krita's way: thumbnails in the same panel as the
  prompt). Nothing lost, tap-out closes. But on the tablet the popover covers about a third of
  the screen while comparing results, and it gets taller.
- **C · A review mode** (like Transform: the canvas fits above the bar, painting off, Done to
  leave). The most "app-like", but modal, the most new code, and in the way of a quick look.

## Direction A, as sketched

- **Up** after a run, or when the tab is tapped. While it's up, a tap on the canvas only tucks it:
  nothing is painted.
- **Tucked** with ⌄, a tap on the canvas, or Esc on desktop. The preview stays on the canvas,
  uncovered; the first stroke hides it, as today.
- **The tray tab**, bottom centre, shows while the artwork has results: the shown result's
  thumbnail and "Results · 4", or a ring and "Generating · 63%" while a run is going. When the first
  picture of a run is ready, the tray comes up by itself, as today.
- **Nothing is lost** by tucking. Results go only through a thumbnail's Delete (hold, as today) or
  "Clear all results…" in ···, which asks first.
- **Quieter**: the tray holds the thumbnails, Again as an icon (↻), Apply (the one blue text
  button), ··· and ⌄. The seed tag moves into ··· as "Use the seed 18234" (it puts the seed in the
  popover, locked, as tapping the tag did).
- **Hides with other tools' bars** (Adjustments, Transform, the Selection toolbar) as today, and
  comes back after as it was, tray or tab.

## Decided (2026-11-06)

| Question | Answer |
|---|---|
| Direction | **A**, a tray that tucks away ("more integrated into ProCraft's philosophy") |
| Name | **Results tray** (briefly "results drawer"); its tab reads "Results · 4" |
| Where the tab sits | **Bottom centre**, where the tray was |
| The seed | **Into ···** ("Use the seed"), out of the tray |
| Again | **An icon** (a round arrow), not a text button |

## Still open

- Whether the Generate popover also lists the results (B's idea). Suggested: not now, since the
  tab already brings them back.
- The Again icon itself: drawn from ProCraft's capsules like the AI icon, in `draw_icons.py`.
- The tab's exact size and look on desktop, and whether a tap on the canvas that tucks the tray
  should also hide the preview (suggested: no, so the whole picture can be seen).
