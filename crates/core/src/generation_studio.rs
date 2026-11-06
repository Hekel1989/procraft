//! The Style Studio's side of generation: a style's settings to edit, its file's text written back
//! (with what was written by hand kept), the server's lists to choose from, and Try it, one
//! picture made with the settings as they are, apart from the canvas and its results tray.

use std::sync::Arc;

use procraft_diffusion::loras::{NOTES_PATH, write_notes};
use procraft_diffusion::style_edit;
use procraft_diffusion::{Family, LoraNotes, Request, Resolution, RunId, StyleKind, StyleSettings};

use crate::ThumbnailImage;
use crate::generation::{Generation, GenerationError, random_seed};

/// The biggest side a Try it picture is kept at for showing.
const PICTURE_SIDE: u32 = 1024;

/// A LoRA in a style: its file as the server lists it, its strength (1 is as trained), and its
/// trigger words, kept on the server for every style that uses it (not in the style's file).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LoraSetting {
    pub file: String,
    pub strength: f32,
    pub trigger: String,
}

/// Every setting of a style, as the Style Studio edits it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StyleEdit {
    pub name: String,
    /// The model family's short name ("Anima").
    pub family: String,
    /// The checkpoint as written: a file, or patterns best first (the built-in styles').
    pub models: Vec<String>,
    /// The prompt template, `{prompt}` where the person's prompt goes.
    pub template: String,
    pub negative: String,
    pub sampler: String,
    pub scheduler: String,
    pub steps: u32,
    pub cfg: f32,
    pub loras: Vec<LoraSetting>,
    pub rescale_cfg: Option<f32>,
    pub vae: Option<String>,
    pub text_encoder: Option<String>,
    pub best_size: Option<u32>,
}

/// What the server offers the Studio's lists.
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct ServerLists {
    /// The checkpoints of the families the app knows.
    pub checkpoints: Vec<String>,
    pub loras: Vec<String>,
    pub samplers: Vec<String>,
    pub schedulers: Vec<String>,
    pub vaes: Vec<String>,
    pub text_encoders: Vec<String>,
}

/// A Try it picture: its number (a new one each time), the picture (premultiplied, at most 1024
/// a side) and its seed.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StudioPicture {
    pub id: u64,
    pub picture: ThumbnailImage,
    pub seed: u64,
}

/// Try it's state: whether it runs, waits behind a run on the canvas, how far it is, the last
/// picture and why the last try failed.
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct StudioState {
    pub running: bool,
    pub waiting: bool,
    pub progress: f32,
    pub picture: Option<StudioPicture>,
    pub error: Option<String>,
}

/// The Studio's run, as core keeps it.
#[derive(Default)]
pub(crate) struct StudioRun {
    run: Option<(RunId, u64)>,
    picture: Option<StudioPicture>,
    error: Option<String>,
    pictures: u64,
    notes_error: Option<String>,
}

impl From<StyleSettings> for StyleEdit {
    fn from(s: StyleSettings) -> StyleEdit {
        StyleEdit {
            name: s.name.unwrap_or_default(),
            family: s.family.name().to_owned(),
            models: s.models,
            template: s.template,
            negative: s.negative,
            sampler: s.sampler.unwrap_or_default(),
            scheduler: s.scheduler.unwrap_or_default(),
            steps: s.steps.unwrap_or(30),
            cfg: s.cfg.unwrap_or(4.0),
            loras: s
                .loras
                .into_iter()
                .map(|(file, strength)| LoraSetting { file, strength, trigger: String::new() })
                .collect(),
            rescale_cfg: s.rescale_cfg,
            vae: s.vae,
            text_encoder: s.text_encoder,
            best_size: s.best_size,
        }
    }
}

impl From<StyleEdit> for StyleSettings {
    fn from(e: StyleEdit) -> StyleSettings {
        let family =
            Family::ALL.into_iter().find(|f| f.name().eq_ignore_ascii_case(&e.family)).unwrap_or(Family::Anima);
        let some = |s: String| (!s.trim().is_empty()).then_some(s);
        StyleSettings {
            name: some(e.name),
            family,
            models: e.models,
            template: e.template,
            negative: e.negative,
            sampler: some(e.sampler),
            scheduler: some(e.scheduler),
            steps: Some(e.steps.clamp(1, 200)),
            cfg: Some(e.cfg),
            loras: e.loras.into_iter().map(|l| (l.file, l.strength)).collect(),
            rescale_cfg: e.rescale_cfg,
            vae: e.vae.filter(|v| !v.trim().is_empty()),
            text_encoder: e.text_encoder.filter(|t| !t.trim().is_empty()),
            best_size: e.best_size,
        }
    }
}

