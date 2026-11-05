# ProCraft Diffusion: design

Status: **final (2026-11-05).** Implements `02-proposal.md` (final). Part A is what the user sees,
part B how it's built, part C the companion, part D testing and the work breakdown. Mockups of the
screens are in `mockups/phase1.html`. The decisions taken in review are in §D3.

Measured on the dev ComfyUI (24 GB GPU, 2026-11-05), used for the numbers below:

| Run | Time |
|---|---|
| Anima base, 1024², 30 steps, euler ancestral, CFG 4 (model loaded) | **≈ 9 s** |
| Anima turbo, 1024², 8 steps, euler, CFG 1 (model loaded) | **≈ 1.5 s** |
| Switching between the two models | **+ ≈ 10 s** once |
| First run after ComfyUI starts (loading everything) | ≈ 21 s |

The plain graph (UNETLoader → CLIPLoader type `omnigen2` with `qwen_3_06b_base` → KSampler →
VAEDecode with `qwen_image_vae`) gives clean pictures. No rating tag is added: what is generated
is up to the person (§B9).

---

# Part A — Experience

## A1. Principles

1. **One tool, ProCraft's vocabulary.** A top-bar button, a frosted popover hanging from it, a
   floating bar at the bottom while results are around. Nothing docked, nothing permanent.
2. **The canvas stays visible.** Results show on the canvas itself; the popover covers only its
   top-left corner.
3. **Nothing is committed until Apply.** Previews are drawn over the canvas, not written into it;
   Apply is a single undo step.
4. **Little typing.** Autocomplete, recent prompts, styles that carry the boilerplate.
5. **Fails loudly and specifically.** Every problem names what's wrong and what to do.

## A2. Where it lives

Top bar, left group, after Transform:

```
Gallery  [Actions] [Adjustments] [Selection] [Transform] [Generate]        [Paint] [Smudge] [Erase] [Layers] (●)
```

- Same size, glyph colour and blue "chosen" disc as its neighbours (`Pc.BarButton*`, `actionCentre(4)`).
- **Lit** (blue disc) while its popover is open.
- **While a run is going**, a thin accent ring around the button fills with the progress, so the user
  can paint with the popover closed and still see it working. (A new element; it uses the button's
  own disc, 2 dp ring, `Pc.Accent`.)
- Desktop: the same; **G** opens it.

**The icon: "AI"** (decided). Drawn, not typeset: the letters are built from the style guide's
capsules in `tools/icons/draw_icons.py` — A's legs and the I 3 units thick, A's bar 2.4, rounded ends,
inside the live area 2–22 — solid, one colour, tinted like the others. Its master SVG goes in
`assets/icons/procraft/` with a line in `ATTRIBUTION.md`.

The style guide says icons carry no text. Drawing the letters from capsules keeps the weight and
corners of the other icons, but an upstream review may still ask for a picture instead; the two
earlier ideas stay in the mockups as fallbacks.

## A3. The Generate popover

Hangs from the button with ProCraft's arrow, like Actions' panel; frosted (`rememberFrost`), corner
35 dp, width **400 dp** (tablet and desktop alike). Top to bottom:

1. **Title row** — "Generate", then the connection state on the right: a small dot (green ready,
   amber working/connecting, red problem) and the server's short name. Tapping the dot opens the
   settings page (§A7).
2. **Style strip** — horizontally scrolling cards, like the brush library's sets: name over a small
   sample picture. Phase 1: **Anima** (quality) and **Anima Turbo** (fast), plus one card per other
   Anima checkpoint found on the server (fine-tunes), named from its file. The chosen card is lit.
3. **Prompt** — a multi-line field, 3 lines tall, growing to 6. Placeholder: "Describe it, or type
   tags…".
   - **Suggestions** appear in a strip just under the field as you type a tag: chips with the tag,
     a coloured dot for its kind (general, artist, copyright, character, meta) and a short count
     ("12k"). Tap to complete; the completed tag gets ", " after it.
   - With the field empty and focused, the strip shows **recent prompts** instead (the last 20,
     newest first). Tap to fill the field.
   - *As built:* the strip shows as many chips as fit (at most four recent prompts on desktop);
     click or Tab/Enter completes the first. The core reports UTF-16 ranges so Kotlin indices work
     unchanged, and desktop converts them to its byte offsets.
4. **Control layers** (only when there are any) — one row each: the layer's thumbnail and name,
   its kind ("Line art" / "Scribble"), a strength slider (0–100 %), and a remove button.
