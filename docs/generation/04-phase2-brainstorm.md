# Phase 2: brainstorm

*Started 2026-11-06. Phase 2's first part is **style editing** (agreed order after Phase 1). The
rest of Phase 2 (proposal §8, the brainstorm's feature table: depth and pose control, upscaling,
and so on) gets its own section here when we come to it. Brainstorm first, then a proposal, then a
design, then code. Sketches: `mockups/style-editor.html`. Names: `names.md`.*

## Style editing

### Where it starts

A style is a preset the user picks from the cards at the top of the Generate popover: a name, a
model (checkpoint), a prompt template (`masterpiece, best quality, {prompt}`), a negative prompt, a
sampler, a scheduler, steps and CFG (`crates/diffusion/styles/anima.json`). Phase 1 ships two,
Anima and Anima Turbo, and lists one more for each other Anima model the server has. None can be
changed in the app, and there are no LoRAs.

What makes krita-ai-diffusion's styles worth having (the user's words: "this is what makes krita
ai diffusion great") is that everything can be customised, LoRAs above all. Its styles are edited
in a window of their own and kept as files.

### Decided (2026-11-06)

| Question | Answer |
|---|---|
| What can be edited | **Everything Krita's style editor has that Anima uses**: name, model, **LoRAs (each with a strength)**, prompt template with `{prompt}`, negative prompt, sampler, scheduler, steps, CFG, rescale CFG, VAE, text encoder, best size. Left out: clip skip, v-prediction and the live-painting sampler, which Anima doesn't use. (A seed was listed here at first, by mistake: Krita's styles have none, and the seed stays in the popover) |
| Where it's edited | **B, a full-screen Style Studio**, as Brush Studio is for brushes: someone working on a style doesn't need the canvas, and Krita edits styles in a window of its own. (A, a page inside the popover, was quicker but a long scrolling column with no room for a preview) |
| How you get there | **Hold a style card** in the Generate popover: your styles offer Edit, Duplicate, Delete; built-in ones only Duplicate |
| Built-in styles | **Read-only**; Duplicate makes an editable copy, so the defaults can always be gone back to and can improve in later versions |
| Simple or full settings | **The real settings, shown as they are**: steps is a number of steps, CFG a number, the sampler by name. ("Detail: Fast · Balanced · Fine", a plain-words stand-in for steps, was dropped: with a full Studio there's room for the real thing.) The rarer ones (scheduler, rescale CFG, VAE, text encoder, best size) sit in an **Advanced** section |
| Where styles live | **App-wide** (every artwork sees them), kept as small files, so they can be copied between the tablet and desktop |
| Krita's style files | **Import later, maybe** (reading its style files is reading data, not copying code) |
| The preview | **The user types their own preview prompt** in the Studio (it starts with a safe scene, "a lighthouse on a cliff, scenery, no humans", and is remembered) |
| A style's description | **None: the name is enough**, as in Krita; the people who make a style know what it's for. No About section |
| LoRA trigger words | **As Krita**: a LoRA can carry its own **trigger words** (and a default strength), kept with the LoRA, not the style. Whenever a style uses that LoRA, its trigger words are added **at the end of the user's prompt** (where Krita adds them; we put a comma before them, since Anima reads tags). Set in the Studio, on the LoRA's row |
| Where trigger words are kept | **On the server**, in ComfyUI's own storage for small files (its `userdata`), not in the app: one entry serves the tablet and the desktop and lives beside the LoRAs. (Krita keeps them on the one computer, in its own settings) |
| Filling them in | **Automatically where the file allows**: when a LoRA is added to a style, the app reads the file's metadata from the server (ComfyUI's `view_metadata`). A trigger phrase, if there is one, is filled in; otherwise the training tags it lists (kohya's trainer writes them, with how often each was used) show as a few chips to tap. The user checks and keeps them. A LoRA with no metadata (merges, some trainers) has them typed once. Asked only when a LoRA is first used, never for the whole folder up front |
| Filling many at once | **Later**: the companion, which runs beside the files, could look each LoRA up on Civitai by its file's fingerprint and fill in their trigger words in one go (needs the internet and Civitai's public lookup) |
| Sharing a style | **Export and Import of one style file, now if it's easy.** It looks easy: a style is one small file, and both apps already export and import brushes, so the same paths serve (Android's share sheet and file picker, the desktop's dialogs). If the design finds otherwise, it moves later |
| Desktop | **The same Studio**, full-window, as the desktop's Brush Studio |
| Best size | **Kept as Krita has it** (its "Preferred Resolution"), in Advanced: an **Override** switch, off by default, and one side in pixels (up to 2048). Off: the planner's own sizes for the family (for Anima, one pass up to 4.2 MP). On, say 1024: every picture is made at about 1024 × 1024 worth of pixels, then scaled up (with a second pass) or down to the target. For fine-tunes trained at another size than their family's; rarely needed for Anima, needed when more families come (which they will) |
| Family badge | **As Krita**: a small badge on each style card names the model family it's for (Anima today; SDXL, Illustrious, Flux and others when they come), and the same badge on each LoRA, so a LoRA for another family stands out. Our own badges, not Krita's icons |
| The file format | **TOML**: plain `name = value` lines a person can read and edit, with comments; one file per style, named after it (below) |
| Where the files are | **Where a person can see them** (an open-source app hides nothing). Android: **Documents › ProCraft Diffusion › Styles**, made by the app and confirmed once in Android's folder picker. Desktop: the app's own data folder, beside `brushes/`, with an **Open styles folder** button in the Studio. Both can be changed in a "Styles folder" setting (below) |

### The Style Studio, as sketched

- A screen of its own: Cancel and Done at the top, **sections on the left**, the chosen section in
  the middle, and a **preview** on the right.
- **LoRAs**: the style's LoRAs, each with a strength slider and ×; "+ Add LoRA"; and the LoRAs the
  server has, to add with a tap.
- **Preview**: the user's own preview prompt and **Try it**, which makes one small picture with
  the style as it is now, so a change (a LoRA's strength, say) can be seen without leaving.
- **Sections**: Model, LoRAs (with each LoRA's trigger words), Prompts, Sampling (sampler, steps,
  CFG), Advanced. No About.
- **Try it** makes the picture at **Draft** size. The seed stays the same between tries, so the
  change made is the only difference, with a **dice** beside Try it for a new seed (when a LoRA
  spoils the picture at that seed). Real testing happens on the canvas; Try it is a quick look.
- **An imported style whose model or LoRAs aren't on the server is kept**, and the Studio says what's
  missing.

### What real Anima LoRAs carry (checked 2026-11-06)

Three of the user's LoRAs, read through ComfyUI's `view_metadata`:

| LoRA | Metadata | The tag on every training picture |
|---|---|---|
| dark_art_style | kohya's trainer, 99 pictures | `dark_art_style` (99 of 99) |
| dragon_ball_classic | kohya's trainer, 285 pictures | `@dragonball` (285 of 285) |
| fire_emblem_3h | none | — |

None has a separate trigger-phrase field. But kohya's metadata says how many training pictures
there were (`ss_dataset_dirs`) and how often each tag was used (`ss_tag_frequency`), and **the
trigger word is the tag used on every picture**; ordinary tags (`1girl`, `solo`) are on fewer.
So the suggestion is exact where kohya's metadata is there: the tags on (nearly) every picture,
pre-filled, with the next most used as chips. A LoRA with no metadata gets its words typed once
(or, later, from Civitai through the companion).

### The files, readable by a person (decided 2026-11-06)

**Format: TOML**, a plain text file of `name = value` lines, which a person can read and edit in
any text editor, with `#` comments allowed. (JSON, which Krita uses, needs quotes and braces
everywhere and allows no comments.) A style:

```toml
# A ProCraft Diffusion style
name = "Moonlit"
model = "anima-base-v1.0.safetensors"

prompt = "masterpiece, best quality, {prompt}, night, moonlight"
negative = "worst quality, low quality, blurry"

sampler = "euler_ancestral"
steps = 24
cfg = 4.0

[[lora]]
file = "anima/dark_art_style_(zura_janai).safetensors"
strength = 0.7

[[lora]]
file = "anima/dragon_ball_classic_(_GAi).safetensors"
strength = 0.5

# Advanced, only when changed from the model's own
# scheduler = "normal"
# vae = "qwen_image_vae.safetensors"
```

One style, one file, named after it: **`Moonlit.toml`**. Export gives the same file; Import takes
one.

**Where the styles are kept**, in a folder a person can open:

| Platform | Folder |
|---|---|
| Android | **`Documents/ProCraft Diffusion/Styles`**, in the tablet's shared storage: seen in the Files app and from a PC over USB, kept after an uninstall, and usable by sync tools. On first use the app makes the folder and opens Android's folder picker inside it; the user taps "Use this folder" and "Allow" once (Android always asks; no manifest permission is needed, the same picker as ProCraft's Save to device). If the folder is later removed or moved, the app asks again. *To check on the tablet: that the picker opens inside the folder on the OnePlus Pad* |
| Linux | `~/.local/share/procraft-diffusion/styles/` (next to `brushes/`, as ProCraft keeps them) |
| Windows | `%APPDATA%\ProCraft Diffusion\styles\` (the same, as the app keeps `brushes\` there) |

On the desktop the folder is hidden by default, so the Studio has an **Open styles folder** button
that opens it in the file manager. A **Styles folder** setting on both platforms picks another
folder (one a sync tool watches, say). Export and Import of one style stay, for sending a style
to someone.

**The trigger words**, on the server: one file for all LoRAs, in ComfyUI's own user folder,
**`ComfyUI/user/default/procraft-diffusion/loras.toml`**, written and read through ComfyUI's
`userdata` API, so the app never needs the server's disk:

```toml
# Trigger words and default strengths for LoRAs, kept by ProCraft Diffusion

["anima/dark_art_style_(zura_janai).safetensors"]
trigger = "dark_art_style"
strength = 0.8

["anima/dragon_ball_classic_(_GAi).safetensors"]
trigger = "@dragonball"
```

### Next

The brainstorm for style editing is settled. Its proposal, `05-styles-proposal.md`, is final;
next is the design.