impl Generation {
    /// `edit` with each LoRA named as the server lists it (`anima/x.safetensors` for a file
    /// written as `x.safetensors`), as a run finds it, so the Studio sees it's there, and with the
    /// trigger words the server keeps for it.
    fn on_server(&self, mut edit: StyleEdit) -> StyleEdit {
        let server = self.worker.server();
        if let Some(models) = &server.models {
            for lora in &mut edit.loras {
                if let Some(file) = procraft_diffusion::styles::find(&lora.file, &models.loras) {
                    lora.file = file;
                }
            }
        }
        for lora in &mut edit.loras {
            lora.trigger = server.lora_notes.get(&lora.file).map(|n| n.trigger.clone()).unwrap_or_default();
        }
        edit
    }
}

#[uniffi::export]
impl Generation {
    /// Style `id`'s settings to edit, every gap filled from its family's own; none if there's no
    /// such style or its file can't be read.
    pub fn style_edit(&self, id: String) -> Option<StyleEdit> {
        style_edit::settings_for(&id, &self.style_files.lock()).map(|s| self.on_server(s.into()))
    }

    /// A new style to start from: "New style", with the first built-in style's settings.
    pub fn new_style(&self) -> Option<StyleEdit> {
        style_edit::fresh().map(|s| self.on_server(s.into()))
    }

