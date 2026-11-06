# Style editing: proposal

Status: **final** (agreed 2026-11-06). Built from the style-editing part of
`04-phase2-brainstorm.md` (decided 2026-11-06). The design turns it into screens, file
formats and APIs. No product code before the design. Sketch: `mockups/style-editor.html` (B).
Names: `names.md`.

---

## 1. Summary

Phase 2 starts by making styles the user's own. A style (the card picked at the top of the
Generate popover) becomes something you can **create, duplicate, edit and delete**, with
**LoRAs**, in a full-screen **Style Studio** like ProCraft's Brush Studio, with a small **Try it**
preview. Every setting krita-ai-diffusion's style editor offers that Anima uses can be changed.
Styles are **plain TOML files in a folder you can open**, so they can be read, edited by hand,
synced and shared. A LoRA's **trigger words** are kept once, on the server, and added to the
prompt whenever a style uses it, and the app fills them in from the LoRA file where it can.

## 2. Goals

1. **Everything customisable**, as in Krita: model, LoRAs with strengths, prompt template,
   negative prompt, sampler, scheduler, steps, CFG, and the rarer settings.
2. **Your files, in the open.** A style is one readable file in a folder you can see and change,
   on Android too. Nothing hidden in the app's private storage.
3. **See a change without leaving.** Try it makes a quick picture with the style as it stands, at
   the same seed each time, so the change made is the only difference.
4. **LoRAs without the bookkeeping.** Trigger words filled in from the file when it carries them,
   typed once when it doesn't, and shared by every device using that server.
5. **Feels like ProCraft**, the same on the tablet and the desktop: the Studio follows Brush
   Studio, the style cards keep working as they do.
6. **Upstream-ready**, as Phase 1: our own files, thin hooks, the `generation` feature switch.

## 3. Non-goals for now

- Importing Krita's style files (maybe later).
- Looking LoRAs up on Civitai, or downloading LoRAs (later, through the companion).
- Model families other than Anima, and the settings only they use (clip skip, v-prediction, the
  live-painting sampler).
