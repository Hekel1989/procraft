# ProCraft Diffusion: notes for agents

> `AGENTS.md` is an exact copy of this file. Change both together (`cp CLAUDE.md AGENTS.md`).

This is a fork of [ProCraft](https://github.com/artcraft-android/procraft) (`Hekel1989/procraft`),
branch `main-diffusion` (the fork's default; its `main` is a plain copy of ProCraft's, left as is), adding AI image generation in the spirit of krita-ai-diffusion, in
ProCraft's own design language. It goes upstream later.

## How the user works

- **Process: brainstorm, then a finalised proposal, then a design, and only then code.** Each
  step is discussed and agreed with the user before the next. Phase 1 went through all three
  (`docs/generation/01–03`). Phase 2 and later need the same: today they are only a direction
  (proposal §8, brainstorm's feature table), with no design.
- **Keep it simple**: no overcomplication. Prefer the plain solution that works.
- **The user decides genuine choices.** Offer two or three options with a recommendation; explain
  any question in plain words, with a concrete example. When the answer is obvious, pick it and
  say so.
- **Mobile first**: the Android tablet is the main target, and it must work and look right on
  both Android and desktop, in ProCraft's design language.
- **Commit and push only when asked.** Docs and `dev/` are tracked with the code.
- **When the user asks for the "Opus/DeepSeek workflow"**, use the `deepseek-orchestrate` skill
  (see the end of this file); the user cares about saving Claude usage.
- **Order after Phase 1**: style editing comes first in Phase 2.
- **Several sessions at once**: a session doing new design work writes its own new documents
  (e.g. `docs/generation/04-phase2-brainstorm.md`) and doesn't edit files another session is
  changing; agree with the user before touching `03-design.md` or code.
- **This machine, the tablet and local paths** are in `dev/local.md` (not in git: never put
  personal details in tracked files; the repository is public). The user's own Krita + ComfyUI setup is off limits (see
  Hard rules).

## ProCraft Diffusion: a fork that follows ProCraft

- The app is **ProCraft Diffusion** (decided 2026-11-05): its own Android app ID
  (`io.github.hekel1989.procraftdiffusion`, `.debug` for debug builds) and desktop data folder
  (`~/.local/share/procraft-diffusion`, `%APPDATA%\ProCraft Diffusion`), so it sits beside ProCraft
  with its own artworks. The name lives in `desktop/src/main.rs` (`APP_NAME`, `APP_DIR`),
  `android/app/src/main/res/values/strings.xml`, `SettingsActivity.kt`'s title, `GalleryScreen.kt`'s title and About, and
  `android/app/build.gradle.kts`'s `applicationId`. The Kotlin namespace, the `.procraft` file
  format and the Linux window class stay ProCraft's. For an upstream PR, leave the rename out.
- **Branches**: `main-diffusion` is ours and the default (named `feat/generation` until
  2026-11-05). `main` is a copy of ProCraft's: on GitHub, "Sync fork" is safe there, never on
  `main-diffusion` (its "Discard commits" would throw our work away).
- **Following upstream**: merge ProCraft's `main` (remote `upstream`) into `main-diffusion` often,
  so conflicts stay small. `.github/workflows/upstream-sync.yml` tries it daily at 08:00 London
  time on GitHub and **only checks**: it can't push, because GitHub refuses pushes carrying
  workflow files from a workflow's own token, and the user chose not to keep a personal token as
  a secret. Don't bring back a push or a `sync/upstream` branch. `gh` needs
  `-R Hekel1989/procraft` (it otherwise picks ProCraft's repository).
- **Taking in ProCraft's changes** (when the user asks, e.g. after a passing run):
  1. `git status` clean, on `main-diffusion`; `git fetch upstream`;
     `git rev-list --count HEAD..upstream/main` gives the number of new commits.
  2. `git merge --no-ff --no-edit -m "chore: merge ProCraft's main (N commits)" upstream/main`.
     On a conflict, keep both sides' behaviour; keep our hooks thin (see below). Fix and commit.
  3. All of "Building and checking", plus `cargo clippy --workspace --all-targets -- -D warnings`
     and `cargo test -p procraft-gpu -p procraft-desktop` (ProCraft's own checks).
  4. If ProCraft changed `crates/gpu/src/canvas.rs`, the layer menus, `Chrome.kt` or the
     Generate entry points, try Generate on the tablet (a safe prompt) before pushing.
  5. Push `main-diffusion` only when the user has asked. Never use GitHub's "Sync fork" on it.
- **The user's plain guide** to all this is `docs/fork-guide.md`: keep it in step when the routine
  changes.
- **Keep the hooks into ProCraft's own files thin**: new code goes in our own files (the
  `generation` modules, `crates/diffusion`, `android/app/src/generation`) and connects with a line
  or two. The usual conflict spots: `crates/core/src/lib.rs`, `desktop/src/ui.rs`,
  `android/.../ui/Chrome.kt`, the layer menus, `crates/engine/src/document.rs`'s `Layer`.

## Where things are

- `docs/generation/`: the source of truth. `01-brainstorm.md`, `02-proposal.md` (final),
  `03-design.md` (final, kept up to date as built: change it when behaviour changes),
  `mockups/phase1.html`. The design's Part D has the milestones and the decisions log. Phase 2:
  `04-phase2-brainstorm.md`, then `05-styles-proposal.md` and `06-styles-design.md` (style
  editing, the first part; both final, the design kept up to date as built).
  `names.md` lists what each part is called (in the app, the docs and the code): add a name there
  first.
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

- The folder named in `dev/local.md` (`PROCRAFT_COMFY_ROOT`), on **port 8189**. Start it with `start.sh` (or
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
  or vague prompt: Anima Turbo can produce explicit pictures from an empty one. Add `scenery, no
  humans` to test prompts too: Turbo put a scantily dressed woman into "a cottage garden with
  flowers, sunny day" (2026-11-06). Clear such test results afterwards and don't preview them.
- On the tablet: ask before uninstalling the app (it deletes the artworks) or changing security
  settings. Before tapping inside a popover, take a screenshot to check it's open; a tap that
  misses lands on the canvas and paints, so undo exactly those strokes.

## The tablet

test tablet, 2800 × 2000, landscape (screenshots are 2800 wide; scale 2.5 from the 1120-dp
layout). Run `adb reverse tcp:8189 tcp:8189`, then the server on the tablet is `127.0.0.1:8189`.
Package `io.github.hekel1989.procraftdiffusion.debug` (the old ProCraft debug app beside it holds the user's own artworks). Install with
`adb install -r android/app/build/outputs/apk/debug/*.apk`.

## Anima tuning (in `family/anima.rs`)

- Native size about 1 MP. Pictures are made in **one pass at the target, capped at 4.2 MP** (2048 ×
  2048, krita-ai-diffusion's limit for Anima; the user finds Krita's one-pass pictures better).
  Bigger targets are scaled up the rest. The graphs still support a second pass (denoise **0.3**
  was tuned for it; 0.4 changed details such as eye colour), but the planner doesn't ask for one.
- Selections are one pass at the capped target, with the LLLite inpaint control at strength 1.0.
- Prompts are `masterpiece, best quality, <prompt>`.

## Status (2026-11-05)

- **Done:** M1 (foundations, CLI), M2 (Generate on the tablet and desktop), M3 (generation
  resolution Draft/Normal/Quality, selections Fill/Add/Remove/Expand, Selection-toolbar button),
  M4 (control layers Line art/Scribble with four steps Loose/Balanced/Close/Strict, added from
  the popover's Control part and traced from what's visible with each row's trace button, as in
  Krita; tag autocomplete with the exact tag first, then every tag holding the word, then aliases, as
  Krita matches).
- **M5** (the companion CLI, `companion/`): done on Linux 2026-11-06, a fresh install accepted;
  Windows written but untested.
- **M6** (styles as TOML files with LoRAs, best size, the styles folder; 06-design §C2): built
  2026-11-06, checked on the tablet and the desktop.
- **Next:** M7 (the Style Studio).
- **Results tray** (§A4, built 2026-11-06): the results bar became a tray that tucks into a tab at
  the bottom centre (⌄, a tap on the canvas, Esc) instead of closing; ↻ Again, ··· with "Use the
  seed" and "Clear all results…". While it's up the canvas treats it as an open panel.
- **After Phase 1:** style editing, designed (`06-styles-design.md`): M6 styles as files with
  LoRAs, M7 the Style Studio, M8 trigger words and sharing.
- **Since M3:** results are kept with their artwork in its private `generation/` folder (the
  newest 24; thumbnails in memory, full pictures on disk, read when shown or applied). Holding a
  thumbnail gives a Delete. The server shows as a button in the popover's title. The results tray
  scrolls.
- **Open:** the popover's prompt, style and count aren't remembered across app restarts. The
  user has seen a few more small bugs on both platforms that aren't listed yet.

## Working with DeepSeek (optional)

The user's `deepseek-orchestrate` skill offloads well-specified Rust work to DeepSeek in git
worktrees under `.deepseek/` (excluded from git). Live checks on the tablet and UI judgement stay
with the main agent.