    /// Keeps on the server the trigger words of `loras` that changed (in its `loras.toml`, for every
    /// device and style using them), on a thread of its own; runs use them at once.
    pub fn save_lora_notes(self: Arc<Self>, loras: Vec<LoraSetting>) {
        let server = self.worker.server();
        let Some(address) = server.address.clone() else { return };
        let changes: Vec<(String, String)> = loras
            .into_iter()
            .filter(|l| {
                server.lora_notes.get(&l.file).map(|n| n.trigger.as_str()).unwrap_or_default() != l.trigger.trim()
            })
            .map(|l| (l.file, l.trigger.trim().to_owned()))
            .collect();
        if changes.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            let api = procraft_diffusion::Comfy::new(address);
            let saved = api.user_file(NOTES_PATH).and_then(|old| {
                let Some(text) = write_notes(old.as_deref(), &changes) else {
                    return Err(procraft_diffusion::Error::Server {
                        message: "its loras.toml can't be read, so it's left as it is".to_owned(),
                    });
                };
                api.put_user_file(NOTES_PATH, &text)?;
                Ok(text)
            });
            match saved {
                Ok(text) => self.worker.set_lora_notes(LoraNotes::read(&text)),
                Err(e) => {
                    log::warn!("the trigger words couldn't be saved on the server: {e}");
                    self.studio.lock().notes_error =
                        Some(format!("Couldn't save the trigger words on the server: {e}"));
                }
            }
        });
    }

    /// The trigger words the server keeps for LoRA `file` (empty when none), for one just added.
    pub fn lora_trigger(&self, file: String) -> String {
        self.worker.server().lora_notes.get(&file).map(|n| n.trigger.clone()).unwrap_or_default()
    }

    /// Why the last trigger words couldn't be saved, once (the popover shows it).
    pub fn take_notes_error(&self) -> Option<String> {
        self.studio.lock().notes_error.take()
    }

    /// A copy of style `id`'s settings, named "<name> copy", to start a new style from.
    pub fn duplicate_style(&self, id: String) -> Option<StyleEdit> {
        style_edit::settings_for(&id, &self.style_files.lock())
            .map(|s| self.on_server(style_edit::duplicate(&s).into()))
    }

    /// The file's text for `edit`: `old` (the file as it is, when editing one) changed in place,
    /// so comments and keys written by hand stay; else a new file.
    pub fn style_text(&self, edit: StyleEdit, old: Option<String>) -> String {
        style_edit::write(&edit.into(), old.as_deref())
    }

    /// The file a style named `name` is kept in (`Moonlit.toml`), not taken by another of the
    /// folder's files (`except`: the style's own file, when renaming it).
    pub fn style_file_name(&self, name: String, except: Option<String>) -> String {
        let taken: Vec<String> =
            self.style_files.lock().iter().map(|f| f.name.clone()).filter(|f| Some(f) != except.as_ref()).collect();
        style_edit::file_name(&name, &taken)
    }

    /// The checkpoint the server has for `models` (a file, or patterns best first); none if it has
    /// none of them.
    pub fn checkpoint_for(&self, models: Vec<String>) -> Option<String> {
        let server = self.worker.server();
        let have = &server.models.as_ref()?.diffusion_models;
        models.iter().find_map(|m| procraft_diffusion::styles::find(m, have))
    }

    /// What the server offers the Studio's lists (empty when not connected).
    pub fn server_lists(&self) -> ServerLists {
        let Some(m) = self.worker.server().models else { return ServerLists::default() };
        ServerLists {
            checkpoints: m
                .diffusion_models
                .iter()
                .filter(|f| Family::ALL.iter().any(|fam| fam.recognises(f)))
                .cloned()
                .collect(),
            loras: m.loras,
            samplers: m.samplers,
            schedulers: m.schedulers,
            vaes: m.vaes,
            text_encoders: m.text_encoders,
        }
    }

    /// Try it: one picture of `prompt` with `edit` as it is, `width × height` in shape at the
    /// style's own size (Draft), with `seed`. It waits behind a run on the canvas; a try already
    /// going is cancelled. Its picture comes into [`studio_state`](Self::studio_state), never into
    /// the canvas or its results.
    pub fn try_style(
        &self,
        edit: StyleEdit,
        prompt: String,
        seed: u64,
        width: u32,
        height: u32,
    ) -> Result<(), GenerationError> {
        let server = self.worker.server();
        let Some(models) = server.models.clone() else {
            return Err(GenerationError::NotReady { reason: "not connected to a server".to_owned() });
        };
        let settings: StyleSettings = edit.into();
        let style =
            settings.style("try".to_owned(), String::new(), StyleKind::Yours, None, &models, &server.lora_notes);
        if let Some(reason) = crate::generation_styles::StyleInfo::from(&style).reason {
            return Err(GenerationError::NotReady { reason });
        }
        self.cancel_try();
        let request = Request {
            prompt,
            style,
            strength: 1.0,
            seed: if seed == 0 { random_seed() } else { seed },
            count: 1,
            picture: None,
            selection: None,
            controls: Vec::new(),
            size: (width.max(64), height.max(64)),
            resolution: Resolution::Draft,
            largest: Some(self.largest_megapixels(&server)),
            nsfw: self.settings.lock().nsfw,
        };
        let seed = request.seed;
        let id = self.worker.submit(request);
        let mut studio = self.studio.lock();
        studio.run = Some((id, seed));
        studio.error = None;
        Ok(())
    }

    /// Stops a try that's going.
    pub fn cancel_try(&self) {
        if let Some((id, _)) = self.studio.lock().run.take() {
            self.worker.cancel(id);
            self.worker.forget(id);
        }
    }

    /// Try it as it is now; the app polls it while the Studio is open.
    pub fn studio_state(&self) -> StudioState {
        let canvas_running = self.canvas_running();
        let mut studio = self.studio.lock();
        let Some((id, seed)) = studio.run else {
            return StudioState {
                picture: studio.picture.clone(),
                error: studio.error.clone(),
                ..StudioState::default()
            };
        };
        let Some(run) = self.worker.run(id) else {
            studio.run = None;
            return StudioState {
                picture: studio.picture.clone(),
                error: studio.error.clone(),
                ..StudioState::default()
            };
        };
        if run.finished {
            studio.run = None;
            self.worker.forget(id);
            match (run.results.first(), run.error) {
                (Some(result), _) => {
                    let shown = result.picture.thumbnail(PICTURE_SIDE).ok().map(|small| {
                        let mut rgba = small.rgba;
                        crate::generation::premultiply(&mut rgba);
                        ThumbnailImage { width: small.width, height: small.height, rgba }
                    });
                    if let Some(picture) = shown {
                        studio.pictures += 1;
                        studio.picture = Some(StudioPicture { id: studio.pictures, picture, seed });
                    }
                }
                (None, Some(e)) => studio.error = Some(e.to_string()),
                (None, None) => {}
            }
            return StudioState {
                picture: studio.picture.clone(),
                error: studio.error.clone(),
                ..StudioState::default()
            };
        }
        StudioState {
            running: true,
            waiting: canvas_running && run.progress <= 0.0,
            progress: run.progress,
            picture: studio.picture.clone(),
            error: None,
        }
    }

    /// The Studio closed: a try going is stopped and its pictures let go.
    pub fn close_studio(&self) {
        self.cancel_try();
        let mut studio = self.studio.lock();
        studio.picture = None;
        studio.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::GenerationSettings;

    #[test]
    fn a_built_in_style_duplicates_and_writes_a_file_that_runs_as_one_of_yours() {
        let g = Generation::new(GenerationSettings::default());
        let mut copy = g.duplicate_style("anima-turbo".into()).unwrap();
        assert_eq!((copy.name.as_str(), copy.steps, copy.family.as_str()), ("Anima Turbo copy", 8, "Anima"));
        copy.loras.push(LoraSetting { file: "anima/a.safetensors".into(), strength: 0.75, trigger: String::new() });
        let name = g.style_file_name(copy.name.clone(), None);
        assert_eq!(name, "Anima Turbo copy.toml");
        let text = g.style_text(copy.clone(), None);
        g.set_style_files(vec![crate::StyleFileText { name: name.clone(), text }]);
        assert_eq!(g.style_edit(format!("user:{name}")), Some(copy));
        assert_eq!(g.style_file_name("Anima Turbo copy".into(), None), "Anima Turbo copy 2.toml");
        assert_eq!(g.style_file_name("Anima Turbo copy".into(), Some(name)), "Anima Turbo copy.toml");
    }

    #[test]
    fn without_a_server_try_it_says_so() {
        let g = Generation::new(GenerationSettings::default());
        let edit = g.style_edit("anima".into()).unwrap();
        assert!(g.try_style(edit, "a lighthouse".into(), 7, 1024, 1024).is_err());
        assert_eq!(g.studio_state(), StudioState::default());
    }
}