- A description or About section for a style: the name is enough.
- Editing the built-in styles in place (they're duplicated instead).
- The rest of Phase 2 (depth and pose control, upscaling, a job queue, and so on): each gets its
  own brainstorm.

## 4. What the user gets

### 4.1 Styles you can change

- The style cards in the Generate popover show the **built-in styles** (Anima, Anima Turbo) and
  **your styles**.
- **Hold a card**: your styles offer **Edit**, **Duplicate**, **Delete**; built-in ones only
  **Duplicate**, so the defaults can always be gone back to and can improve in later versions.
- Each card has a small **family badge** naming the model family the style is for (Anima today;
  SDXL, Illustrious, Flux and others when they're added), as Krita shows.
- A new style starts as a duplicate, of a built-in one or one of yours.

### 4.2 The Style Studio

A screen of its own, as Brush Studio is for brushes, on the tablet and as a full window on the
desktop. Cancel and Done at the top, sections on the left, the chosen section in the middle, the
preview on the right.

| Section | What's in it |
|---|---|
| **Model** | The name, and the model (checkpoint) from those the server has, with its family badge |
| **LoRAs** | The style's LoRAs, each with its family badge, a strength slider, its trigger words and ×; **+ Add LoRA** lists the LoRAs the server has, those for the style's family first |
| **Prompts** | The prompt template with `{prompt}` where the user's prompt goes, and the negative prompt |
| **Sampling** | Sampler, steps and CFG, shown as they are (a number of steps, a CFG value, the sampler by name) |
| **Advanced** | Scheduler, rescale CFG, VAE, text encoder, and **best size** (Krita's "Preferred Resolution"): an Override switch, off by default, and one side in pixels; on, every picture is made at about that size squared, then scaled to the target. For fine-tunes trained at an unusual size |

- **Preview**: a **preview prompt** the user types (it starts as "a lighthouse on a cliff,
  scenery, no humans" and is remembered) and **Try it**, which makes one picture at **Draft** size.
  The seed stays the same between tries; a **dice** beside Try it picks a new one.
- **A style whose model or LoRAs the server doesn't have** is kept, and the Studio says what's
  missing.

### 4.3 LoRAs and their trigger words

- A LoRA can carry **trigger words** and a **default strength**, kept with the LoRA, not the style.
  Whenever a style uses it, its trigger words go **at the end of the user's prompt**, after a
  comma (as Krita adds them; Anima reads tags).
- **Filled in from the file** when a LoRA is first added to a style: the app reads its metadata
  from the server. In LoRAs trained with kohya's trainer (most of them), the trigger word is the
  tag used on every training picture, so it's pre-filled exactly, with the next most used tags as
  chips to tap. A LoRA with no metadata has its words typed once. The user checks and keeps them.
- **Kept on the server**, in ComfyUI's own user folder, so one entry serves the tablet and the
  desktop and lives beside the LoRAs.

### 4.4 The files

- **One style, one TOML file**, named after it (`Moonlit.toml`): `name = value` lines with `#`
  comments, readable and editable in any text editor.
- **Where they are**:
  - **Android**: **Documents › ProCraft Diffusion › Styles**, seen in the Files app and from a PC,
    kept after an uninstall, usable by sync tools. The app makes the folder; Android's folder
    picker opens inside it once, and the user taps "Use this folder" and "Allow".
  - **Desktop**: the app's data folder, beside `brushes/` (`~/.local/share/procraft-diffusion/styles/`,
    `%APPDATA%\ProCraft Diffusion\styles\`), with an **Open styles folder** button in the Studio.
  - A **Styles folder** setting on both picks another folder.
- **Export and Import** of one style, for sending it to someone: the share sheet and the file
  picker on Android, the file dialogs on the desktop, as brushes are exported today.
- **Trigger words**: one TOML file for all LoRAs on the server,
  `ComfyUI/user/default/procraft-diffusion/loras.toml`, read and written through ComfyUI's
  `userdata` API, so the app never needs the server's disk.

Both formats are written out in `04-phase2-brainstorm.md`.

## 5. How it's built (outline; detail in the design)

- **`crates/diffusion`**: the style file (read, write, check against what the server has); LoRAs
  in the Anima graphs (a LoRA loader per LoRA between the model loader and the sampler); trigger
  words added to the prompt; the server's LoRA list, LoRA metadata (`/view_metadata/loras`) and
  the trigger-words file (`/userdata`); the kohya tag reading for the pre-fill. Tested on its own
  and against the dev ComfyUI, and usable from `diffusion-cli`.
- **`crates/core`**: the style library (built-in plus the folder's files), exposed over UniFFI and
  to the desktop; Try it as a small generation that doesn't touch the canvas or the results tray.
- **Android**: the Style Studio in Compose, in `src/generation`; the styles folder through
  Android's document API (the folder picker, as ProCraft's Save to device uses).
- **Desktop**: the Style Studio in egui, in our own files; plain files in the data folder.
- **Hooks into ProCraft's files** stay as they are (the AI button, the popover); the Studio is
  reached from the popover, which is ours.

## 6. Milestones

Continuing Phase 1's numbering. Each works on the tablet **and** the desktop before the next.

| # | Milestone | Done when |
|---|---|---|
| M6 | **Styles as files, with LoRAs** | Styles in the styles folder (written by hand for now) show as cards, with their family badge, and generate, LoRAs and trigger words included; the Android folder is set up through the picker; `diffusion-cli` generates with a style file |
| M7 | **The Style Studio** | Hold a card → Edit / Duplicate / Delete; every section works; Try it with its seed and dice; missing models shown; Done saves the file, Cancel leaves it as it was |
| M8 | **LoRA trigger words and sharing** | Trigger words pre-filled from the LoRA's metadata, kept on the server and seen by both devices; Export / Import; Open styles folder; the Styles folder setting |

## 7. Risks

| Risk | What we do |
|---|---|
| **Android's document API** is slower and fussier than plain files, and makers change the folder picker | Styles are few and small: read the folder once, cache, write one file at a time. Check the picker on the OnePlus Pad early (M6). If the folder goes away, ask again rather than fail |
| **A style file edited by hand is wrong** (a typo, a missing field) | Read what can be read, fall back to the model's own settings for the rest, and show the style with a note instead of dropping it |
| **A LoRA made for another model** (an SDXL LoRA in the same folder) | Each LoRA shows its family badge, read from its metadata (`ss_base_model_version`: `anima` in the user's LoRAs; the `modelspec.architecture` field was wrong in one of them). One for another family is marked and listed last; one without metadata shows no badge |
| **Trigger-word guessing is wrong** for a LoRA trained another way | It's only a pre-fill: the user sees and keeps or changes it |
| **Two devices edit the same style or `loras.toml`** at once | Last save wins; the files are small and saved whole. Good enough for one person's devices |
| **ProCraft changes Brush Studio** or the popover's neighbours | The Studio is our own code, modelled on Brush Studio, not built into it |

## 8. To settle in the design

- The Studio's screens on the tablet (1120 × 800 dp) and desktop, from mockup B; the hold menu
  on a style card; what Cancel does with unsaved changes.
- The exact style file: every field, which are optional, the model named by file or by pattern (as
  the built-ins are), a format version line; whether the built-in styles move to the same format.
- The family badges' look (our own, not Krita's icons), and how a LoRA's family is read when
  the metadata is missing (its folder name, say `loras/anima/`).
- How best size, when overridden, replaces the planner's sizes (the size range around it, and the
  second pass when the target is bigger).
- How LoRAs go into the Anima graph (model only, or the text encoder too) and their strength range.
- The trigger-words file's exact shape, and what happens when the server can't be written to.
- Where trigger words go relative to the template's own words.
- Try it: its size, whether it waits for or runs beside a canvas generation, where its pictures
  are kept (not in the results tray).
- The Android folder: how it's made, the first-run moment (on first save, or when the Studio
  first opens), and what the user sees if they decline.
- Names for anything new, in `names.md` first.
- Test plan: unit tests for the files and the tag reading, graph tests with a LoRA against the
  dev ComfyUI (safe prompts), on-device checks of the folder picker.
