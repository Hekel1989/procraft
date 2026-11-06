//! Every version, address and file the companion pins, in one place. `check` compares an installed
//! folder against these; `install` and `models` use them to download what is missing.

/// The uv version the companion installs.
pub const UV_VERSION: &str = "0.12.1";
/// Where uv's release archives are.
pub const UV_RELEASE_URL: &str = "https://github.com/astral-sh/uv/releases/download/0.12.1/";
/// The uv archive for Linux (x86-64).
pub const UV_LINUX_ARCHIVE: &str = "uv-x86_64-unknown-linux-gnu.tar.gz";
/// The uv archive for Windows (x86-64).
pub const UV_WINDOWS_ARCHIVE: &str = "uv-x86_64-pc-windows-msvc.zip";

/// The Python version the venv uses.
pub const PYTHON_VERSION: &str = "3.12";
/// The PyTorch version; the three packages are unpinned so the index chooses its own match.
pub const TORCH_VERSION: &str = "2.14.1";
/// The PyTorch packages to install.
pub const TORCH_PACKAGES: [&str; 3] = ["torch", "torchvision", "torchaudio"];
/// PyTorch's index for NVIDIA GPUs (CUDA 13.0).
pub const TORCH_INDEX_NVIDIA: &str = "https://download.pytorch.org/whl/cu130";
/// PyTorch's index for AMD GPUs (ROCm 7.2, Linux only).
pub const TORCH_INDEX_AMD: &str = "https://download.pytorch.org/whl/rocm7.2";
/// PyTorch's index for Intel GPUs (XPU).
pub const TORCH_INDEX_INTEL: &str = "https://download.pytorch.org/whl/xpu";
/// PyTorch's index for CPUs.
pub const TORCH_INDEX_CPU: &str = "https://download.pytorch.org/whl/cpu";
/// Every index an install may record.
pub const TORCH_INDEXES: [&str; 4] = [TORCH_INDEX_NVIDIA, TORCH_INDEX_AMD, TORCH_INDEX_INTEL, TORCH_INDEX_CPU];

/// The GPU an install builds PyTorch for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Gpu {
    Nvidia,
    Amd,
    Intel,
    Cpu,
}

impl Gpu {
    /// The PyTorch index for it.
    pub fn index(self) -> &'static str {
        match self {
            Gpu::Nvidia => TORCH_INDEX_NVIDIA,
            Gpu::Amd => TORCH_INDEX_AMD,
            Gpu::Intel => TORCH_INDEX_INTEL,
            Gpu::Cpu => TORCH_INDEX_CPU,
        }
    }
}

/// ComfyUI's repository.
pub const COMFYUI_REPO: &str = "https://github.com/comfyanonymous/ComfyUI";
/// The ComfyUI commit the companion installs.
pub const COMFYUI_COMMIT: &str = "e9027f2b30f37bb3052714eb08fcf479542f4fc0";

/// An add-on: its folder under `custom_nodes/`, its repository and the commit to install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddOn {
    pub folder: &'static str,
    pub repo: &'static str,
    pub commit: &'static str,
}

impl AddOn {
    /// The GitHub archive of its pinned commit.
    pub fn archive_url(self) -> String {
        format!("{}/archive/{}.tar.gz", self.repo, self.commit)
    }
}

/// The add-ons the companion installs.
pub const ADD_ONS: [AddOn; 3] = [
    AddOn {
        folder: "comfyui-tooling-nodes",
        repo: "https://github.com/Acly/comfyui-tooling-nodes",
        commit: "b3ae4aa2d98f6ac4284ddbe261e3559c94bd652b",
    },
    AddOn {
        folder: "comfyui-inpaint-nodes",
        repo: "https://github.com/Acly/comfyui-inpaint-nodes",
        commit: "bd6d8fd5104d539187dfb172d07352215c09fcaa",
    },
    AddOn {
        folder: "comfyui_controlnet_aux",
        repo: "https://github.com/Fannovel16/comfyui_controlnet_aux",
        commit: "e8b689a513c3e6b63edc44066560ca5919c0576e",
    },
];

/// A model file: its folder under `models/`, its name, size, SHA-256 and download URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelFile {
    pub folder: &'static str,
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    pub url: &'static str,
}

/// A model family's files, licence line and link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelSet {
    pub id: &'static str,
    pub licence: &'static str,
    pub link: &'static str,
    pub files: &'static [ModelFile],
}