5. **Selection** (only when there's a selection) — a row "In the selection" with a mode menu:
   **Fill** · **Add** · **Remove** · **Expand** (§A5).
6. **Strength** — ProCraft's labelled slider: "Strength", value on the right. At 100 % the value reads
   "New image"; below it the percentage and "Refine". Defaults: 100 % without a selection, 100 % in
   Fill/Add/Remove, 100 % for Expand.
7. **Seed and count**, one row:
   - Seed: "Random" by default; tapping it shows the number field with a dice (new random) and a lock
     (keep this seed). A result's seed can be taken from the results bar.
   - Count: a 1 · 2 · 3 · 4 segment.
8. **Generate** — a full-width accent button: "Generate" (or "Generate 4"). Disabled, with the reason
   under it, when nothing is ready (§A8).

On **Generate** (§D3 4): the run starts, the soft keyboard goes down, and **the popover
stays open** so the prompt or strength can be tweaked straight after seeing the first results. The
results bar appears at the bottom with the progress, and previews show on the canvas; the popover
(400 dp of the 1120 dp width, top left) leaves most of it in view. Tapping the canvas outside it
closes it, as with every ProCraft popover; reopening shows the same settings. **The prompt and settings belong to the artwork**
while it's open (switching artworks starts from the last-used settings).

**Keyboard on the tablet.** The popover is anchored at the top; with the soft keyboard up (about
40 % of the 800 dp height) the prompt, the suggestions and the Generate button stay above it. If the
popover is taller than the room left, it scrolls with the prompt pinned.

## A4. Results bar

A floating bar at the bottom, the adjustment panels' look (frosted, rounded), full width minus
ProCraft's margins:

```
 [ ▣ ▣ ▣ ▣ │ ◌ 63% ]                     [ seed 18234 ⧉ ]   [ Again ]  [ Apply ]   ✕
```

- **Thumbnails** of this artwork's results, newest on the right; older runs scroll left. A running
  job shows as a thumbnail with a live progress ring (and an intermediate picture if the server
  sends previews, §B6).
- **Tap a thumbnail** to show it on the canvas (a preview drawn over the canvas, masked to the
  selection when there was one). **Tap it again** to hide it and compare with the original.
- **Press and hold a thumbnail** (desktop: hold or right-click) for a small red **Delete** above
  it: that result goes from the bar and from the artwork's folder, for the pictures nobody wants.
  Tapping elsewhere dismisses it. *(Added 2026-11-05.)*
- **More results than fit** scroll: on Android by swiping the thumbnails, with a fade and ‹ when
  older ones are off to the left; on desktop with ‹ and › or the mouse wheel.
- **Apply** adds the shown result as a **new layer at the top of the stack**, named
  "Generated · <first words of the prompt>", masked to the selection (with its feather) if there
  was one. One undo step ("Generate"). The bar stays, so another result can be applied too.
  *(M2: at the top rather than above the current layer, because the preview shows over every
  layer (§B8); the layer lands exactly where the preview showed it.)*
- **Again** runs the same settings with a new random seed (the fixed seed if locked).
- **Seed** shows the shown result's seed; tapping copies it into the popover's seed (locked).
- **✕** discards the results (asks first if none was applied) and closes the bar.
- **Cancel**: while a run is going, ✕ on the running thumbnail cancels it.
- Starting a stroke hides the preview (the bar stays); the bar hides with other tools' bars
  (Adjustments, Transform) and comes back after.
- **Results stay with the artwork**: leaving it (back to the Gallery) keeps the bar's results, and
  opening it again brings them back, even after the app restarts. They are saved in the artwork's
  private `generation/` folder (§B10), up to 24 per artwork (the oldest is dropped and deleted);
  applied results stay in the bar too. ✕ discards them for good.

## A5. Selections

- With a selection, the run uses **the selection's bounds plus surrounding context** (§B7) and the
  result is blended back through the selection's mask and feather.
- **Modes**:

  | Mode | When to use | What it does | Prompt |
  |---|---|---|---|
  | **Fill** | default | Paints the area to fit its surroundings, guided by the prompt | optional |
  | **Add** | a new thing in the area | Starts from a neutral fill, so the prompt shapes it | needed |
  | **Remove** | make something go away | Pre-fills from the surroundings; adds "object, person" to the negative | ignored |
  | **Expand** | after Crop & Resize made the canvas bigger | Fills transparent edges from the picture's borders | optional |

  Automatic choice: **Expand** when the selection is mostly over transparent pixels at the canvas
  edge, otherwise **Fill**. The user can change it.
- **Selection toolbar**: a **Generate** button at the end (before Clear), enabled with a selection;
  it opens the Generate popover with the selection row showing.

## A6. Control layers

- **Making one**: the layer's options menu gets a row **"Control layer"** (beside Reference and
  Drawing Assist) opening a small menu: **None** · **Line art** · **Scribble**.
  - Line art: clean lines, followed closely. Scribble: rough strokes, followed loosely.
- **In the Layers panel** the layer gets a small badge left of its blend letter ("N"): a solid glyph
  for line art, a looser one for scribble. Otherwise it's a normal layer: it's painted, hidden,
  moved as usual.
- **In the popover** each control layer has its row with strength (default 70 % line art, 50 %
  scribble) and a remove button.
- **What is sent**: the control layer's own pixels (not the picture). Control layers are **left out
  of the picture** that is sent for refine and selections, so their lines don't get painted in as
  colour. Hidden control layers are ignored.
- **Saved with the artwork** (a layer property; §B10).
- Named **"Control layer"**, the term ComfyUI users know (§D3); it also stays clear of ProCraft's
  own "Guide" and "Reference".

## A7. Settings: the Generate popover's server view

