//! The styles the Generate popover offers: the person's own, from their styles folder, then the
//! built-in ones and one for each other checkpoint the server has. The apps read the folder (on
//! Android through its document API, which Rust can't use) and hand the files' text over each time
//! the popover opens; `procraft-diffusion` reads them, and a run uses the style by its id.

use procraft_diffusion::styles::Part;
use procraft_diffusion::{ServerState, Style, StyleFile, StyleKind as Kind, styles::name_of};

use crate::generation::Generation;

/// Where a style comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StyleKind {
    /// Comes with the app: read-only, duplicated to be changed.
    BuiltIn,
    /// Made from a checkpoint found on the server.
    Found,
    /// The person's own, from their styles folder.
    Yours,
}

/// A style as the popover's cards show it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StyleInfo {
    pub id: String,
    pub name: String,
    pub steps: u32,
    pub cfg: f32,
    /// The model family's short name, for the card's badge ("Anima").
    pub family: String,
    pub kind: StyleKind,
    /// The file in the styles folder, for the person's own.
    pub file: Option<String>,
    /// A short note in place of the card's settings line when it can't run: "Missing LoRA",
    /// "Can't be read".
    pub note: Option<String>,
    /// Why it can't run, in full: what the server hasn't got, or what's wrong in its file.
    pub reason: Option<String>,
}

/// A file of the styles folder: its name (`Moonlit.toml`) and text.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StyleFileText {
    pub name: String,
    pub text: String,
}

impl From<&Style> for StyleInfo {
    fn from(s: &Style) -> StyleInfo {
        let kind = match s.kind {
            Kind::BuiltIn => StyleKind::BuiltIn,
            Kind::Found => StyleKind::Found,
            Kind::Yours => StyleKind::Yours,
        };
        let (note, reason) = match (&s.problem, s.missing.first()) {
            (Some(problem), _) => (Some("Can't be read".to_owned()), Some(problem.clone())),
            (None, Some((part, _))) => {
                let names: Vec<String> = s.missing.iter().map(|(_, file)| name_of(file)).collect();
                (Some(missing_note(*part).to_owned()), Some(format!("The server hasn't got {}", names.join(", "))))
            }
            (None, None) => (None, None),
        };
        StyleInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            steps: s.steps,
            cfg: s.cfg,
            family: s.family.name().to_owned(),
            kind,
            file: s.id.strip_prefix("user:").map(str::to_owned),
            note,
            reason,
        }
    }
}

fn missing_note(part: Part) -> &'static str {
    match part {
        Part::Model => "Missing model",
        Part::Lora => "Missing LoRA",
        Part::Vae => "Missing VAE",
        Part::TextEncoder => "Missing text encoder",
    }
}

#[uniffi::export]
impl Generation {
    /// The files of the person's styles folder, read by the app (each `.toml` there): used from now
    /// on, until the next call.
    pub fn set_style_files(&self, files: Vec<StyleFileText>) {
        *self.style_files.lock() = files.into_iter().map(|f| StyleFile { name: f.name, text: f.text }).collect();
    }

    /// The styles the popover offers, in their order: the person's own (by name), the built-in ones
    /// the server can run, then the found ones. Theirs show even when they can't run, with why.
    pub fn styles(&self) -> Vec<StyleInfo> {
        self.server_styles(&self.worker.server()).iter().map(StyleInfo::from).collect()
    }
}

impl Generation {
    /// Every style on `server`, the person's files included.
    pub(crate) fn server_styles(&self, server: &ServerState) -> Vec<Style> {
        server.styles_with(&self.style_files.lock())
    }

    /// The style `id` on `server`, or the first that can run when there's no such style.
    pub(crate) fn style_by_id(&self, server: &ServerState, id: &str) -> Option<Style> {
        let styles = self.server_styles(server);
        let runnable = |s: &&Style| s.problem.is_none() && s.missing.is_empty();
        styles.iter().find(|s| s.id == id).or_else(|| styles.iter().find(runnable)).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::GenerationSettings;

    #[test]
    fn without_a_server_there_are_no_styles() {
        let g = Generation::new(GenerationSettings::default());
        g.set_style_files(vec![StyleFileText { name: "Moonlit.toml".into(), text: "model = \"a\"".into() }]);
        assert!(g.styles().is_empty());
    }

    #[test]
    fn a_style_that_cant_run_says_why() {
        let style = procraft_diffusion::styles_for(
            &procraft_diffusion::ServerModels {
                diffusion_models: vec!["anima_baseV10.safetensors".into()],
                ..Default::default()
            },
            &[
                StyleFile {
                    name: "M.toml".into(),
                    text:
                        "model = \"anima_baseV10.safetensors\"\n[[lora]]\nfile = \"anima/dark_art_style.safetensors\""
                            .into(),
                },
                StyleFile { name: "B.toml".into(), text: "model = ".into() },
            ],
            &Default::default(),
        );
        let infos: Vec<StyleInfo> = style.iter().map(StyleInfo::from).collect();
        assert_eq!(infos[0].note.as_deref(), Some("Can't be read"));
        assert_eq!(infos[1].note.as_deref(), Some("Missing LoRA"));
        assert_eq!(infos[1].reason.as_deref(), Some("The server hasn't got dark art style"));
        assert_eq!(
            (infos[1].kind, infos[1].file.as_deref(), infos[1].family.as_str()),
            (StyleKind::Yours, Some("M.toml"), "Anima")
        );
        assert_eq!((infos[2].kind, infos[2].note.clone()), (StyleKind::BuiltIn, None));
    }
}