/// Anima's files, from CircleStone Labs' official Hugging Face repository.
pub const ANIMA: ModelSet = ModelSet {
    id: "anima",
    licence: "Anima's weights: CircleStone Labs non-commercial licence",
    link: "https://huggingface.co/circlestone-labs/Anima",
    files: &[
        ModelFile {
            folder: "diffusion_models",
            name: "anima-base-v1.0.safetensors",
            size: 4182218328,
            sha256: "bd43b7cffe1ed1153d9c41e7beb2f18cb1273eafbaa3af3edd6a173dc90a006e",
            url: "https://huggingface.co/circlestone-labs/Anima/resolve/main/split_files/diffusion_models/anima-base-v1.0.safetensors",
        },
        ModelFile {
            folder: "diffusion_models",
            name: "anima-turbo-v1.0.safetensors",
            size: 4182230656,
            sha256: "c0b905034510750a505d21aa96c81718f4ffcc500777318421f58a88636e2174",
            url: "https://huggingface.co/circlestone-labs/Anima/resolve/main/split_files/diffusion_models/anima-turbo-v1.0.safetensors",
        },
        ModelFile {
            folder: "text_encoders",
            name: "qwen_3_06b_base.safetensors",
            size: 1192135096,
            sha256: "cd2a512003e2f9f3cd3c32a9c3573f820bb28c940f73c57b1ddaa983d9223eba",
            url: "https://huggingface.co/circlestone-labs/Anima/resolve/main/split_files/text_encoders/qwen_3_06b_base.safetensors",
        },
        ModelFile {
            folder: "vae",
            name: "qwen_image_vae.safetensors",
            size: 253806246,
            sha256: "a70580f0213e67967ee9c95f05bb400e8fb08307e017a924bf3441223e023d1f",
            url: "https://huggingface.co/circlestone-labs/Anima/resolve/main/split_files/vae/qwen_image_vae.safetensors",
        },
        ModelFile {
            folder: "controlnet",
            name: "anima-lllite-inpainting-v2.safetensors",
            size: 65772872,
            sha256: "5242e677d2be34ee70ca7c97c3b14ff5ee49838c03fc1e60ac4852a180db6ef5",
            url: "https://huggingface.co/kohya-ss/Anima-LLLite/resolve/main/anima-lllite-inpainting-v2.safetensors",
        },
        ModelFile {
            folder: "controlnet",
            name: "anima-lllite-any-test-like-v2.safetensors",
            size: 16403008,
            sha256: "8863424f04a2445815a827b22cbda557a085d2752e05fab54a2b303821a9e3b2",
            url: "https://huggingface.co/kohya-ss/Anima-LLLite/resolve/main/anima-lllite-any-test-like-v2.safetensors",
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use procraft_diffusion::requirements::{Feature, Missing, Readiness, all_nodes};
    use procraft_diffusion::{Family, ServerModels};

    /// A server that has `files` and every node the app asks for.
    fn server(files: &[&ModelFile]) -> ServerModels {
        let mut models = ServerModels::default();
        for file in files {
            match file.folder {
                "diffusion_models" => models.diffusion_models.push(file.name.to_owned()),
                "text_encoders" => models.text_encoders.push(file.name.to_owned()),
                "vae" => models.vaes.push(file.name.to_owned()),
                "controlnet" => models.controls.push(file.name.to_owned()),
                other => panic!("unexpected model folder {other}"),
            }
        }
        models.nodes = all_nodes().into_iter().map(str::to_owned).collect();
        models
    }

    #[test]
    fn together_the_pinned_files_make_every_feature_ready() {
        let files: Vec<&ModelFile> = ANIMA.files.iter().collect();
        let readiness = Readiness::check(Family::Anima, &server(&files));
        for feature in Feature::ALL {
            assert!(readiness.ready(feature), "{}: {:?}", feature.label(), readiness.missing(feature));
        }
    }

    #[test]
    fn each_pinned_file_satisfies_the_requirement_for_its_kind() {
        for file in ANIMA.files {
            if file.folder == "diffusion_models" {
                // either checkpoint on its own is enough for every feature
                let files: Vec<&ModelFile> = ANIMA
                    .files
                    .iter()
                    .filter(|other| other.folder != "diffusion_models" || other.name == file.name)
                    .collect();
                let readiness = Readiness::check(Family::Anima, &server(&files));
                for feature in Feature::ALL {
                    assert!(
                        readiness.ready(feature),
                        "{} with only {}: {:?}",
                        feature.label(),
                        file.name,
                        readiness.missing(feature)
                    );
                }
            } else {
                // without it, a feature that needs its kind must say so
                let files: Vec<&ModelFile> = ANIMA.files.iter().filter(|other| other.name != file.name).collect();
                let readiness = Readiness::check(Family::Anima, &server(&files));
                let used = Feature::ALL.iter().any(|&feature| {
                    readiness
                        .missing(feature)
                        .iter()
                        .any(|missing| matches!(missing, Missing::Model { folder, .. } if *folder == file.folder))
                });
                assert!(used, "{} is not used as a {} file", file.name, file.folder);
            }
        }
    }
}
