# Style editing: design

Status: **final (2026-11-06).** Implements `05-styles-proposal.md` (final). Part A is what the
user sees, Part B how it's built, Part C testing and the work breakdown, Part D the points to
agree. Names are in `names.md`; Phase 1's design (`03-design.md`) still holds for everything
not changed here. Sketch: `mockups/style-editor.html` (B).

Checked again after ProCraft's 42-commit merge (`92b319e`, 2026-11-06: its plugin system,
patches such as the interface size, inline renaming in Brush Studio): the hook spots (`AppRoot.kt`
beside `BrushStudio(vm)`, desktop `ui.rs`), Brush Studio's measurements, Android's lowest version
(10) and `toml_edit` in the lock are unchanged, and the design follows the two things that
changed (A2, A3). Plugins are sandboxed and have no network, so generation stays as it is.

Checked on the dev ComfyUI and the user's three Anima LoRAs (2026-11-06):

- All three change only the diffusion model (every key is `lora_unet_…`), none the text encoder.
- ComfyUI lists them through `LoraLoader`'s choices (`anima/…`, with their folder) and gives
  their metadata at `GET /view_metadata/loras?filename=…`.
- Their family is in `ss_base_model_version` (`anima`), and `modelspec.architecture` was wrong
  in one (`stable-diffusion-v1/lora`), so it isn't used.
- `GET /userdata?dir=.` answers; a folder that doesn't exist yet answers 404, which means "no
  file yet".

---

# Part A — Experience

## A1. Style cards in the Generate popover

The style strip stays as it is (cards 116 × 76 dp, scrolling sideways, the chosen one ringed),
with these changes:

- **Order**: your styles first (by name), then the built-in ones (Anima, Anima Turbo), then one
  for each other Anima checkpoint on the server, as today.
- **Family badge**: a small pill at the card's top right, the family's short name ("Anima") in
  10 sp, on a tint of the family's own colour. The badges are drawn by us, text in a pill; no
  logos.
- **The card's second line** stays "30 steps · CFG 4". When something the style needs isn't on
  the server, it reads "Missing LoRA" or "Missing model" in amber instead.
- **A "+" card at the end** of the strip, half as wide, opens a small menu: **New from
  "Anima"** (a copy of the chosen style, opened in the Studio) and **Import a style…**.
- **Hold a card** (desktop: hold or right-click) to get a small menu above it, drawn like the
  result's Delete bubble:
  - your styles: **Edit · Duplicate · Export · Delete**;
  - built-in and found styles: **Duplicate** only.
- **Duplicate** makes "Moonlit copy" and opens the Studio on it. Cancel there throws the copy
  away, as Brush Studio does with a new brush.
- **Delete** asks first: an alert, "Delete "Moonlit"?", "Its file is removed from the styles
  folder.", [Cancel] [Delete].
- **A file that can't be read** (a typo made by hand) still shows a card, with its file name and
  "Can't be read" in amber. Its hold menu has **Show the problem**, which shows the line and
  what's wrong, and **Delete**.

## A2. The Style Studio

A screen of its own, the same size and build as Brush Studio, sliding up over the canvas. The
popover closes behind it. **Done** saves and brings you back with the edited style chosen;
**Cancel** leaves the file as it was, with no question asked, as Brush Studio does.

**Tablet, landscape (1120 × 800 dp)**, Brush Studio's measurements:

```
┌──────────────────┬────────────────────┬────────────────────────────────────┐
│ Style Studio     │ LORAS              │                   [Cancel] (✓)     │
│                  │                    │                                    │
│ ▣ Model          │ [Anima] dark art   │      ┌──────────────────────┐      │
│ ◈ LoRAs      ◀── │ style          ×   │      │                      │      │
│ ✎ Prompts        │ Strength ━━━●── 70%│      │   the last try        │      │
│ ⚙ Sampling       │ Trigger words      │      │                      │      │
│ ⋯ Advanced       │ [dark_art_style  ] │      └──────────────────────┘      │
│                  │                    │      ▢ ▢ ▢ ▢   tries, newest left  │
│                  │ [Anima] dragon ... │                                    │
│                  │ ...                │  [a lighthouse on a cliff, scen…]  │
│                  │                    │  [⚄]  [ Try it ]      Seed 18234   │
│ Open styles      │ [+ Add LoRA]       │                                    │
│ folder (desktop) │                    │                                    │
└──────────────────┴────────────────────┴────────────────────────────────────┘
   pages card 320       settings 323            preview (the rest)
```