*(M2: the settings live in the Generate popover, as its server view, rather than as a Prefs
sub-page: they need text that changes (versions, what's missing), which the Actions panel's rows
don't carry, and everything generation-related stays in one place.)* Reached from the connection dot
in the popover's title, from "Set up" when no server is set, and from a row **Actions › Prefs ›
Image generation** that opens the popover on it. Its content:

- **Server** — address field (`http://<LAN address>:8189`; `http://` added if left out; port 8188
  assumed if none), and **Connect**.
- **Status** — once connected, read-only lines with ✓ / ✕:
  - ComfyUI version
  - Add-ons: tooling nodes, inpaint nodes (controlnet_aux shown, not needed in Phase 1)
  - Models: Anima checkpoint(s), text encoder, VAE, inpaint control, line-art control
  - What works: Generate · Refine · Selections · Control layers — each ✓ or "needs <file> in
    <folder>".
- **Show NSFW tags** — switch, off by default. It only chooses whether NSFW tags are suggested
  while typing; it never changes what is generated (decided 2026-11-05).
- **About** — "Pictures and prompts are sent to this server and nowhere else." and the model's
  licence line ("Anima: CircleStone Labs Non-Commercial License. Built on NVIDIA Cosmos.") once an
  Anima model is found.

The settings are app-wide (not per artwork).

## A8. States and errors

| State | Where it shows | Says |
|---|---|---|
| No server set | Popover body replaced by a card | "Generate needs ComfyUI running on a PC. Set the server in Prefs." [Open Prefs] |
| Connecting | Title dot amber | — |
| Can't reach the server | Dot red; Generate disabled | "Can't reach <LAN address>:8189. Is ComfyUI running?" [Retry] |
| Missing add-on / model | Dot red or amber; the affected control disabled | "Selections need anima-lllite-inpainting in models/controlnet." |
| Connection lost during a run | Running thumbnail turns red | "Lost the server during the run." [Again] |
| Out of GPU memory | Thumbnail red | "The server ran out of GPU memory. Try a smaller selection or count." |
| Other server error | Thumbnail red, tap for detail | ComfyUI's message, first line |
| Cancelled | Thumbnail removed | — |

Reconnects automatically every 5 s while the popover or the bar is open and the server is
unreachable.

## A9. Desktop

The same screens at the same sizes (ProCraft's desktop matches Android point for point), plus:
- **Enter** in the prompt runs, **Shift+Enter** is a new line; **Tab** / **↓↑** pick a suggestion.
- Mouse wheel scrolls the style strip and the thumbnails.
- **G** opens Generate.

---

# Part B — Architecture

## B1. Pieces

```
android (Compose) ─┐                                   ┌── ComfyUI (dev: :8189)
desktop (egui) ────┴── procraft-core ── procraft-diffusion ──HTTP + WebSocket──┤   tooling + inpaint
                      (generation.rs:         (no UI, no GPU)                  │   (+ controlnet_aux)
                       session glue,                                           └── procraft-companion
                       UniFFI)                                                     (CLI; shares
                                                                                    requirements)
```

| Crate | New / changed | Holds |
|---|---|---|
| `crates/diffusion` (`procraft-diffusion`) | new | requests, model families, graph builder, ComfyUI client, worker, resolution planner, styles, tags, requirements |
| `crates/core` | small, behind `generation` | `generation.rs`: the app-wide service object and the per-artwork state; snapshotting the canvas; previews; Apply |
| `crates/engine` | tiny | a layer's optional `control` property (serialised, default none) |
| `crates/gpu` | small | a preview overlay drawn above the current layer (§B8) |
| `desktop` | behind `generation` | the button, popover, results bar, settings page, layer-menu row, badge |
| `android` | behind `procraft.generation` | the same in Compose; `INTERNET` permission; cleartext to the server |
| `companion` (`procraft-companion`) | new | the CLI (Part C) |

Rules carried over from ProCraft: Rust 2024, the workspace lints (no `unwrap`, `expect`, `panic`,
indexing), `unsafe` denied, prose doc comments, semantic commits.

## B2. `procraft-diffusion` modules

| Module | Contents |
|---|---|
| `request.rs` | `Request` — everything a run needs: kind, prompt, style, strength, seed, count, input image, mask, control images, the planned sizes. Plain data, serialisable, no I/O. |
| `family.rs`, `family/anima.rs` | `Family` trait: name, how to recognise its files, text encoder and VAE, native size, control and inpaint models, and `graph(&Request, &ServerModels) -> Graph`. Anima is the one implementation in Phase 1. |
| `graph.rs` | A small builder for ComfyUI's API-format graph (`{"id": {"class_type", "inputs"}}`): `add(class, inputs) -> NodeOut`, typed outputs, `to_json()`. |
| `comfy.rs` | `Comfy` (HTTP) and `Socket` (WebSocket) — the client (§B5). |
| `worker.rs` | `Worker` — one background thread per server, a queue of runs, run state snapshots (§B4). |
| `plan.rs` | The resolution planner (§B7). |
| `styles.rs` + `styles/*.json` | `Style` and the built-in Anima styles (§B9). |
| `tags.rs` + `tags/*.csv` | The tag index and search (§B11). |
| `requirements.rs` | Required add-ons, model patterns per family and feature, pinned versions: the single list both the app and the companion use (§B12). |
| `picture.rs` | RGBA helpers: unpremultiply, crop, scale (Lanczos via `image`), PNG in/out, masks, line-art preparation. |
| `error.rs` | `Error` with the cases of §A8. |

## B3. The core API (UniFFI, exported to Kotlin; called directly by desktop)

Two objects:

- **`Generation`** — app-wide: the connection, the worker, styles, tags. Created once by the app with
  the settings.
- **The artwork's part** — the run, results and previews, held in `Generation` for the open
  artwork; its methods take the artwork's `CanvasSession`, and a different session drops the last
  one's run and results. *(M2: kept in `Generation` rather than added to `CanvasSession`'s state, so
  ProCraft's own session needs no new fields: the core change is one new module and a few lines.)*

As built in M2 (`crates/core/src/generation.rs`): `Generation::new(settings)`, `set_settings`,
`connect`, `server() -> ServerStatus`, `styles() -> Vec<StyleInfo>`, `generate(session, RunSettings)`,
`cancel`, `state() -> GenerationState`, `show_result(session, Option<id>)`,
`apply_result(session, id) -> Option<layer>`, `discard_results(session)`, `leave(session)`,
`set_folder(session, folder)` (the artwork's folder, for results kept with it) and
`generation_address(text)`. M3 adds `selection(session) -> Option<SelectionInfo>`, the context box
and guessed mode the Form's Selection row uses. The sketch below was the plan.

```rust
// app-wide
#[derive(uniffi::Object)] pub struct Generation { /* worker, client, tags, styles */ }

#[uniffi::export]
impl Generation {
    #[uniffi::constructor] pub fn new(settings: GenerationSettings) -> Arc<Self>;
    pub fn set_settings(&self, settings: GenerationSettings);   // address, nsfw
    pub fn connect(&self);                                      // (re)connect now, in the background
    pub fn server(&self) -> ServerStatus;                       // state, version, readiness per feature
    pub fn styles(&self) -> Vec<StyleInfo>;                     // built-in + fine-tunes on the server
    pub fn suggest(&self, text: String, caret: u32, limit: u32) -> Vec<TagSuggestion>;
}

// the artwork's part
#[uniffi::export]
impl CanvasSession {
    pub fn generate(&self, generation: &Generation, run: RunSettings) -> Result<u64, GenerationError>;
    pub fn cancel_run(&self, generation: &Generation, run: u64);
    pub fn generation_state(&self, generation: &Generation) -> GenerationState; // runs, results, progress
    pub fn show_result(&self, result: Option<u64>);   // preview on the canvas, or none
    pub fn apply_result(&self, result: u64) -> Option<u64>; // new layer id
    pub fn discard_results(&self);
    pub fn set_control_layer(&self, layer: u64, control: Option<ControlKind>);
    pub fn set_control_strength(&self, layer: u64, strength: f32);
    pub fn control_layers(&self) -> Vec<ControlLayerInfo>;
    pub fn selection_mode_guess(&self) -> SelectionMode;
}
```

Records (`uniffi::Record`): `GenerationSettings { address, nsfw }`, `RunSettings { prompt, style,
strength, seed: Option<u64>, count, resolution, mode: Option<SelectionMode> }` (`mode` defaults to
none, which guesses), `SelectionInfo { x, y, width, height, mode }`, `GenerationState { runs:
Vec<RunInfo>, results: Vec<ResultInfo>, shown: Option<u64> }`, `RunInfo { id, progress, preview:
Option<Thumbnail>, error: Option<String> }`, `ResultInfo { id, run, seed, thumbnail }`,
`ServerStatus`, `StyleInfo`, `TagSuggestion { tag, kind, count, source, nsfw }`,
`ControlLayerInfo`. Enums: `ControlKind { LineArt, Scribble }`, `SelectionMode { Fill, Add, Remove,
Expand }`.

**How the UIs follow it — polling, as ProCraft does for exports.** No callbacks: the UI reads
`generation_state()` and `server()` (cheap snapshots behind a mutex) — Android in a coroutine every
100 ms while the popover or bar is open or a run is going; desktop each frame, asking egui for a
repaint every 100 ms while busy. Thumbnails in the snapshot are small (≤ 160 px) and only change when
a result arrives.

## B4. Threads and a run's life

```
UI thread          CanvasSession (lock held briefly)       Worker thread                 ComfyUI
──────────         ─────────────────────────────────       ─────────────                 ───────
generate(run) ──►  1. snapshot: picture without control
                      layers & preview (read_composite_without),
                      selection mask, control layers' pixels,
                      canvas profile
                   2. Request built, queued ─────────────► 3. plan sizes, crop, scale,
                   ◄── run id                                  PNG-encode, build graph
                                                           4. POST /prompt ─────────────► queue
                                                           5. WS events: progress,  ◄─── run
                                                              previews, images
                                                           6. decode, scale back to
                                                              the target rect, keep
generation_state() ◄─── snapshot ◄────────────────────────    as results
show_result(id) ──► 7. overlay set on the renderer
apply_result(id) ─► 8. new layer through change("Generate"),
                       pixels uploaded, history step
```

- The canvas lock is held only for 1, 7, 8. Encoding, network and decoding run on the worker.
- **One run at a time per server**, queued in order; Phase 1's UI only starts one at a time anyway
  (Generate is "Cancel" while running).
- **Leaving the artwork** cancels its running job; the pictures it finished are kept and saved with
  the artwork, and opening it again brings its results back (§B10, §D3 5).
- The worker thread lives as long as `Generation`; it parks when idle.
- **Colour**: the picture is converted from the canvas's profile to sRGB on the way out and back on
  the way in (core's existing conversion, `colour.rs`).

## B5. ComfyUI protocol used

| Call | For |
|---|---|
| `GET /system_stats` | reachability, ComfyUI version, GPU name and VRAM |
| `GET /object_info/{node}` | add-on check, model lists (from the loaders' choices: `UNETLoader`, `CLIPLoader`, `VAELoader`, `ETN_control_load`) |
| `POST /prompt {prompt, client_id}` | queue a graph; returns `prompt_id` |
| `WS /ws?clientId=…` | events: `progress {value,max}`, `executing`, `execution_success`, `execution_error`, `execution_interrupted`; binary frames carrying preview and result images |
| `POST /interrupt` | cancel the running graph |
| `POST /queue {"delete": [id]}` | cancel a queued one |

- **Images in**: as base64 PNG inside the graph, `ETN_LoadImageBase64` / `ETN_LoadMaskBase64`
  (nothing stored on the server).
- **Images out**: `ETN_SendImageWebSocket` (PNG) — arrive as binary WebSocket frames; nothing saved
  on the server.
- **Same request twice** (same prompt, style and locked seed): ComfyUI answers an identical graph
  from its cache and doesn't run the sending node, so no picture comes. The worker remembers the
  last 8 pictures by graph and uses that; if it hasn't got it (e.g. the app restarted), it asks
  again with the sending node set to JPEG, so only that node runs (≈ 0.5 s). *(M1, tested.)*
- **Telling results from previews**: a picture is a result when it arrives while the sending node
  is the one running (from the `executing` events); any other picture is a sampling preview.
  *(M1.)*
- **Intermediate previews** need ComfyUI started with `--preview-method auto`; the companion's
  `start` passes it, and the UI shows them when they come. *(M1: verified for Anima, one preview per
  step.)*

**Networking crates**: `ureq` (HTTP, blocking, rustls) and `tungstenite` (WebSocket, blocking),
both on the worker thread, no async runtime — ProCraft has none. Base64 is ProCraft's own
(`engine/src/base64.rs`), JSON `serde_json` (already in the workspace).

## B6. Graphs (Anima)

**One graph per picture** *(M1)*: a run for *n* pictures queues *n* graphs, seeds one apart, rather
than one batch. Each picture comes back as soon as it's made, and each can be made again from its
seed alone. (A batch would be a little faster overall but only sends everything at the end.)

Shared start for every graph:

```
UNETLoader(checkpoint) ─► [ETN_control_load(lllite) ─► ETN_control_apply(image, strength, 0..1, mask?)]* ─► model
CLIPLoader(qwen_3_06b_base, type=omnigen2) ─► CLIPTextEncode(positive), CLIPTextEncode(negative)
VAELoader(qwen_image_vae)
KSampler(sampler, scheduler, steps, cfg from the style; seed; denoise)
```

| Kind | Graph |
|---|---|
| **Generate** (100 %, no selection) | `EmptyLatentImage(w, h, 1)` → KSampler(denoise 1) → VAEDecode → `ETN_SendImageWebSocket` |
| **Refine** (< 100 %, no selection) | `ETN_LoadImageBase64` → VAEEncode → KSampler(denoise = strength) → VAEDecode → send |
| **Selection** (any mode) | image + mask → `INPAINT_ExpandMask(grow, blur)` → pre-fill per mode (`INPAINT_MaskedFill`: neutral, telea or navier-stokes) → VAEEncode → `SetLatentNoiseMask` → model with the **LLLite inpaint** control (pre-filled image, mask) → KSampler(denoise = strength) → VAEDecode → `ImageCompositeMasked` onto the original crop with the blurred mask → `INPAINT_ColorMatch` → send |
| **+ Control layers** | one `ETN_control_load`/`apply` pair per control layer, with the **any-test** LLLite model, its strength, on the model before sampling |

Mode details for selections:

| Mode | Pre-fill | Inpaint control | Prompt / negative |
|---|---|---|---|
| Fill | neutral grey | yes | as typed |
| Add | neutral grey | yes | as typed (required) |
| Remove | telea (from surroundings) | yes | prompt dropped; negative + "object, person, text" |
| Expand | navier-stokes (continues the edges) | yes | as typed |

*As built (M3):* a selection is **one pass at its target size**, with the largest-picture cap
applied. A second pass would have to repeat the whole inpaint structure, so Quality on a large box
is generated at the capped target and scaled from there. The mask grows by 2 % of the context box's
longer side (at least 8 px) and blurs by half that (`INPAINT_GROW`, `INPAINT_GROW_MIN`,
`INPAINT_BLUR_SHARE` in `family/anima.rs`); the inpaint control runs at full strength and the last
colour match at 0.5. Fill and Add pre-fill neutral grey, Remove telea (and appends ", object, person,
text" to the negative), Expand navier-stokes. The graph composites onto the box's crop through the
blurred mask; core then multiplies the selection's own mask (feather included) into the result's
alpha, so the overlay and the applied layer show only the selected part.

**Control images** are prepared in `picture.rs`, not in the graph: the layer flattened onto white,
turned to white lines on black (what the control models expect), cropped and scaled like the
picture. Exact settings (grow, blur, control strengths for Anima) are tuned in M3/M4 against the
dev server and kept as constants in `family/anima.rs`.

## B7. Sizes: the resolution planner

Anima works best near **1 MP**. For an area of `W × H` canvas pixels (the canvas, or the selection's
context box):

1. **Context box** (selections): the selection's bounds grown by 50 % of their size on each side (at
   least 64 px), clamped to the canvas.
2. **Generation size**: scale the box to about 1 MP keeping its shape: `s = √(1 048 576 / (W·H))`,
   then round each side to a multiple of 16, each side between 512 and 2048.
3. **Back to the canvas**: results are scaled to `W × H` (Lanczos) and placed at the box's position.
4. **Big areas** (box > 2 MP, e.g. a full 4K canvas): generated at 1 MP and scaled up, which softens
   detail. *(This was M2's behaviour; replaced by B7a, built below.)*

### B7a. Generation resolution (decided 2026-11-05, built 2026-11-05)

The area's **target size** is chosen in the popover; the model reaches it in one or two passes, as
krita-ai-diffusion does (`backend/resolution.py`: canvas size × a multiplier, capped, and two
passes when the target is much bigger than the model's size).

**A "Resolution" row in the Generate popover**, three choices in the count's style:

| Choice | Target size | How |
|---|---|---|
| **Draft** | the model's own size (≈ 1 MP, shape kept) | one pass, scaled to the area: fast, soft on big canvases (M2's behaviour) |
| **Normal** | half the area's size each way (at least the model's size) | one pass if it's near the model's size, else two |
| **Quality** (default) | the area's own size | one pass if it's near the model's size, else two |

- **Two passes** when the target is more than about 1.5 × the model's pixels: compose at ≈ 1 MP,
  scale that up to the target, then refine at the target size with a low denoise (≈ 0.35–0.45, to
  tune for Anima), same prompt, seed and style. Results are then scaled to the area if the target
  was smaller (Normal) or capped.
- **Small text under the row** says what will happen, e.g. "2800 × 2000 · 2 passes",
  "1400 × 1000 · 2 passes", "1216 × 864 → scaled up", or "capped at 6 MP".
- **The cap: "Largest picture"** in the server view, in megapixels, filled in from the GPU's memory
  as the server reports it (24 GB → 6 MP, 16 GB → 4, 12 GB → 3, 8 GB or less → 2) and changeable.
  A target above it is generated at the cap and scaled up.
- **Remembered** with the app's other generation settings; **Quality** at first.
- **Selections**: the same, with the context box as the area, but always one pass at the target
  (see §B6): the inpaint structure is not repeated for a second pass.
- **The results bar** notes when a result was scaled up by more than 1.5 × ("scaled up from
  1216 × 864"), the note M2 left out.
- Expected cost on a 24 GB GPU for 2800 × 2000 with the Anima style: Draft ≈ 9 s; Quality roughly
  25–40 s (to measure when built); Anima Turbo much less.
- *Built:* a megapixel is 1,000,000 pixels. Both make two passes above 1.5 × the model's pixels
  (so Normal on 2800 × 2000, 1400 × 1000, is one pass at 1408 × 1008). Refining is one pass at the
  target, never below the model's own size.
- *Tuned on the dev server (2026-11-05):* Anima's passes are made at **3 MP at most**, whatever the
  GPU could take: a second pass at 5.6 MP (2800 × 2000) came out soft with a faint grid, while 2–3 MP
  added clean line detail over Draft. Bigger targets are made at 3 MP and scaled up the rest (≈ 1.37×
  for 2800 × 2000, under the note's 1.5×). The second pass refines at **0.3**: at 0.4 it changed
  details such as eye colour. Times on a 24 GB GPU, Anima style, 2800 × 2000: Draft ≈ 10 s warm,
  Quality (2048 × 1456, 2 passes) ≈ 33 s. The small text says "capped at 3 MP" there.
- *Selections, tried:* Add (a black cat on a table) and Remove (a vase) work at inpaint control
  strength 1.0; Remove at 100 % sometimes puts a small new object back. Fill on the tablet (a full
  moon in an ellipse of night sky) blends with no visible edge.

## B8. Previews on the canvas

Previews are **not layers**. The renderer gets a small new feature: an **overlay**, one picture
over a canvas rectangle, shown in the window and nowhere else. It never enters the document,
history, saves, exports or the time-lapse.

*As built (M2):* `Canvas::set_overlay(gpu, Option<(IRect, &[u8])>)` and `has_overlay()`. It's drawn
in the **display pass**, right after the finished picture and before the selection's marching ants,
with the same display pipeline (so it gets the screen's colour conversion and zoom). So it shows
**over every layer**, not just above the current one: compositing it into the layer stack itself
would have touched the live-cache, group and clipping paths of a 12k-line renderer that ProCraft
changes often. Apply therefore puts the layer at the top (§A4), and a selection's mask is baked into
the overlay's alpha before it's sent. Tested: `crates/gpu/tests/canvas.rs`
`an_overlay_shows_over_the_canvas_in_the_window_only`.

## B9. Styles

A style (`styles/anima.json`, our own file):

```json
{ "id": "anima", "name": "Anima", "family": "anima",
  "checkpoint": ["anima_base*", "anima-base*"],
  "positive": "masterpiece, best quality, {prompt}",
  "negative": "worst quality, low quality, blurry, jpeg artifacts, watermark, signature",
  "sampler": "euler_ancestral", "scheduler": "normal", "steps": 30, "cfg": 4.0 }
```

| Style | Checkpoint | Sampler | Steps | CFG |
|---|---|---|---|---|
| **Anima** | `anima_base*` | euler ancestral / normal | 30 | 4 |
| **Anima Turbo** | `anima_turbo*` | euler / simple | 8 | 1 |
| *each other Anima checkpoint on the server* | that file | as Anima | 30 | 4 |

- No rating tag: the NSFW switch only filters tag suggestions, and what people generate is up to
  them. (Until 2026-11-05 a `safe` tag was added while it was off.) Without a subject in the
  prompt, Anima Turbo can produce explicit pictures, so tests use explicit safe subjects.
- Fine-tunes are recognised as Anima by name (`*anima*` in `diffusion_models/`) — the same rule
  krita-ai-diffusion uses — and get the Anima style's settings.
- With strength < 100 %, nothing changes in the style: ComfyUI's KSampler with `denoise` below 1
  still runs the style's number of steps, over the tail of a longer schedule. *(M1.)*

## B10. Saved with the artwork

- **Control layer**: `Layer.control: Option<{ kind, strength }>` in the engine's document,
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. Old artworks load unchanged; an
  artwork with control layers opened in upstream ProCraft just ignores the field.
- **Results** (brought forward from Phase 2): kept in the artwork's folder, in a private
  `generation/` subfolder that no export reads. `index.json` lists every result in order (its id,
  seed, name, area, scaled_from and thumbnail size), `<id>.png` holds its pixels losslessly
  (premultiplied RGBA8, in the canvas's colours) and `<id>.thumb.png` its small picture. They are
  written on a background thread, each PNG and the index beside its final name until whole and
  then renamed, the index last; an interrupted save leaves the last index whole. At most 24 are
  kept per artwork: adding another drops and deletes the oldest. Discard deletes them; opening the
  artwork reads only the thumbnails, on a background thread, and they appear in the bar when ready.
  Only the shown result keeps its full pixels in memory (read from its PNG then, dropped when
  another is shown), as does one just made until it is written; `apply_result` reads them if
  needed. `set_folder` tells generation where the open artwork lives. An app built without the
  `generation` feature ignores the folder.
- Nothing else in Phase 1 (history: Phase 2).
- App-wide, kept like ProCraft's other preferences (Android `SharedPreferences`, desktop's
  `interface` file): server address, NSFW switch, last style, recent prompts (20).

## B11. Tags

- **Files**: krita-ai-diffusion's four tag CSVs (Danbooru, Danbooru NSFW, e621, e621 NSFW;
  `tag,type,count,aliases`), copied as they are into `crates/diffusion/tags/`, with a line each in
  `ATTRIBUTION.md`. ≈ 4.2 MB; deflated at build time (`build.rs`, `miniz_oxide`) and embedded, so
  both apps get them without asset plumbing (≈ 1.3 MB in the binary).
- **Index**: loaded once on first use (~100 ms): one list of entries sorted by normalised tag
  (lower case, `_` → space), plus aliases pointing at their tag.
- **Search**: the word being typed at the caret (text since the last comma) is matched by prefix
  against tags and aliases (binary search), ranked by `count / largest count in its source` so
  Danbooru and e621 mix fairly; duplicates across the two keep the higher. NSFW entries only when
  the switch is on. Top 12 returned.
- **Inserted** with spaces instead of underscores, except tags that are emoticons (`^_^`, `>_<`).
- *As built (M4):* `build.rs` deflates each CSV into `OUT_DIR` and `tags.rs` embeds the four
  blobs; a `OnceLock` inflates and indexes them on first use, whichever thread asks. The core's
  `Generation::suggest_tags` returns the top 12 with UTF-16 ranges (`complete_tag` inserts one),
  and recent prompts are kept there—newest first, at most 20—and saved by the desktop in its
  `interface` preferences as one escaped `generation_recent` line each.

## B12. Requirements (shared with the companion)

`requirements.rs` holds one list:

| Need | Required for | Check |
|---|---|---|
| `comfyui-tooling-nodes` | everything (image in/out) | `ETN_LoadImageBase64`, `ETN_SendImageWebSocket` present |
| ″ | control layers, selections | `ETN_control_load`, `ETN_control_apply` present |
| `comfyui-inpaint-nodes` | selections | `INPAINT_ExpandMask`, `INPAINT_MaskedFill`, `INPAINT_ColorMatch` present |
| Anima checkpoint | everything | a `UNETLoader` choice matching `*anima*` |
| `qwen_3_06b*` text encoder | everything | `CLIPLoader` choice |
| `qwen_image_vae*` | everything | `VAELoader` choice |
| `anima*lllite*inpaint*` | selections | `ETN_control_load` choice |
| `anima*lllite*any*` | control layers | `ETN_control_load` choice |

…plus, for the companion only: pinned commits, download URLs and SHA-256 for each model.
`server()` turns the check into per-feature readiness for §A7/§A8.

## B13. The `generation` build switch

- **Cargo**: `procraft-core` feature `generation` (pulls in `procraft-diffusion`, `generation.rs`);
  `procraft-desktop` feature `generation` (on by default in our fork) enabling its UI modules;
  `procraft-gpu`'s overlay is small and always built (no network in it).
- **Gradle**: property `procraft.generation` (default on in the fork) passes `--features generation`
  to cargo-ndk and picks the Kotlin source folder: `src/generation/java` holds the real
  `GenerationUi` object, `src/noGeneration/java` one that draws nothing. The shared UI code calls
  `GenerationUi` only. *(As built, M2:)* the manifest isn't swapped (Android merges one main
  manifest); it takes two placeholders instead: `${networkPermission}` (`INTERNET`, or an unused
  permission name when off) and `${cleartext}` for `android:usesCleartextTraffic` (the server is
  plain HTTP on the LAN; Android can't allow-list a host typed at run time).
- **Icon** *(M2)*: "AI" is defined in code (three stroked capsules, `desktop/src/generation.rs` and
  `GenerationUi.kt`) rather than drawn by `tools/icons/draw_icons.py`; moving it into that pipeline
  (master SVG, `ATTRIBUTION.md`) is upstream-preparation work.
- With the switch off, ProCraft builds and behaves exactly as upstream.

---

# Part C — The companion

## C1. Commands

```
procraft-companion install [--dir DIR] [--gpu nvidia|amd|intel|cpu]
procraft-companion models anima [--dir DIR] [--accept-licence]
procraft-companion check [DIR | URL]
procraft-companion start [--dir DIR] [--port 8189] [--lan]
procraft-companion stop [--dir DIR]
```

Default `DIR`: `~/.local/share/procraft-companion` (Linux), `%LOCALAPPDATA%\ProCraft Companion`
(Windows).

## C2. What each does

- **install**
  1. Downloads `uv` (pinned version) into `DIR/uv/`.
  2. `uv venv --python 3.12 DIR/venv`.
  3. ComfyUI and each add-on as **GitHub archive downloads of the pinned commit** (no git needed),
     unpacked to `DIR/ComfyUI` and `DIR/ComfyUI/custom_nodes/<name>`.
  4. PyTorch from the index for the GPU (`--gpu`, else detected: `nvidia-smi` → CUDA 13.0;
     `rocminfo` → ROCm; otherwise CPU, with a warning).
  5. ComfyUI's and each add-on's `requirements.txt`.
  6. Writes `DIR/companion.json` (versions installed). Re-running repairs/updates to the pins.
- **models anima** — prints the licence line and link, asks to accept (or `--accept-licence`), then
  downloads each file from its official Hugging Face URL into the right `models/` folder:
  resumable (HTTP `Range`), verified by SHA-256, skipped if already there and correct.
- **check** — read-only. On a folder: versions, add-ons, models (same rules as §B12). On a URL: the
  same through the API. Prints what works and what's missing. Never writes anything.
- **start** — runs ComfyUI from `DIR` with `--port`, `--preview-method auto`, `--listen 0.0.0.0` only
  with `--lan` (with a warning that ComfyUI has no login); writes `DIR/comfyui.pid`; prints the
  address to type on the tablet (the machine's LAN address when `--lan`, else the `adb reverse`
  line). Foreground; Ctrl+C stops.
- **stop** — as our `dev/stop-dev-comfyui.sh`: Ctrl+C (Windows: `CTRL_BREAK`), then terminate, only
  the process recorded or started from `DIR`.
- It **never modifies a folder it didn't install** (no `companion.json` → read-only).

## C3. Built with

Rust, in our fork's workspace (`companion/`), sharing `requirements.rs` with the app. `ureq` for
downloads, `sha2` for hashes, `flate2`/`zip` for archives, `clap` for arguments. Linux and Windows;
it is accepted when it rebuilds our dev instance from nothing and the app passes its check.

---

# Part D — Testing and plan

## D1. Testing

| Level | What |
|---|---|
| Unit (`cargo test`) | graph builder (golden JSON per kind), planner (sizes, rounding, clamps), tag search (prefix, aliases, mixing, NSFW filter), requirements check against saved `object_info`, picture helpers |
| Against the dev ComfyUI (`cargo test -- --ignored`, `PROCRAFT_COMFY_URL`) | each graph kind runs and returns the right count and size; cancel; missing-model errors |
| Core (`crates/core/src/tests.rs` style) | apply = one undo step; selection masking; control layers left out of the picture; leaving an artwork cancels |
| On the test tablet | a checklist per milestone: the screens at 1120 × 800 dp, keyboard overlap, pen and finger, Wi-Fi and `adb reverse` |
| Desktop | the same checklist with mouse and keyboard |

## D2. Work breakdown

| Milestone | Steps |
|---|---|
| **M1 Foundations** ✅ | fork + branch; `crates/diffusion` skeleton; graph builder; `Comfy` client (HTTP + WS); requirements check; Anima family: generate + refine; planner; a `diffusion-cli` test harness (`generate "prompt" -o out.png`); unit + dev-server tests |
| **M2 Generate on the tablet** ✅ | renderer overlay spike; `generation.rs` in core + UniFFI; `generation` switches (Cargo, Gradle, manifest); Android: button, icon, popover (prompt, styles, strength, seed, count), results bar, Prefs page, states; desktop: the same; on-device pass |
| **M3 Selections** | selection graphs + modes; mode guess; Selection toolbar button; tuning grow/blur/colour match on real pictures |
| **M4 Control layers + autocomplete** | engine `control` property; layer menu row + badge; popover rows; control image preparation; tag files, index, suggestion strip, recent prompts |
| **M5 Companion** | `procraft-companion` install / models / check / start / stop; model list with hashes; Windows pass; rebuild the dev instance with it |

### M2 as delivered (2026-11-05)

Tested on the test tablet against the dev ComfyUI over `adb reverse`: set up the server, generate
two pictures (live previews, progress on the button and in the bar), preview on the canvas, Apply
(layer at the top, refreshed in the Layers panel), refine at 50 % on a P3 canvas, one undo removing
an applied layer. Fixed on the device: the keyboard covering Generate (the popover now shrinks above
it and Generate stays outside the scrolling part), Apply not refreshing Android's layer list, a
pill drawn opaque. Desktop builds and is lint-clean with the feature on and off, but hasn't been
tried by hand yet.

Not done yet, carried forward:
- **The icon in `draw_icons.py`** (§B13) and the "Image generation" row's place in the Prefs tab
  (after Gesture controls).

## D3. Decisions

| # | Question | Answer |
|---|---|---|
| 1 | Icon | **"AI"**, drawn from capsules (§A2) |
| 2 | Name | **"Control layer"** — the term ComfyUI users know |
| 3 | Desktop shortcut | **`G`** opens Generate |
| 4 | Popover on Generate | **Stays open** for tweaking (§A3) |
| 5 | Leaving an artwork during a run | **(a) cancel the run; results stay saved with the artwork** |

**5, in detail.** Results are saved with the artwork (§B10, brought forward from Phase 2). Going
back to the Gallery, or opening another artwork, while a run is going cancels the run: the pictures
it finished are kept and saved, and opening the artwork again (even after a restart) brings them
back. A session-only artwork, with no folder set, keeps its results in memory for that session.
