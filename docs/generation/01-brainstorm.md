# ProCraft Diffusion: brainstorm (draft 1)

Status: **brainstorm**, not decided. This becomes `02-proposal.md` once the open questions are settled,
then `03-design.md`. No code before the design.

Reference checkouts live outside this repository, in `../reference/` beside the clone (ProCraft at `634be67`, krita-ai-diffusion 1.54.0 at `828b70c`).

## Decided so far (2026-11-05)

| # | Decision |
|---|---|
| D1 | **Backend: a ComfyUI server** over HTTP/WebSocket, behind a `Backend` trait so other backends can be added later. |
| D2 | **Code lives in a new `crates/diffusion`** plus thin glue in `crates/core` (recommended; not yet challenged). |
| D3 | **Fork first, upstream later.** Build in a fork, but keep it upstream-ready: their style, their lints, small reviewable commits, the feature isolated. |
| D4 | **Mobile first.** Design for an Android tablet with touch and pen first; it must work and look right on both Android and desktop. An Android tablet will be provided for testing. |
| D5 | **Entry point: a new top-bar button** with its own popover and a bottom results bar, plus a shortcut from the Selection toolbar. |
| D6 | **First model family: Anima** (Cosmos-based anime model; Qwen3 0.6B text encoder, Qwen Image VAE, booru-tag prompts). Others come later through the same `Arch` abstraction. |
| D7 | **Development server: your existing ComfyUI**, the one krita-ai-diffusion set up, so its custom nodes are already there. |
| D8 | **Connection v1: type the address.** Network discovery and QR pairing are planned, not v1. |
| D9 | **A PC companion** sets up ComfyUI the way we need it (Python, ComfyUI, custom nodes, models), runs it, and serves the tablet — the job krita-ai-diffusion's installer does for Krita. **Simple above all.** See §4. |
| D10 | **Phase 1 also gets** control layers (line art / scribble, via Anima LLLite), tag autocomplete, and seed control. |
| D11 | **Pairing (token proxy, QR, discovery) comes later**, not with the first companion. |
| D12 | **Required server add-ons:** comfyui-tooling-nodes + comfyui-inpaint-nodes from Phase 1; comfyui_controlnet_aux when control layers need preprocessing. Checked on connect. |
| D13 | **The krita-ai-diffusion ComfyUI is read-only.** Nothing on it is ever changed. Testing happens on a separate dev instance (§4a). |

---

## 1. What we found

### ProCraft (the host)