- **Portrait**: the pages as icons down the left (Brush Studio's rail), the preview on top
  (510 dp tall), the settings under it.
- **Desktop**: the same, in the full window, as the desktop's Brush Studio. Esc is Cancel.
- **Interface size**: ProCraft's interface-size patch scales every dp. The Studio, measured in
  dp like Brush Studio, scales with it; the preview column takes what's left, so a larger
  interface makes the preview smaller, not the settings.

## A3. The pages

**Model**
- **Name**: the page's heading is the style's name, tapped to change it there, as Brush Studio's
  About page has done since ProCraft's 2026-11 changes (its `InlineName`). It's also the file's
  name.
- **Model**: the checkpoints on the server, as rows, the chosen one ticked. Each row has its
  family badge. A model the server doesn't have stays at the top in amber, "not on this server".
- The family isn't set by hand: it follows the model.

**LoRAs**
- One block per LoRA: its family badge, its name (the file name without folder or extension,
  `_` as spaces) and ×; a **Strength** slider from 0 to 200 % (a typed value can be anything from
  −200 to 200 %, as ProCraft's sliders allow typing); and **Trigger words**, a text field with
  the note "Used by every style with this LoRA".
- **+ Add LoRA** opens a list over the settings column. LoRAs for the style's family come first,
  grouped by folder. Below them "Other families", each with its badge, and "Not known" for those
  whose family can't be told. A search field shows when there are more than 12.
- **When a LoRA is added**: if the server has trigger words for it, they're used, with its
  default strength. If not, the app reads its metadata and fills them in (A5). The field is
  marked "Filled in from the file: check them", with chips of the next most used tags under it
  to add with a tap.

**Prompts**
- **Prompt**: a multi-line field, the template, with the note "{prompt} is where your prompt
  goes". Without `{prompt}`, the note reads "Your prompt goes at the end".
- **Negative prompt**: a multi-line field.
- Both have the tag suggestion strip, as the Generate popover's prompt does.

**Sampling**
- **Sampler**: a row with its name, opening the server's list of samplers.
- **Steps**: a slider from 1 to 100.
- **CFG**: a slider from 1 to 15, in steps of 0.1 (typed values from 0 to 30).

**Advanced**
- **Scheduler**: a row opening the server's list.
- **Rescale CFG**: an Override switch; when on, a slider from 0 to 1 (0.7 when first turned on,
  as Krita).
- **VAE** and **Text encoder**: each a row, "The model's own" unless changed, opening the
  server's list.
- **Best size**: an Override switch; when on, a number from 256 to 2048 in steps of 8 (one side,
  in pixels) (B6).

## A4. Try it

- **The preview prompt** sits under the picture. It's app-wide and remembered, and starts as "a
  lighthouse on a cliff, scenery, no humans".
- **Try it** makes one picture with the style as it is now, at Draft size (the model's own size,
  or the best size when overridden), in the open canvas's shape.
- **The seed** is picked when the Studio opens and stays the same between tries; **⚄** picks a
  new one. The seed shows small beside the button.
- **While it runs** the button shows a progress ring and Cancel. If a run on the canvas is going,
  Try it waits for it ("Waiting for the canvas's run").
- **Tries**: the last four pictures sit as thumbnails under the big one, newest on the left. A
  tap shows one, so a change can be compared with the try before it. They're kept until the
  Studio closes, never in the results tray or with the artwork.

## A5. Trigger words

- **In the prompt**: when a style with LoRAs runs, each LoRA's trigger words go after the user's
  prompt, with a comma, inside the template where `{prompt}` is:
  `masterpiece, best quality, a castle, dark_art_style`. A trigger word already in the prompt (as
  a whole tag) isn't added twice. The popover doesn't show them, as Krita doesn't.
- **Filled in from the file**: from kohya's metadata, the number of training pictures and how
  often each tag was used.
  - Filled in: the tags on at least 95 % of the pictures that aren't ordinary Danbooru general
    tags (the app has the tag list). `1girl` on every picture of a character LoRA isn't a trigger
    word; `dark_art_style` or `@dragonball` is. A known character or copyright tag can be one,
    and is kept.
  - Offered as chips: the next eight most used tags.
  - Tags are compared with spaces and underscores alike (kohya writes `long hair`, Danbooru
    `long_hair`).
  - A LoRA without that metadata gets an empty field.
- **Kept on the server**, when Done is tapped, in `loras.toml` (B5). Cancel keeps nothing.

## A6. The styles folder

**Android**
- **The first time a style is saved** (Done on a new copy, or an Import), the app writes it into
  **Documents › ProCraft Diffusion › Styles**, making the folders. Android lets any app create its
  own files in Documents without a permission (since Android 10, the app's lowest).
- The app then asks once: "Keep your styles in Documents › ProCraft Diffusion › Styles? Android
  will ask you to let ProCraft Diffusion use that folder." [Not now] [Continue].
  - **Continue** opens Android's folder picker inside that folder, where the user taps **Use this
    folder**, then **Allow**. From then on the app sees every file there, including those added by
    hand or by a sync tool.
  - **Not now**: the styles the app wrote are still there and still used, but files added from
    elsewhere aren't seen. It asks again the next time a style is saved.
- **The setting**: the server view gets a **Styles folder** row ("Documents/ProCraft
  Diffusion/Styles", [Change]) that opens the picker anywhere.
- **If the folder has gone** (deleted, or the card it was on removed), the app's own styles stay
  in memory and it asks again on the next save.

**Desktop**
- `~/.local/share/procraft-diffusion/styles/` or `%APPDATA%\ProCraft Diffusion\styles\`, made
  when first needed.
- **Open styles folder** at the bottom of the Studio's pages card opens it in the file manager.
- The server view's **Styles folder** row ([Change] [Open]) picks another.

**Both**
- The folder is read again each time the Generate popover opens. A style edited in a text
  editor or synced from the other device shows up without a restart.
- **Export** (hold menu) shares the style's file: Android's share sheet, a save dialog on the
  desktop.
- **Import** (the + card) takes a `.toml` file: Android's file picker, an open dialog on the
  desktop. It's copied into the folder, with " 2" added to its name if one is already called that.

## A7. States and errors

| Case | What shows |
|---|---|
| Not connected | The Studio opens and saves. The model, LoRA and sampler lists say "Not connected to a server", and Try it is off with the same words |
| The trigger words can't be saved on the server | The style is saved. A line in the server view: "Couldn't save the trigger words on the server: …" |
| A style's LoRA isn't on the server | The card says "Missing LoRA". Generate is off with "The server hasn't got dark art style", and in the Studio the LoRA's block is amber |
| A file can't be read | Its card, "Can't be read" (A1) |
| The folder can't be written | Done keeps the Studio open: "Couldn't save the style: …" |

---

# Part B — Architecture

## B1. The style file

One TOML file per style, named after it (`Moonlit.toml`). The characters that can't go in file
names (`/ \ : * ? " < > |`) become `-`.

```toml
# A ProCraft Diffusion style
name = "Moonlit"
model = "anima-base-v1.0.safetensors"

prompt = "masterpiece, best quality, {prompt}, night, moonlight"
negative = "worst quality, low quality, blurry"

sampler = "euler_ancestral"
scheduler = "normal"
steps = 24
cfg = 4.0

[[lora]]
file = "anima/dark_art_style_(zura_janai).safetensors"
strength = 0.7

# Advanced: only written when overridden
# rescale_cfg = 0.7
# vae = "qwen_image_vae.safetensors"
# text_encoder = "qwen_3_06b_base.safetensors"
# best_size = 1024
```

| Key | Required | When missing |
|---|---|---|
| `name` | yes | the file's name |
| `model` | yes | the card says "Missing model". A `*` in it matches any characters, as the built-in styles use (`anima_base*`) |
| `family` | no | told from the model, as Phase 1 does (`*anima*`) |
| `prompt`, `negative` | no | `{prompt}`, empty |
| `sampler`, `scheduler`, `steps`, `cfg` | no | the family's first built-in style's |
| `[[lora]]` `file`, `strength` | no | no LoRAs; strength 1.0 |
| `rescale_cfg`, `vae`, `text_encoder`, `best_size` | no | off, the model's own |
| `version` | no | 1. Written only when a later version changes something |

- **Kept as written**: the Studio saves through `toml_edit`, which keeps the file's comments,
  order and any keys it doesn't know. A file edited by hand keeps its notes after an edit in the
  Studio.
- **Strengths** are stored as numbers (`0.7`), and shown as percentages (70 %).
- **Built-in styles move to the same format**: `crates/diffusion/styles/anima.toml` and
  `anima-turbo.toml`, compiled in, read by the same code. Their ids stay `anima` and
  `anima-turbo`. `anima.json` goes.
- **Ids**: built-in, their name in the code (`anima`); found, `file:<checkpoint>` (as today);
  yours, `user:<file name>`.

## B2. `crates/diffusion`

- **`styles.rs`**: `Style` gains `loras: Vec<LoraUse { file, strength }>`, `rescale_cfg`, `vae`,
  `text_encoder`, `best_size` (each `Option`), `kind: Kind { BuiltIn, Found, Yours }`,
  `missing: Vec<String>` and `problem: Option<String>`. `read(name, text) -> Style` and
  `write(&Style, Option<&str>) -> String` (the new text, edited into the old where there is one).
  `positive(prompt, triggers)`.
- **`loras.rs`** (new): `LoraNotes` (the server's `loras.toml`, read and written with
  `toml_edit`); `look(metadata: &Value, tags: &Tags) -> LoraLook { family, filled, more }` (A5);
  `family_of(file, metadata)`: `ss_base_model_version`, otherwise the folder's name (`anima/`),
  otherwise not known.
- **`comfy.rs`**:
  - `ServerModels` gains `loras` (`LoraLoader`'s choices), `samplers` and `schedulers`
    (`KSampler`'s).
  - New calls: `lora_metadata(file)` (`GET /view_metadata/loras?filename=`), `user_file(path)`
    (`GET /userdata/{path}`, with 404 meaning none yet) and `put_user_file(path, text)`
    (`POST /userdata/{path}?overwrite=true`, with the path URL-encoded, `/` as `%2F`).
- **`family/anima.rs`**: after `UNETLoader` and `CLIPLoader`, one **`LoraLoader`** per LoRA,
  chained, with the same strength for the model and the text encoder. It handles LoRAs that only
  change the model, like the user's, and those that change both. Then `RescaleCFG` when set, then
  the control layers as today. A style's `vae` and `text_encoder` replace the files
  `requirements.rs` picks. A LoRA the server hasn't got stops the run with `NotReady`, before
  anything is queued.
- **`plan.rs`**: B6.

## B3. `crates/core`

A new module, `generation_styles.rs`, with a few lines in `generation.rs` to hold its state.
Every call that touches the server runs on the worker and is read back by polling, as runs are
(Phase 1 §B3).

```rust
// the styles folder: the apps read and write the files, core reads and writes their text
pub fn set_style_files(&self, files: Vec<StyleFile>);          // { name, text }, on each popover open
pub fn styles(&self) -> Vec<StyleInfo>;                         // + family, kind, missing, problem
pub fn style_edit(&self, id: String) -> Option<StyleEdit>;      // every field, for the Studio
pub fn duplicate_style(&self, id: String) -> Option<StyleEdit>; // "<name> copy"
pub fn style_text(&self, edit: StyleEdit, old: Option<String>) -> String;
pub fn style_file_name(&self, name: String) -> String;

// what the server has, for the Studio's lists
pub fn server_lists(&self) -> ServerLists;                      // checkpoints, loras, samplers, …
pub fn look_up_lora(&self, file: String);                       // metadata → LoraLook, polled
pub fn save_lora_notes(&self, notes: Vec<LoraNote>);            // into loras.toml, polled

// Try it: one picture, not on the canvas or in the results tray
pub fn try_style(&self, edit: StyleEdit, prompt: String, seed: u64, width: u32, height: u32);
pub fn cancel_try(&self);
pub fn studio_state(&self) -> StudioState;                      // try progress and picture, look-ups, saves
```

- **Files on Android** go through Android's document API, which Rust can't use. So the apps do
  the reading and writing: Kotlin lists and reads the folder (a few small files), passes their
  text to core, and writes back what `style_text` returns. The desktop does the same with
  `std::fs`. Core never touches the folder, and both apps share one parser and writer.
- **The `toml_edit` crate** is already in the workspace's lock (through another crate), so it
  adds little.

## B4. The apps

- **Android**, all in `src/generation/`:
  - `StyleStudio.kt`, the Studio, with Brush Studio's measurements and colours repeated: they're
    private to ProCraft's file, and repeating a dozen values keeps us out of it.
  - `StylesFolder.kt`: the MediaStore write the first time, the folder picker with the
    persistent permission, and listing, reading and writing through `DocumentsContract`.
  - The hold menu and the + card in `GenerationUi.kt`.
  - The no-op stub grows by one function.
  - The one new hook is in `AppRoot.kt`: `GenerationUi.StyleStudio(vm)` beside `BrushStudio(vm)`.
- **Desktop**: `generation_studio.rs` (the Studio) and `generation_styles.rs` (the folder, Open
  styles folder through `xdg-open` or `explorer`, with no new crate). The one new hook is in
  `ui.rs`: while the Studio is open, it's drawn instead of the canvas screen, as Brush Studio is.

## B5. The trigger-words file, on the server

`ComfyUI/user/default/procraft-diffusion/loras.toml`, through `userdata`:

```toml
# Trigger words and default strengths for LoRAs, kept by ProCraft Diffusion.
# One entry per LoRA file, as ComfyUI names it. Edit freely.

["anima/dark_art_style_(zura_janai).safetensors"]
trigger = "dark_art_style"
strength = 0.8

["anima/dragon_ball_classic_(_GAi).safetensors"]
trigger = "@dragonball"
```

- **Read** when the app connects and when the Studio opens.
- **Written** on Done: the file is read again first, only the entries changed in the Studio are
  replaced, and it's written whole. Two devices saving at the same moment is the only way to lose
  an edit, and only that one.
- **Strength** here is the default given to the LoRA when it's added to a style. The style keeps
  its own.

## B6. Best size and the planner

- **Off** (the default): nothing changes. The family's own size is used (about 1 MP for Anima)
  in the usual one pass at the target, up to the largest picture.
- **On, at N**: the model's own size becomes about N × N pixels, each side kept within 20 % of N
  (as Krita does). Draft makes that size.
  - When Normal or Quality ask for clearly more (a side over N + 20 %, and the picture more than
    about 10 % bigger each way, which is Krita's rule), the picture is made at about N × N first
    and then **refined in a second pass at the target** (capped at the largest picture), with
    denoise 0.3, the second pass the graphs already have. A model trained at N doesn't compose
    well much above it, which is why the setting exists. Krita applies the same rule to every
    model; with Anima's own range (sides up to 2048) it never triggers below our 4.2 MP cap, which
    is why Phase 1 is one pass.
  - Selections stay one pass, at the style's own size, scaled to the box.
- `plan_for` takes the style's size and a flag for the second pass, instead of the fixed 1 MP.
  The planner's tests cover both.

---

# Part C — Testing and plan

## C1. Testing

- **Unit** (`crates/diffusion`):
  - Reading every key, the defaults, and a file with mistakes in it.
  - Writing that keeps comments and unknown keys.
  - File names.
  - Trigger words from a made-up kohya metadata, with a `1girl` on every picture, a made-up
    trigger word, and a known character tag. Made up so the repository holds none of the user's
    LoRAs' tag lists.
  - Families from metadata and folders.
  - `positive` with trigger words, including one already in the prompt.
  - The LoRA chain in the graph.
  - The planner with best size.
- **Against the dev ComfyUI** (`#[ignore]`):
  - A picture with the dark art style LoRA at 0.8, prompt "a lighthouse on a cliff, scenery, no
    humans". Its results are cleared afterwards.
  - A `userdata` round trip with a test file that's deleted afterwards.
  - The real metadata of the three LoRAs giving `dark_art_style`, `@dragonball` and an empty
    field. Read at test time, not stored.
- **On the tablet**:
  - The first save, the question, and the folder picker opening inside the folder. This is
    checked first in M6, since makers change the picker.
  - The folder in the Files app.
  - A style edited on the desktop and synced, then the popover reopened.
  - The Studio in landscape and portrait, and Try it.
- **Desktop**: the same Studio and Open styles folder. Windows untested, as the companion is.

## C2. Work breakdown

*M6 as built (2026-11-06):*
- **`crates/diffusion`**:
  - `styles.rs` reads TOML through `toml_edit`. The built-in styles are now `styles/anima.toml`
    and `styles/anima-turbo.toml`; their `model` is a list of patterns (`["anima_base*",
    "anima-base*"]`), which hand-written files may use too.
  - A mistake reads as one line ("Line 1, column 13: invalid basic string, expected `"`").
  - `loras.rs` reads the server's `loras.toml` when the app connects.
  - The Anima graph chains a `LoraLoader` per LoRA, then `RescaleCFG`, and uses a style's VAE or
    text encoder when it names one.
  - `plan_at` with `ModelSize::best(n)` gives best size its range and Krita's second pass.
  - `diffusion-cli generate --style Moonlit.toml` runs a style file.
- **Checked on the dev ComfyUI**: the dark art style LoRA at 0.8 attached its 448 patches, and
  best size 1024 on a 1536 × 1024 Quality picture made 1232 × 832, then 1536 × 1024.
- **Core**: `generation_styles.rs` with `set_style_files` and `StyleInfo` (`family`, `kind`,
  `file`, `note`, `reason`). `size_for` takes the style. A run with a style that can't run stops
  with its reason.
- **Desktop**: `generation_styles.rs`. The folder is read every 2 s while the popover is open.
  The strip now scrolls sideways (it showed three cards before), with badges and notes.
- **Android**: `StylesFolder.kt`. The server view's **Styles folder** row (Choose / Change)
  arrived here, ahead of M8: without a save in M6 there was nothing else to start the picker
  from.
  - Android can only make a folder with a file in it, so the app writes **"About these
    files.txt"**: two lines on what the files are and that any text editor opens them.
  - Checked on the OnePlus Pad:
    - the picker opens inside Documents › ProCraft Diffusion › Styles;
    - the grant survives a reinstall;
    - styles copied in by `adb` show on the next open, yours first, with "Missing LoRA" and
      "Can't be read";
    - a Best size style's plan reads "2 passes".

| Milestone | Work |
|---|---|
| **M6 Styles as files, with LoRAs** | `styles.rs` on TOML (built-ins moved); `loras.rs` reading `loras.toml`; LoRAs, rescale CFG, VAE and text encoder in the Anima graph; trigger words in the prompt; best size in the planner; core's `set_style_files` and the new `StyleInfo`; both apps reading the folder, the card order, badges and missing notes; Android's first save, question and picker (tested with a style written by hand); `diffusion-cli --style <file>` |
| **M7 The Style Studio** | The hold menu and + card (Edit, Duplicate, Delete, New from); the Studio on both apps, with its five pages and the server's lists; `style_text` saving with comments kept; Try it with its seed, dice and tries; the states of A7 |
| **M8 Trigger words and sharing** | `look_up_lora` with the metadata and the tag list, the pre-fill and chips; `save_lora_notes`; Export and Import; Open styles folder; the Styles folder setting on both apps |

---

# Part D — Agreed choices

The smaller questions, all **agreed with the user (2026-11-06)**:

1. **Card order**: your styles first, then the built-in ones, then found checkpoints (A1).
2. **A "+" card** at the end of the strip, for New and Import (A1). Holding a card is easy to
   miss, and Import needs a home.
3. **Cancel without asking**, as Brush Studio does (A2).
4. **Tries**: the last four pictures under the big one, to compare a change with the try before
   it (A4).
5. **The 95 % rule, minus ordinary tags**, for filling in trigger words (A5).
6. **The Android question on first save** rather than when the Studio first opens (A6): nobody
   is asked about a folder before they have something to keep in it.
7. **Comments kept** in files edited by hand (`toml_edit`, B1).
8. **A second pass** when best size is overridden and the target is bigger (B6), as Krita does,
   while Anima without it stays one pass.
