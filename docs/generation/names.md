# Names

What we call the parts of image generation, so the app, the docs, the code and our conversations
use the same words. **Bold** is the name; "quoted" is the exact text shown in the app. Add a line
when something gets a name, and change it here first when a name changes.

## On the canvas screen

| Name | What it is | Shown as |
|---|---|---|
| **AI button** | The top-bar button that opens the Generate popover; a progress ring goes round it while a run is going. Desktop: G. | the "AI" icon |
| **Generate popover** | The popover under the AI button: style, prompt, control layers, strength, resolution, seed, count, Generate. | "Generate" |
| **Server view** | The popover's own settings page: the server address, what it has, NSFW, Largest picture. Opened from the server button in the popover's title. | the server's address |
| **Results tray** | The bar at the bottom with this artwork's results: thumbnails, Again, Apply, ··· and ⌄. Tucks away into its tab, never closes. *(Named 2026-11-06; was "the results bar". Being redesigned: `mockups/results-tray.html`.)* | — |
| **Tray tab** | The small tab the results tray tucks into, at the bottom centre: the shown result's thumbnail and the count, or the progress while generating. | "Results · 4", "Generating · 63%" |
| **Preview** | A result shown over the canvas (not yet a layer), from a tap on its thumbnail. | — |

## In the Generate popover

| Name | What it is | Shown as |
|---|---|---|
| **Style** | A card at the top: a named set of model, prompt words and settings. | e.g. "Anima" |
| **Prompt** | The text field; tags or plain words. | "Prompt" |
| **Suggestions** (the **suggestion strip**) | The chips under the prompt: tags for the word being typed, or recent prompts when it's empty. | — |
| **Recent prompts** | The last 20 prompts, offered when the prompt is empty. | — |
| **Control part** | The popover's Control section: a row per control layer, then the add buttons. | "Control" |
| **Control layer** | A layer the picture follows rather than shows. Two kinds. | "Line art", "Scribble" |
| **Trace button** | On each control row: fills that layer with lines traced from what's visible. | a line with a sparkle |
| **Steps** | How closely a control layer is followed. | "Loose", "Balanced", "Close", "Strict" |
| **Strength** | How much a run changes: 100 % makes a new picture; less refines what's there. | "Strength", "New image" |
| **Generation resolution** | How big the picture is made. | "Draft", "Normal", "Quality" |
| **Mode** (selection mode) | What a run does inside a selection. | "Fill", "Add", "Remove", "Expand" |
| **Seed** | The number a run starts from; the same seed and settings give the same picture. | "Seed" |

## In the results tray

| Name | What it is | Shown as |
|---|---|---|
| **Thumbnail** | One result. Tap: preview it (again: hide it). Hold: its Delete. | — |
| **Again** | Runs the same settings with a new seed (the fixed one if locked). | ↻ |
| **Apply** | Adds the shown result as a new layer at the top. | "Apply" |
| **More menu** | ···: put the shown result's seed in the popover (locked), clear all results. | "Use the seed 18234", "Clear all results…" |
| **Tuck** | Puts the tray away into its tab (also a tap on the canvas, or Esc). | ⌄ |

## Settings and set-up

| Name | What it is | Shown as |
|---|---|---|
| **NSFW switch** | Whether NSFW tags are suggested. Never changes what is generated. | "Show NSFW tags" |
| **Largest picture** | The most megapixels a picture is made at, from the GPU's memory unless changed. | "Largest picture" |
| **Companion** | The CLI that installs and runs a ComfyUI for the app (M5). | `procraft-companion` |
| **Dev ComfyUI** | The ComfyUI on this machine used for development (port 8189). Not a user-facing name. | — |
