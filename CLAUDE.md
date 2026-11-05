# ProCraft Diffusion: notes for agents

> `AGENTS.md` is an exact copy of this file. Change both together (`cp CLAUDE.md AGENTS.md`).

This is a fork of [ProCraft](https://github.com/artcraft-android/procraft) (`Hekel1989/procraft`),
branch `feat/generation`, adding AI image generation in the spirit of krita-ai-diffusion, in
ProCraft's own design language. It goes upstream later.

## Where things are

- `docs/generation/`: the source of truth. `01-brainstorm.md`, `02-proposal.md` (final),
  `03-design.md` (final, kept up to date as built: change it when behaviour changes),
  `mockups/phase1.html`. Part D has the milestones and the decisions log.
- `crates/diffusion/`: the ComfyUI client and everything model-specific, with no GPU and no UI.
  `comfy.rs` (HTTP + WebSocket), `worker.rs` (runs requests on its own thread), `plan.rs`
  (resolution planner, context box), `family/anima.rs` (Anima graphs and tuned constants),
  `styles.rs` + `styles/anima.json`, `requirements.rs`, and the `diffusion-cli` test harness
  (`src/bin/diffusion_cli.rs`).
- `crates/core/src/generation.rs`: the `Generation` object exported over UniFFI to Kotlin and used
  directly by desktop, behind the `generation` Cargo feature.
- `crates/gpu/src/canvas.rs`: the renderer. Generation only adds an overlay (`set_overlay`), drawn
  in the display pass over every layer.
- `desktop/src/generation.rs`: the egui popover and results bar. On by default (`default =
  ["generation"]`).
- `android/app/src/generation/java/.../generation/GenerationUi.kt`: the Compose UI, with a no-op
  stub in `src/noGeneration/`. The Gradle property `procraft.generation` (default true) switches
  it, along with the INTERNET permission and cleartext traffic.
- `dev/`: scripts for the dev ComfyUI, and notes on the tablet (`dev/README.md`).

## Building and checking

```bash
cargo fmt --all -- --check
cargo clippy -p procraft-diffusion -p procraft-core --features procraft-core/generation --all-targets -- -D warnings
cargo clippy -p procraft-desktop --all-targets -- -D warnings
cargo test -p procraft-diffusion
cargo test -p procraft-core --features generation --lib
cargo run --release -p procraft-desktop          # from this folder; G opens Generate
cd android && ./gradlew assembleDebug -Pprocraft.abis=arm64-v8a   # arm64 only: x86_64 isn't installed
```

Conventions: workspace lints deny `unwrap`, `expect`, `panic` and indexing outside tests
(integration tests opt out with `#![allow(...)]`). rustfmt max width is 120. Doc comments are plain
prose. Commits are semantic (`feat(…)`, `fix(…)`, `docs(…)`, `tune(…)`) and end with a
`Co-Authored-By` line. Commit and push only when the user asks.

## The dev ComfyUI

- `<dev ComfyUI folder>/`, on **port 8189**. Start it with `start.sh` (or
  `nohup …/start.sh > …/comfyui.log 2>&1 &`) and stop it with `stop.sh`. The Data drive must be
  mounted. A stale `comfyui.pid` can be removed if its process is gone.
- Models are copied in by hand by the user: `anima_baseV10`, `anima_turboV10`, `qwen_3_06b_base`,
  `qwen_image_vae`, `anima-lllite-inpainting-v2`, `anima-lllite-any-test-like-v2`.
- `PROCRAFT_COMFY_URL` points the `#[ignore]` server tests at it (default `127.0.0.1:8189`).

## Hard rules

- **Never touch the user's Krita ComfyUI** at `~/.local/share/krita/ai_diffusion/server`: no
  edits, restarts or jobs, and the dev instance must not read or link its models folder.
- **Never copy or translate krita-ai-diffusion code** (GPL-3.0). Its tag CSVs (Danbooru, e621)
  may be bundled as they are.
- **Anima's weights are non-commercial** (CircleStone Labs licence). Never host or ship them;
  outputs are free to use.
- **No iPad**, ever: the platforms are Android tablets and desktop (Windows, Linux).
- **The NSFW switch only decides whether NSFW tags are suggested** by autocomplete. It must never
  change what is generated: no rating tags and no negative-prompt filtering. What people
  generate is up to them.
- **Test generations use explicitly safe subjects** (landscapes, objects, animals), never an empty
  or vague prompt: Anima Turbo can produce explicit pictures from an empty one.
- On the tablet: ask before uninstalling the app (it deletes the artworks) or changing security
  settings. Before tapping inside a popover, take a screenshot to check it's open; a tap that
  misses lands on the canvas and paints, so undo exactly those strokes.

## The tablet

test tablet, 2800 × 2000, landscape (screenshots are 2800 wide; scale 2.5 from the 1120-dp
layout). Run `adb reverse tcp:8189 tcp:8189`, then the server on the tablet is `127.0.0.1:8189`.
Package `io.github.artcraftandroid.procraft.debug`. Install with
`adb install -r android/app/build/outputs/apk/debug/*.apk`.

## Anima tuning (in `family/anima.rs`)

- Native size about 1 MP; passes are made at **3 MP at most** (5.6 MP went soft and showed a
  grid).
- Two passes when the target is more than 1.5× the native pixels; the second at denoise **0.3**
  (0.4 changed details such as eye colour).
- Selections are one pass at the capped target, with the LLLite inpaint control at strength 1.0.
- Prompts are `masterpiece, best quality, <prompt>`.

## Status (2026-11-05)

- **Done:** M1 (foundations, CLI), M2 (Generate on the tablet and desktop), M3 (generation
  resolution Draft/Normal/Quality, selections Fill/Add/Remove/Expand, Selection-toolbar button).
- **Next:** M4 (control layers, tag autocomplete), M5 (the companion CLI).
- **After Phase 1:** style editing.
- **Since M3:** results are kept with their artwork in its private `generation/` folder (the
  newest 24; thumbnails in memory, full pictures on disk, read when shown or applied). Holding a
  thumbnail gives a Delete. The server shows as a button in the popover's title. The results bar
  scrolls.
- **Open:** the popover's prompt, style and count aren't remembered across app restarts. The
  user has seen a few more small bugs on both platforms that aren't listed yet.

## Working with DeepSeek (optional)

The user's `deepseek-orchestrate` skill offloads well-specified Rust work to DeepSeek in git
worktrees under `.deepseek/` (excluded from git). Live checks on the tablet and UI judgement stay
with the main agent.