- **Three layers, one brain.** A Rust engine (`crates/engine`, no GPU), a wgpu renderer (`crates/gpu`), and
  `crates/core`: the shared session both apps drive (tools' state, history, selection, layers, exports).
  Android is Kotlin/Compose over UniFFI bindings; desktop (Windows/Linux) is egui calling core directly;
  an early iPad app is SwiftUI over the same bindings. **Rule of the codebase: behaviour lives in core,
  the UIs are thin and pixel-matched to each other** (`desktop/src/look.rs` mirrors `ui/Theme.kt`).
- **Design language.** Procreate-like: full-screen canvas, a translucent top bar (artwork actions left,
  tools right), a slim sidebar, and frosted-glass popovers hanging from bar buttons (35 dp corners,
  `#0076FD` accent, Inter type, dark and light looks). Tools with parameters get a bottom toolbar
  (Adjustments, Selection, Transform) with Apply/Cancel semantics. Icons are original, solid,
  24-unit grid, drawn by `tools/icons/draw_icons.py` to `docs/icons/STYLE_GUIDE.md` and recorded in
  `ATTRIBUTION.md`.
- **Strict code style.** Rust 2024, `unsafe` denied, clippy denies `unwrap`/`expect`/`panic`/indexing.
  Prose-style doc comments everywhere. Semantic commits.
- **Primitives we need already exist** in core: read the flattened composite (`read_composite`), the
  selection mask (`selection`, `has_selection`), insert an RGBA image as a new layer (`insert_image`,
  `paste`), undo history, private layers (left out of gallery thumbnails — handy for previews).
- **No networking at all today**: no HTTP/WebSocket crate, no `INTERNET` permission on Android.
  Adding one is a real (but small) dependency decision for the maintainers.
- Licence: **MIT OR Apache-2.0**.

### krita-ai-diffusion (the reference)

- A **thin client** to a **ComfyUI** server (local managed install, a remote/LAN server, or their paid
  cloud). The plugin itself runs no models.
- Clean split worth copying as an *idea*:
  - `WorkflowInput` — one serialisable request holding everything a generation needs.
  - `workflow.py` — turns a `WorkflowInput` into a ComfyUI node graph.
  - `comfy_client.py` — HTTP + WebSocket: upload images, enqueue, progress, fetch results, cancel.
  - `model/` — observable per-document state (prompt, style, strength, jobs, history); `ui/` — widgets.
- Features: generate / refine (strength) / inpaint with selection (fill, expand, add, remove, replace
  background), styles (checkpoint + LoRA + prompts + sampler presets), control layers (scribble, line
  art, depth, pose, …, IP-Adapter reference/style/face), regional prompts tied to layers, live painting,
  tiled upscaling, edit models (Flux Kontext / Qwen Edit / Flux 2 Klein), job queue, result history
  previewed on the canvas, custom ComfyUI workflows.
- Resolution handling is a big part of the "it just works" feel: generate at the model's native size,
  then scale/refine to canvas size; crop to the selection's bounds plus context.
- Needs custom ComfyUI nodes on the server (controlnet_aux, IPAdapter_plus, Acly's tooling and
  inpaint nodes).
- Licence: **GPL-3.0**. ⚠️ We **cannot copy or translate its code** into ProCraft (MIT/Apache). We can
  study behaviour, UX and the public ComfyUI graphs it builds, and write our own. Talking to the same
  ComfyUI custom nodes over the network is fine.
  - Why: GPL is *copyleft*: a program that includes GPL code (a Python→Rust port counts) must as a
    whole be distributed under the GPL. MIT/Apache are *permissive*: they let anyone, including
    closed-source apps, reuse the code. Code can flow from MIT/Apache into GPL, not back. If our
    fork contained ported GPL code it would have to become GPL, and ProCraft couldn't merge it
    without relicensing their whole app.
  - Not affected: running GPL programs (ComfyUI, its add-ons) and talking to them over HTTP; learning
    from how krita-ai-diffusion behaves; using model files under their own licences.
  - Rule we follow: read krita-ai-diffusion to understand it, write our own code fresh, don't port
    functions or copy data files (style presets, model lists).

---

## 2. Big decisions (each needs your call)

### D1. Where do the models run?

| Option | Pros | Cons |
|---|---|---|
| **A. ComfyUI server over HTTP/WS** (user's PC or LAN box; tablet connects to it) | Same proven path as krita-ai-diffusion; every model family; users may already have ComfyUI; tiny client | Needs a separate machine with a GPU for tablet users; network setup |
| B. On-device inference (Android NPU / desktop GPU via ONNX/candle) | Offline, no setup | Huge effort; tablets can't run SDXL/Flux at usable speed; quality gap |
| C. Hosted API (fal, Replicate, Interstice, …) | Zero setup | Accounts, money, privacy; ties an open-source app to a vendor |

**Recommendation: A**, behind a `Backend` trait so C (or a managed local ComfyUI on desktop) can be
added later without touching the UI.

### D2. Where does the code live?

**Recommendation:** a new crate `crates/diffusion` (no UI, no GPU): request types, ComfyUI graph
builder, network client, job queue, resolution logic. `crates/core` gets a thin `GenerationSession`
glue (reads canvas/selection, inserts results, history) exported over UniFFI. Android and desktop each
add only their panels. This matches how every other ProCraft feature is built and keeps the Android
and desktop UIs from drifting.

### D3. Is this headed upstream?

- **Upstream PR to `artcraft-android/procraft`:** we follow their style to the letter, keep it small and
  reviewable, need maintainer buy-in on networking, and probably gate it behind a build feature.
- **A fork / downstream build:** more freedom, but we carry a rebase burden forever.

This changes how conservative the design must be, so it matters early.

### D4. Which UI first?

Desktop (egui) is fastest to iterate on (no device deploy, easy to test against a local ComfyUI).
Android is the flagship. Core-first means both get the logic for free; the question is only which
panel we build and polish first.

---

## 3. How it could look in ProCraft

Guiding idea: **generation is a tool, not a dashboard.** No docked side panel (that's Krita's idiom,
not ProCraft's). Use the existing vocabulary: a bar button, a frosted popover, a bottom toolbar.

1. **A new top-bar button, "Generate"**, among the artwork actions on the left (next to Adjustments /
   Selection / Transform). Needs its own icon idea (the Adjustments wand already owns "sparkles") —
   e.g. *a seed sprouting a leaf*, or *a brush tip trailing a burst*.
2. **The Generate popover** hanging from it:
   - prompt field (multi-line, with recent prompts as chips — typing on a tablet is slow);
   - style picker (cards, like the brush library's sets) — Style = model + LoRAs + prompt template + sampler;
   - **Strength** slider in ProCraft's slider look: 100 % = new image, lower = refine what's there;
   - what it works on, as a segmented switch: *Canvas* / *Layer* / *Selection* (Selection is picked
     automatically when a selection exists);
   - the big **Generate** button; batch count; connection status dot.
3. **The results bar** (bottom toolbar, like an Adjustment's): appears once results arrive and the
   popover closes so the canvas is visible. Progress while generating; a strip of result thumbnails;
   tapping one previews it on the canvas on a private layer; **Apply** (new layer, or into the selection)
   / **Discard** / **Again**. Undo removes an applied result like any other step.
4. **Selection toolbar** gets a "Generate" shortcut: select an area → fill / add / remove object.
5. **Connection setup** under Actions › Prefs ("Image generation": server address, test, model list),
   with a first-run card from the Generate popover if nothing is set up.
6. **Later:** control layers shown in the Layers panel with a small badge (a sketch layer marked
   "Line art" guides generation); regions as layers with their own prompt; a Live toggle.

---

## 4. The PC companion

Tablets can't run these models, so a PC does the work. The companion's jobs:

1. **Install:** a private Python (via `uv`), ComfyUI at a pinned version, PyTorch for the GPU it
   finds (CUDA / ROCm / XPU / CPU), our pinned custom nodes, and the models a chosen pack needs
   (downloaded from Hugging Face, checked by hash, resumable).
2. **Reuse models, never modify another install:** point it at existing model folders (yours, from
   krita-ai-diffusion) and the new install reads them through its own `extra_model_paths.yaml`.
   No duplicate downloads, and the other install is never touched. (Replaces the earlier
   "adopt and top up" idea: changing someone else's ComfyUI is exactly what we don't want.)
3. **Check:** a read-only report on any ComfyUI folder or running server: version, add-ons, which
   models for which features are present or missing.
4. **Run:** start ComfyUI with the right arguments (LAN listening, port), and say the address to type
   into the tablet.
5. **Pair (later, D11):** token proxy, QR code, network discovery.

**Recommendation: a command-line tool first** (`procraft-companion install | check | models | start`),
built in Rust from a small crate in our fork's workspace. Later, ProCraft desktop gets a Prefs
section that drives the same crate with buttons and progress bars.

- Why a CLI first: it's the least to build and the easiest to test; it runs on a PC with no screen
  (or as a service); and ComfyUI users are used to a terminal.
- Why not inside ProCraft desktop first: someone painting on a tablet shouldn't need the full desktop
  painting app open just to serve images, and it would mean designing that UI before the logic is proven.
- Why Rust rather than a Python script: one toolchain with the app, and the desktop app can reuse
  the crate later.

### 4a. Development setup

What's on this machine now (read only): Linux, a 24 GB GPU. The Krita
ComfyUI (0.38.0, at `~/.local/share/krita/ai_diffusion/server`) listens on **127.0.0.1:8188 only**,
so the tablet can't reach it without restarting it with new arguments, which isn't allowed (D13). Its
`models/` folder (~303 GB) already holds everything Anima needs: base and turbo checkpoints plus
fine-tunes, `qwen_3_06b_base` text encoder, `qwen_image_vae`, and LLLite inpaint / any / depth / pose /
regional models. All four required add-ons are installed.

**Decided: a brand-new, fully separate dev ComfyUI** at `<dev ComfyUI folder>/`
(the Data drive: 466 GB btrfs, empty). It shares **nothing** with the Krita install, models included.

- its own `uv`, Python, ComfyUI and add-ons, and its own `models/` folder; port **8189**;
- the models are copied in by hand (list below); later the companion's download list must
  produce the same set;
- the tablet reaches it over USB with `adb reverse tcp:8189 tcp:8189`, so it doesn't need to listen
  on the LAN while we develop; `--listen` on the LAN when we want Wi-Fi;
- set up by hand first, then rebuilt by the companion once it exists, as its first real test;
- run one instance's generation at a time: both on one GPU can run out of VRAM;
- the drive is mounted under `/run/media`, so it must be mounted before the instance starts.

Models for Phase 1 (≈10 GB; the names are the files in the Krita install, sizes rounded):

| Folder | File | Size | For |
|---|---|---|---|
| `diffusion_models/` | `anima_baseV10.safetensors` | 4.2 GB | the base model |
| `diffusion_models/` | `anima_turboV11.safetensors` | 4.2 GB | fast drafts (and Live, later) |
| `text_encoders/` | `qwen_3_06b_base.safetensors` | 1.2 GB | reading the prompt |
| `vae/` | `qwen_image_vae.safetensors` | 254 MB | pixels ↔ latents |
| `controlnet/` | `anima-lllite-inpainting-v2.safetensors` | 66 MB | filling a selection |
| `controlnet/` | `anima-lllite-any-test-like-v2.safetensors` | 16 MB | line art / scribble control layers |

Optional: one or two favourite Anima fine-tunes (4.2 GB each) to test style switching;
`anima-lllite-depth-1` and `anima-lllite-pose-1` for Phase 2.

Findings that shape it:

- **krita-ai-diffusion's installer has no Anima download list.** It supports Anima if you bring the
  files, but ships no catalogue entry for it. We compile our own (checkpoint, Qwen3 0.6B text encoder,
  Qwen Image VAE, the LLLite inpaint/control models, optionally the turbo LoRA).
- **ComfyUI has no login.** krita-ai-diffusion's managed server listens on `localhost` only. A tablet
  needs it on the LAN, which opens it to anyone on that network (including running workflows and
  writing files). The companion should keep ComfyUI on localhost and put a **small proxy with a
  pairing token** in front of it; the QR code then carries address + token.
- **Where it lives (open):** inside ProCraft desktop (Prefs › Image generation), as a separate small
  app/CLI (works on a headless Linux box), or both built from one shared crate.

---

## 5. Scope sketch (phases)

- **Phase 0 — foundations:** `crates/diffusion` with the ComfyUI client, model discovery, a
  `GenerationRequest` type and graph builder for SDXL/Illustrious + one modern family (Flux 2 Klein or
  Z-Image); a CLI test harness against a real ComfyUI.
- **Phase 1 — MVP:** Generate (text → new layer), Refine (strength), Inpaint with selection
  (fill / add / remove / expand), result history with preview + Apply, styles (bundled defaults +
  user-editable), settings for the server. Desktop + Android.
- **Phase 2:** control layers (scribble, line art, depth, pose, reference image), edit models
  (instruction edits), tiled upscale, job queue UI, per-artwork history saved in the artwork file.
- **Phase 3:** regional prompts, Live painting, custom ComfyUI workflows, optional hosted backend,
  optional managed ComfyUI install on desktop.

---

## 6. krita-ai-diffusion features not in Phase 1

| Feature | Where it lands | Note for Anima |
|---|---|---|
| Control layers (scribble, line art, depth, pose, universal) | Phase 2 | Anima uses LLLite control models; a sketch/line-art layer guiding generation is a natural fit for a painting app — candidate to pull earlier |
| Reference images / style / face (IP-Adapter) | Phase 2+ | No Anima IP-Adapter — n/a for now |
| Edit models (text instructions) | Phase 2+ | Different model family — n/a for Anima |
| Regional prompts | Phase 3 | Anima supports them |
| Live painting | Phase 3 | Anima has a turbo LoRA (8 steps) that makes it viable |
| Upscaling (tiled refine) | Phase 2 | |
| Job queue controls (front / replace / cancel one) | Phase 2 | Phase 1 has generate + cancel |
| Seed control, batch count | **Phase 1** (small) | |
| Prompt tools: tag autocomplete, `<lora:…>` syntax, weights, wildcards, translation | Autocomplete: **candidate for Phase 1** | Anima prompts are booru tags; autocomplete matters a lot on a tablet keyboard |
| Style editor (sampler, steps, CFG, LoRAs, prompt templates) | Phase 1: pick styles; Phase 2: edit them | |
| History saved with the artwork | Phase 2 | |
| Animation batch (one generation per frame) | Phase 3 | ProCraft has Animation Assist — interesting later |
| Custom ComfyUI workflows | Phase 3 | |
| Hosted cloud backend | Not planned yet | |

## 7. Required ComfyUI custom nodes

Stock ComfyUI only has basic nodes (load model, sample, decode, save). krita-ai-diffusion's
workflows also use four community extensions installed on the server:

- **comfyui-tooling-nodes** (Acly): send images to and from the client in memory, without files on
  the server; regions; tiling; and — for Anima — **loading and applying LLLite control models**
  (`ETN_control_load` / `ETN_control_apply`). Anima inpainting goes through an LLLite inpaint model,
  so **Anima inpainting needs this.**
- **comfyui-inpaint-nodes** (Acly): mask grow/shrink/blur, pre-filling the hole, colour matching
  the result to its surroundings. Makes fills blend in.
- **comfyui_controlnet_aux**: preprocessors that turn a picture into line art / depth / pose.
  Phase 2 (control layers).
- **ComfyUI_IPAdapter_plus**: reference images for SD1.5/SDXL. Not needed for Anima.

Because these run on the server and we only talk to them over the network, their licences don't
affect ProCraft's. **Proposal:** require tooling + inpaint nodes from Phase 1 (your ComfyUI already
has them; the companion installs them for everyone else), add controlnet_aux in Phase 2, and check
for each one on connect with a clear message for anything missing.

## 8. Next round of questions

- **Companion:** OK with a CLI first, ProCraft desktop Prefs later (§4)?
- ~~Dev instance~~ — **installed 2026-11-05** (see `dev/README.md`): ComfyUI 0.38.0, PyTorch 2.14.1
  (CUDA 13.0) on the GPU, the three add-ons loaded, reachable from the tablet over `adb reverse`.
  Waiting for the models to be copied in.

## 9. Smaller open points (no rush)

- Networking crate: `ureq` + `tungstenite` (blocking, on a worker thread like ProCraft's existing
  background work) vs pulling in an async runtime. Leaning blocking — ProCraft has no async today.
- Which custom ComfyUI nodes we require. Fewer is friendlier; inpainting quality may need some.
- Where per-artwork AI state (prompt, history, styles used) is saved — inside the artwork package?
- Colour management: ProCraft has P3/CMYK profiles; generation works in sRGB — convert on the way
  in and out.
- Prompt autocomplete / translation / wildcards: krita-ai-diffusion has them; probably Phase 3.
- Safety/privacy messaging: images leave the device to whatever server the user points at.
