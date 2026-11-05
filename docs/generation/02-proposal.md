# ProCraft Diffusion: proposal

Status: **final.** Built from `01-brainstorm.md` (decisions D1–D13). Once agreed,
`03-design.md` turns it into screens, APIs and data formats. No product code before the design.

---

## 1. Summary

Add image generation to ProCraft, in the spirit of krita-ai-diffusion: generate, refine and fill
selections with a diffusion model, guided by the user's own sketches, without leaving the canvas.
ProCraft on the tablet is the client; the models run in **ComfyUI on a PC**, which a small
**companion tool** sets up. The first model family is **Anima**. It is built **mobile first** (Android
tablet), works and looks the same on the desktop app, and is written to be offered upstream to
ProCraft later.

## 2. Goals

1. **Feels like ProCraft.** A tool reached from the top bar, frosted popovers, a bottom bar for
   results, ProCraft's colours, type, icon style and gestures. Not a docked panel of AI parameters.
2. **Precision over slot machine.** Selections limit where it works, strength decides how much
   changes, and the user's line art or scribble steers the shape.
3. **Fits the painting loop.** Results land as ordinary layers; undo works; painting continues
   while a job runs.
4. **Tablet first.** Comfortable with a finger and a pen, and with little typing.
5. **Simple to set up for someone who already knows ComfyUI**, and possible for someone who doesn't,
   with the companion.
6. **Upstream-ready.** ProCraft's code style and lints, the feature isolated so it can be compiled out,
   no GPL code.

## 3. Non-goals for Phase 1

