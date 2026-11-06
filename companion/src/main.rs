//! The ProCraft Diffusion companion: sets up a ComfyUI server for the app and checks it.
//!
//! ```text
//! procraft-companion install [--dir DIR] [--gpu nvidia|amd|intel|cpu]
//! procraft-companion models anima [--dir DIR] [--accept-licence]
//! procraft-companion check [DIR | URL] [--verify]
//! procraft-companion start [--dir DIR] [--port 8189] [--lan]
//! procraft-companion stop [--dir DIR]
//! ```

mod check;
mod download;
mod installed;
mod models;
mod pins;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use pins::Gpu;

#[derive(Parser)]
#[command(name = "procraft-companion", version, about = "Sets up and checks a ComfyUI server for ProCraft Diffusion")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Installs uv, ComfyUI and its add-ons into a folder.
    Install {
        /// The folder to install into (default: the companion's data folder).
        #[arg(long)]
        dir: Option<PathBuf>,
        /// The GPU's PyTorch build (default: detected).
        #[arg(long, value_enum)]
        gpu: Option<Gpu>,
    },
    /// Downloads model files into an installed folder.
    Models {
        #[command(subcommand)]
        model: Model,
    },
    /// Checks a folder, or a running server, without writing anything.
    Check {
        /// A folder, or a server (`http://host:port` or `host:port`); default: the companion's folder.
        target: Option<String>,
        /// Also hash the pinned model files.
        #[arg(long)]
        verify: bool,
    },
    /// Starts ComfyUI from a folder.
    Start {
        /// The folder to run from (default: the companion's data folder).
        #[arg(long)]
        dir: Option<PathBuf>,
        /// The port ComfyUI listens on.
        #[arg(long, default_value_t = 8189)]
        port: u16,
        /// Listen on all addresses, so the tablet can reach it over the LAN.
        #[arg(long)]
        lan: bool,
    },
    /// Stops a ComfyUI the companion started.
    Stop {
        /// The folder it runs from (default: the companion's data folder).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Model {
    /// The Anima model files.
    Anima {
        /// The folder to download into (default: the companion's data folder).
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Accept the licence without asking.
        #[arg(long)]
        accept_licence: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    match Cli::parse().command {
        Command::Install { dir: _, gpu: _ } | Command::Start { dir: _, port: _, lan: _ } | Command::Stop { dir: _ } => {
            eprintln!("not built yet");
            Ok(ExitCode::from(2))
        }
        Command::Models { model: Model::Anima { dir, accept_licence } } => {
            models::anima(&dir_or_default(dir)?, accept_licence)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Check { target, verify } => {
            let target = match target {
                Some(target) => target,
                None => dir_or_default(None)?.display().to_string(),
            };
            let ready = check::run(&target, verify)?;
            Ok(ExitCode::from(if ready { 0 } else { 1 }))
        }
    }
}

/// `dir`, else the platform's data folder.
fn dir_or_default(dir: Option<PathBuf>) -> Result<PathBuf, String> {
    dir.or_else(default_dir)
        .ok_or_else(|| "can't find the data folder: set LOCALAPPDATA, HOME or XDG_DATA_HOME".to_owned())
}

/// `~/.local/share/procraft-companion` on Linux, `%LOCALAPPDATA%\ProCraft Companion` on Windows.
fn default_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        env_path("LOCALAPPDATA").map(|dir| dir.join("ProCraft Companion"))
    } else {
        env_path("XDG_DATA_HOME")
            .map(|dir| dir.join("procraft-companion"))
            .or_else(|| env_path("HOME").map(|dir| dir.join(".local/share/procraft-companion")))
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}