- Running models on the tablet, or in the ProCraft process on the desktop.
- A hosted/cloud service, accounts, payment.
- Model families other than Anima (the code is ready for more; they're added later).
- Editing styles in the app, and LoRA management. In Phase 1 the user picks a style from the cards;
  its settings (model, prompt template, negative prompt, sampler, steps, CFG, LoRAs) come from
  the presets we ship and can't be changed in the app yet. Style editing is the first thing after
  Phase 1, once everything is tested and works.
- Regional prompts, Live painting, upscaling, edit models, custom ComfyUI workflows, animation.
- Network discovery, QR codes and pairing tokens (planned, D11).
- A graphical companion (it's a command-line tool first).

## 4. Who it's for

- **The painter with a tablet and a gaming PC**: draws on the tablet, the PC does the generation on
  the same network.
- **The desktop painter**: ProCraft and ComfyUI on the same machine.
- Both are assumed to be comfortable following a short setup guide and running one command.

## 5. Phase 1: what the user gets

### 5.1 Generate and refine

- A **Generate** button on the top bar, among the artwork actions (Actions, Adjustments, Selection,
  Transform). Its icon is a new drawing to ProCraft's style guide, with an idea of its own (the
  Adjustments wand already owns sparkles).
- It opens the **Generate popover**:
  - **Prompt** (tags or plain words) with **tag autocomplete** and recent prompts as chips. The tag
    lists (Danbooru and e621) ship with the app; NSFW tags are included but off until switched on
    in the settings.
  - **Style** cards: a style is a model plus prompt template, negative prompt and sampler settings.
    Phase 1 ships a few Anima styles (e.g. base quality, turbo/draft) and lists the server's Anima
    checkpoints.
  - **Strength**: 100 % makes a new image; lower keeps more of what is on the canvas (refine).
  - **Seed**: random by default, or fixed (with the seed of any result reusable).
  - **Count**: how many results per run (1–4).
  - **Generate** button and a connection status.
- What it works on, with no extra switch: **the visible picture**; with a **selection**, only the
  selected area (see 5.2).
- While it runs, the user can close the popover and keep painting; a small progress indicator shows on
  the button and in the results bar. One job runs at a time; the user can cancel it.

### 5.2 Selections

- With a selection, generation works inside it only: the area plus surrounding context is sent, the
  result is blended back into the selection with its feather.
- Modes, chosen automatically and changeable: **Fill** (default), **Add** (something new from the
  prompt), **Remove** (fill with surroundings, prompt optional), **Expand** (fill transparent areas
  at the canvas's edges after Crop & Resize made it bigger).
- The **Selection toolbar** gets a Generate button that opens the popover with the selection in use.

### 5.3 Results

- Results arrive in a **results bar** at the bottom of the screen (like an adjustment's bar), with
  thumbnails of the run.
- Tapping a thumbnail shows it **on the canvas** as a preview (a private layer the gallery's
  thumbnail leaves out). Nothing is committed yet.
- **Apply** places it as a **new layer** above the current one, named after the prompt, masked to the
  selection when there was one. **Discard** clears the run. **Again** runs the same settings with a
  new seed.
- Apply is one undo step.
- Results of this session's runs stay browsable until the artwork is closed.

### 5.4 Control layers (sketch guidance)

- Any layer can be made a **control layer**: **Line art** or **Scribble**, with a strength. It steers
  the composition of the next generation and stays a normal layer the user keeps drawing on.
- Made from the layer's options menu (beside Alpha Lock, Mask, Reference, Drawing Assist) and shown
  in the Generate popover, where its strength is set or it is removed. The Layers panel marks it with
  a small badge.
- Anima does this with its LLLite control models ("any-test" for line art and scribble).
- The name is to be settled in the design: "Guide" and "Reference" already mean other things in
  ProCraft (Drawing Guide, the reference layer).

### 5.5 Connection and settings

- **Actions › Prefs › Image generation**: server address (e.g. `http://<LAN address>:8189`), a
  **Test** button, and what the server has: ComfyUI version, required add-ons, Anima models.
- First use from the Generate popover with nothing set up shows a short card that leads there.
- On connect, ProCraft checks for the required add-ons and models and names exactly what is missing.
- Clear errors for an unreachable server, a dropped connection mid-job, out-of-memory on the GPU.
- **Privacy note** in the settings: pictures and prompts go to the server you name, nowhere else.
  No telemetry.

### 5.6 The companion (`procraft-companion`, command line)

| Command | What it does |
|---|---|
| `install [dir]` | A private Python (via `uv`), ComfyUI and the required add-ons at pinned versions, PyTorch for the GPU found (NVIDIA first; AMD/Intel/CPU as they're tested) |
| `models anima` | Downloads the Anima pack (checkpoint, text encoder, VAE, LLLite inpaint and any-test models) with checks and resume; skips files already there |
| `check [dir \| url]` | Read-only report on any ComfyUI folder or running server: version, add-ons, models, what's missing for which feature |
| `start [--lan]` | Starts ComfyUI on port 8189 and prints the address to type into the tablet |

- It never modifies a ComfyUI it didn't install.
- Before downloading a model it names the model's licence and links it (for Anima: CircleStone Labs
  Non-Commercial License, "Built on NVIDIA Cosmos"). It downloads from the official repositories
  only; we never host or bundle weights.
- Linux and Windows in Phase 1 (ProCraft's desktop platforms).
- Phase 1 ends with it rebuilding our dev instance from scratch, as its acceptance test.

## 6. How it's built (outline; detail in the design)

```
 Android (Compose) ─┐                          ┌─ ComfyUI on the PC
 Desktop (egui) ────┼─ crates/core ─ crates/diffusion ── HTTP / WebSocket ─┤   + tooling, inpaint,
                   ─┘   (glue, UniFFI)  (no UI, no GPU)                    │     controlnet_aux add-ons
                                                                           └─ set up by procraft-companion
```

- **`crates/diffusion`** (new): the request type that holds everything a generation needs; the
  ComfyUI graph builder per model family (Anima first); the ComfyUI client (upload images, queue,
  progress, results, cancel, model and add-on discovery); resolution logic (generate near the model's
  native size, crop selections with context, scale results back); styles; the tag list for
  autocomplete. Testable on its own against the dev ComfyUI.
- **`crates/core`** (small additions): a generation session tied to the canvas session: reads the
  picture, the selection and control layers; shows previews; applies results with history; exposes it
  all over UniFFI to Android and directly to desktop.
- **Android and desktop UIs**: the popover, the results bar, the settings section, the layer badge,
  the icon. Pixel-matched, as ProCraft keeps its two UIs today.
- **`procraft-companion`** (new crate/binary): the companion; shares the add-on and model lists with
  `crates/diffusion` so the app and the companion agree on what "ready" means.
- **Isolated**: everything behind a `generation` build feature; with it off, ProCraft builds as today,
  without networking code or the Android `INTERNET` permission. Android also needs plain HTTP allowed
  to the user's server (it blocks cleartext by default).
- **No async runtime** is expected: ProCraft has none, and blocking calls on a worker thread fit how
  it does background work today (to be confirmed in the design).

## 7. Milestones

Each milestone works on the tablet **and** the desktop before the next starts.

| # | Milestone | Done when |
|---|---|---|
| M1 | **Foundations** | `crates/diffusion` makes an Anima picture from a prompt on the dev ComfyUI, from a test harness; model/add-on discovery reports correctly |
| M2 | **Generate on the tablet** | From a blank canvas on the test tablet: prompt, style, strength, seed, count → previews → Apply as a layer → undo. Same on desktop. Settings and connection errors work |
| M3 | **Selections** | Fill / Add / Remove / Expand blend in without visible seams; Selection toolbar shortcut |
| M4 | **Control layers + autocomplete** | A line-art or scribble layer steers the result; tag autocomplete is quick on the tablet keyboard |
| M5 | **Companion** | `procraft-companion` installs a working server from nothing on Linux and Windows and rebuilds our dev instance |

Phase 1 is done after M5, plus a pass on polish and the setup guide.

## 8. After Phase 1 (direction, not commitment)

- **Phase 2:** depth and pose control, extracting line art from a picture, upscaling, editing styles
  and LoRAs, a job queue, history saved with the artwork, more model families (e.g. SDXL/Illustrious,
  Flux 2 Klein, Z-Image), a ProCraft desktop Prefs page driving the companion.
- **Phase 3:** regional prompts, Live painting (Anima turbo), pairing with QR and discovery,
  animation frames, custom workflows.

## 9. Risks

| Risk | What we do |
|---|---|
| **ProCraft moves fast**; a fork drifts | Keep our changes in our own crates and few, small touch points in core and the UIs; rebase often |
| **Licences**: krita-ai-diffusion is GPL | Write everything fresh; no ported code or copied data files |
| **Anima's licence** (CircleStone Labs Non-Commercial v1.2, plus NVIDIA's Open Model License as a Cosmos derivative) | Fine for a free app whose users run the model themselves; pictures made with it are free to use, commercially too. Not allowed: the model powering anything paid (a paid tier, a paid hosted server, a monetized product) without a commercial licence from CircleStone. So: download only from the official repo, show the licence, never host weights, keep model families pluggable so a permissively licensed one can be the default if ProCraft ever needs it. Fine-tunes of Anima carry the same licence |
| **ComfyUI or add-on changes** break graphs | Pin versions in the companion; check on connect; test the graphs against the dev instance |
| **Large canvases** are slow to send over Wi-Fi | Send only the area needed, at the size the model needs; compress |
| **Typing on a tablet** | Autocomplete, recent prompts, styles that carry the boilerplate |
| **Open LAN server** (ComfyUI has no login) | Local-only by default, USB forwarding for development, pairing later (D11) |
| **GPU memory** with two ComfyUIs on one card | Run one at a time in development |

## 10. To settle in the design (`03-design.md`)

- Screens: the popover's layout on the tablet (1120 × 800 dp) and desktop; the results bar; the
  settings section; the first-run card; the control-layer badge; the Generate icon.
- Interaction details: where previews sit in the layer stack; what happens to running jobs when the
  user switches artwork; selection-mode heuristics.
- The control layer's name in the UI.
- APIs: the `crates/diffusion` types, the core session's UniFFI surface, the threading model.
- Networking crates (and their size on Android); image transfer format.
- The ComfyUI graphs for Anima: generate, refine, inpaint, with control.
- Resolution rules for Anima.
- The default styles and their prompt templates.
- The tag list: krita-ai-diffusion's Danbooru and e621 tag files (SFW and NSFW), taken as they
  are; they're the sites' public tag data. Recorded in `ATTRIBUTION.md` with their source, as
  ProCraft does for every asset. How the two sets mix in the suggestions.
- The companion's pinned versions, model list with hashes, and Windows specifics.
- How the `generation` feature is wired through Cargo and Gradle.
- Test plan: unit tests in `crates/diffusion`, graph tests against the dev ComfyUI, on-device checks.
