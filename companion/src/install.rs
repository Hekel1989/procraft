//! `install`: sets up a ComfyUI folder from nothing, or brings a folder this companion made back to
//! the pins. Downloads and unpacking write over files but never delete anything, so models, inputs
//! and outputs survive an update.

use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use flate2::read::GzDecoder;
use zip::ZipArchive;

use crate::download;
use crate::installed::{self, Installed};
use crate::pins::{self, Gpu};

/// Sets up `dir`, or repairs it, for `gpu` (detected when `None`). A dry run only prints.
pub fn run(dir: &Path, gpu: Option<Gpu>, dry_run: bool) -> Result<(), String> {
    let recorded = check_dir(dir)?;
    let (gpu, warning) = choose_gpu(gpu, program_runs, cfg!(target_os = "linux"));
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }
    if !dry_run {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }

    install_uv(dir, recorded.as_ref(), dry_run)?;
    install_venv(dir, dry_run)?;
    install_comfyui(dir, recorded.as_ref(), dry_run)?;
    for add_on in &pins::ADD_ONS {
        install_add_on(dir, add_on, recorded.as_ref(), dry_run)?;
    }
    install_torch(dir, gpu, dry_run)?;
    install_requirements(dir, "ComfyUI", &comfyui_dir(dir).join("requirements.txt"), true, dry_run)?;
    for add_on in &pins::ADD_ONS {
        let requirements = add_on_dir(dir, add_on.folder).join("requirements.txt");
        install_requirements(dir, add_on.folder, &requirements, false, dry_run)?;
    }

    if !dry_run {
        let installed = Installed {
            comfyui_commit: pins::COMFYUI_COMMIT.to_owned(),
            add_ons: pins::ADD_ONS.iter().map(|add_on| (add_on.folder.to_owned(), add_on.commit.to_owned())).collect(),
            torch_index: gpu.index().to_owned(),
            uv_version: pins::UV_VERSION.to_owned(),
        };
        installed::write(dir, &installed)?;
    }

    print_next(dir);
    Ok(())
}

/// Checks that `dir` is empty, or is one the companion made. Refuses anything else.
fn check_dir(dir: &Path) -> Result<Option<Installed>, String> {
    match fs::metadata(dir) {
        Ok(metadata) if !metadata.is_dir() => Err(format!("{}: not a folder", dir.display())),
        Ok(_) => {
            if let Ok(installed) = installed::read(dir) {
                return Ok(Some(installed));
            }
            let mut entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            match entries.next() {
                Some(Ok(_)) => Err(format!(
                    "{} has things in it this companion didn't install; choose an empty folder",
                    dir.display()
                )),
                Some(Err(e)) => Err(format!("{}: {e}", dir.display())),
                None => Ok(None),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", dir.display())),
    }
}

/// Installs uv's binary unless the pinned version is already recorded and present.
fn install_uv(dir: &Path, recorded: Option<&Installed>, dry_run: bool) -> Result<(), String> {
    let up_to_date =
        recorded.is_some_and(|installed| installed.uv_version == pins::UV_VERSION) && uv_binary(dir).is_file();
    if up_to_date {
        println!("== uv {} (already there)", pins::UV_VERSION);
        return Ok(());
    }

    println!("== uv {}", pins::UV_VERSION);
    let archive = downloads_dir(dir).join(uv_archive_name());
    let url = uv_archive_url();
    if dry_run {
        println!("   download {url}");
        println!("   unpack {} -> {}", archive.display(), uv_dir(dir).display());
        return Ok(());
    }

    fs::create_dir_all(downloads_dir(dir)).map_err(|e| format!("{}: {e}", downloads_dir(dir).display()))?;
    download::download_unverified(&url, &archive)?;
    let unpacked = unpack(&archive, &uv_dir(dir), uv_archive_kind());
    let _ = fs::remove_file(&archive);
    unpacked?;
    make_executable(&uv_binary(dir))?;
    Ok(())
}

/// Makes the venv unless it already has a Python.
fn install_venv(dir: &Path, dry_run: bool) -> Result<(), String> {
    if venv_python(dir).is_file() {
        println!("== Python {} (already there)", pins::PYTHON_VERSION);
        return Ok(());
    }
    println!("== Python {}", pins::PYTHON_VERSION);
    run_uv(dir, &venv_args(dir), dry_run)
}

/// Unpacks ComfyUI unless its pinned commit is already recorded and its folder is there.
fn install_comfyui(dir: &Path, recorded: Option<&Installed>, dry_run: bool) -> Result<(), String> {
    let up_to_date =
        recorded.is_some_and(|installed| installed.comfyui_commit == pins::COMFYUI_COMMIT) && comfyui_dir(dir).is_dir();
    if up_to_date {
        println!("== ComfyUI {} (already there)", short_commit(pins::COMFYUI_COMMIT));
        return Ok(());
    }
    install_archive(dir, "ComfyUI", pins::COMFYUI_COMMIT, &comfyui_archive_url(), &comfyui_dir(dir), dry_run)
}

/// Unpacks one add-on unless its pinned commit is already recorded and its folder is there.
fn install_add_on(dir: &Path, add_on: &pins::AddOn, recorded: Option<&Installed>, dry_run: bool) -> Result<(), String> {
    let up_to_date = recorded
        .is_some_and(|installed| installed.add_ons.get(add_on.folder).is_some_and(|commit| commit == add_on.commit))
        && add_on_dir(dir, add_on.folder).is_dir();
    if up_to_date {
        println!("== {} {} (already there)", add_on.folder, short_commit(add_on.commit));
        return Ok(());
    }
    install_archive(dir, add_on.folder, add_on.commit, &add_on.archive_url(), &add_on_dir(dir, add_on.folder), dry_run)
}

/// Downloads a GitHub archive and unpacks it with its top folder stripped.
fn install_archive(
    dir: &Path,
    name: &str,
    commit: &str,
    url: &str,
    target: &Path,
    dry_run: bool,
) -> Result<(), String> {
    println!("== {name} {}", short_commit(commit));
    let archive = downloads_dir(dir).join(format!("{name}.tar.gz"));
    if dry_run {
        println!("   download {url}");
        println!("   unpack {} -> {}", archive.display(), target.display());
        return Ok(());
    }

    fs::create_dir_all(downloads_dir(dir)).map_err(|e| format!("{}: {e}", downloads_dir(dir).display()))?;
    download::download_unverified(url, &archive)?;
    let unpacked = unpack_tar_gz(&archive, target);
    let _ = fs::remove_file(&archive);
    unpacked
}

/// Installs PyTorch from the index for `gpu`.
fn install_torch(dir: &Path, gpu: Gpu, dry_run: bool) -> Result<(), String> {
    println!("== PyTorch ({}, {})", gpu_name(gpu), gpu.index());
    run_uv(dir, &torch_args(dir, gpu), dry_run)
}

/// Installs one `requirements.txt`; add-ons without one are skipped.
fn install_requirements(
    dir: &Path,
    name: &str,
    requirements: &Path,
    required: bool,
    dry_run: bool,
) -> Result<(), String> {
    if !requirements.is_file() {
        if dry_run {
            // the archive wasn't unpacked, so only ComfyUI's is known to have one
            println!("== {name} requirements{}", if required { "" } else { " (if present)" });
            return run_uv(dir, &requirements_args(dir, requirements), dry_run);
        }
        if required {
            return Err(format!("{}: missing", requirements.display()));
        }
        println!("== {name} requirements (none)");
        return Ok(());
    }
    println!("== {name} requirements");
    run_uv(dir, &requirements_args(dir, requirements), dry_run)
}

/// Runs a uv command with its cache and Python installs inside `dir`.
fn run_uv(dir: &Path, args: &[String], dry_run: bool) -> Result<(), String> {
    let program = uv_binary(dir);
    let line = command_line(&program, args);
    if dry_run {
        println!("   {line}");
        return Ok(());
    }
    let status = Command::new(&program)
        .args(args)
        .env("UV_CACHE_DIR", uv_cache_dir(dir))
        .env("UV_PYTHON_INSTALL_DIR", python_install_dir(dir))
        .status()
        .map_err(|e| format!("{line}: {e}"))?;
    if !status.success() {
        return Err(format!("{line}: {status}"));
    }
    Ok(())
}

/// Whether `program` runs here.
fn program_runs(program: &str) -> bool {
    Command::new(program).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|status| status.success())
}

/// The GPU to install for, and a warning when it isn't what was asked for.
fn choose_gpu(requested: Option<Gpu>, runs: impl Fn(&str) -> bool, linux: bool) -> (Gpu, Option<&'static str>) {
    match requested {
        Some(Gpu::Amd) if !linux => {
            (Gpu::Cpu, Some("no ROCm build for Windows: ComfyUI will run on the CPU, very slowly"))
        }
        Some(gpu) => (gpu, None),
        None => match detect_gpu_on(runs, linux) {
            Gpu::Cpu => (Gpu::Cpu, Some("no NVIDIA or AMD GPU found: ComfyUI will run on the CPU, very slowly")),
            gpu => (gpu, None),
        },
    }
}

/// Detects a GPU by asking whether `nvidia-smi` (any platform) or `rocminfo` (Linux) runs.
fn detect_gpu_on(runs: impl Fn(&str) -> bool, linux: bool) -> Gpu {
    if runs("nvidia-smi") {
        Gpu::Nvidia
    } else if linux && runs("rocminfo") {
        Gpu::Amd
    } else {
        Gpu::Cpu
    }
}

fn gpu_name(gpu: Gpu) -> &'static str {
    match gpu {
        Gpu::Nvidia => "CUDA",
        Gpu::Amd => "ROCm",
        Gpu::Intel => "XPU",
        Gpu::Cpu => "CPU",
    }
}

/// `uv venv --python 3.12 DIR/venv`.
fn venv_args(dir: &Path) -> Vec<String> {
    vec!["venv".to_owned(), "--python".to_owned(), pins::PYTHON_VERSION.to_owned(), path_text(&venv_dir(dir))]
}

/// `uv pip install --python <venv python> torch==2.14.1 torchvision torchaudio --index-url <index>`.
fn torch_args(dir: &Path, gpu: Gpu) -> Vec<String> {
    let mut args = vec![
        "pip".to_owned(),
        "install".to_owned(),
        "--python".to_owned(),
        path_text(&venv_python(dir)),
        format!("torch=={}", pins::TORCH_VERSION),
    ];
    for package in pins::TORCH_PACKAGES.iter().skip(1) {
        args.push((*package).to_owned());
    }
    args.push("--index-url".to_owned());
    args.push(gpu.index().to_owned());
    args
}

/// `uv pip install --python <venv python> -r <requirements>`.
fn requirements_args(dir: &Path, requirements: &Path) -> Vec<String> {
    vec![
        "pip".to_owned(),
        "install".to_owned(),
        "--python".to_owned(),
        path_text(&venv_python(dir)),
        "-r".to_owned(),
        path_text(requirements),
    ]
}

fn command_line(program: &Path, args: &[String]) -> String {
    let mut line = path_text(program);
    for arg in args {
        line.push(' ');
        line.push_str(arg);
    }
    line
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

fn short_commit(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

fn comfyui_archive_url() -> String {
    format!("{}/archive/{}.tar.gz", pins::COMFYUI_REPO, pins::COMFYUI_COMMIT)
}

fn uv_archive_name() -> &'static str {
    if cfg!(windows) { pins::UV_WINDOWS_ARCHIVE } else { pins::UV_LINUX_ARCHIVE }
}

fn uv_archive_url() -> String {
    format!("{}{}", pins::UV_RELEASE_URL, uv_archive_name())
}

fn uv_archive_kind() -> ArchiveKind {
    if cfg!(windows) { ArchiveKind::Zip } else { ArchiveKind::TarGz }
}

pub(crate) fn uv_dir(dir: &Path) -> PathBuf {
    dir.join("uv")
}

pub(crate) fn uv_binary(dir: &Path) -> PathBuf {
    uv_dir(dir).join(if cfg!(windows) { "uv.exe" } else { "uv" })
}

pub(crate) fn venv_dir(dir: &Path) -> PathBuf {
    dir.join("venv")
}

pub(crate) fn venv_python(dir: &Path) -> PathBuf {
    if cfg!(windows) {
        venv_dir(dir).join("Scripts").join("python.exe")
    } else {
        venv_dir(dir).join("bin").join("python")
    }
}

pub(crate) fn comfyui_dir(dir: &Path) -> PathBuf {
    dir.join("ComfyUI")
}

pub(crate) fn add_on_dir(dir: &Path, name: &str) -> PathBuf {
    comfyui_dir(dir).join("custom_nodes").join(name)
}

fn downloads_dir(dir: &Path) -> PathBuf {
    dir.join(".downloads")
}

fn uv_cache_dir(dir: &Path) -> PathBuf {
    dir.join(".uv-cache")
}

fn python_install_dir(dir: &Path) -> PathBuf {
    dir.join("python")
}

/// The next commands, with `--dir` when this isn't the default folder.
fn print_next(dir: &Path) {
    let custom = crate::default_dir().as_deref() != Some(dir);
    let dir = if custom { format!(" --dir {}", dir.display()) } else { String::new() };
    println!("Next: procraft-companion models anima{dir}");
    println!("Then: procraft-companion start{dir}");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Zip,
}

fn unpack(archive: &Path, dest: &Path, kind: ArchiveKind) -> Result<(), String> {
    match kind {
        ArchiveKind::TarGz => unpack_tar_gz(archive, dest),
        ArchiveKind::Zip => unpack_zip(archive, dest),
    }
}

/// Unpacks a `.tar.gz`, stripping the single top folder every GitHub archive has.
fn unpack_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let decoder = GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    let entries = tar.entries().map_err(|e| format!("{}: {e}", archive.display()))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("{}: {e}", archive.display()))?;
        let path = entry.path().map_err(|e| format!("{}: {e}", archive.display()))?.into_owned();
        let relative = strip_top(&path)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(&relative);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        } else if kind.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            entry.unpack(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
        // symlinks and special entries are left out: an archive should not point outside itself
    }
    Ok(())
}

/// Unpacks a zip, stripping the single top folder every uv archive has.
fn unpack_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("{}: {e}", archive.display()))?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| format!("{}: {e}", archive.display()))?;
        let name = entry.enclosed_name().ok_or_else(|| format!("{}: unsafe path", archive.display()))?;
        let relative = strip_top(&name)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            let mut output = File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
            std::io::copy(&mut entry, &mut output).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    Ok(())
}

/// `path` without its first folder, refusing anything that points outside the destination.
fn strip_top(path: &Path) -> Result<PathBuf, String> {
    let mut parts = path.components().filter(|component| !matches!(component, Component::CurDir));
    match parts.next() {
        Some(Component::Normal(_)) | None => {}
        Some(_) => return Err(format!("{}: archive path escapes the folder", path.display())),
    }
    let mut relative = PathBuf::new();
    for component in parts {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("{}: archive path escapes the folder", path.display()));
            }
        }
    }
    Ok(relative)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "procraft-companion-install-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (name, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *content).unwrap();
        }
        builder.finish().unwrap();
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        use zip::write::SimpleFileOptions;
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, content) in entries {
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file(*name, options).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn nvidia_is_detected_first() {
        assert_eq!(detect_gpu_on(|program| program == "nvidia-smi", true), Gpu::Nvidia);
        assert_eq!(detect_gpu_on(|program| program == "nvidia-smi", false), Gpu::Nvidia);
    }

    #[test]
    fn amd_is_detected_on_linux_only() {
        let rocminfo = |program: &str| program == "rocminfo";
        assert_eq!(detect_gpu_on(rocminfo, true), Gpu::Amd);
        assert_eq!(detect_gpu_on(rocminfo, false), Gpu::Cpu);
    }

    #[test]
    fn no_gpu_falls_back_to_cpu_with_a_warning() {
        let (gpu, warning) = choose_gpu(None, |_| false, true);
        assert_eq!(gpu, Gpu::Cpu);
        assert_eq!(warning, Some("no NVIDIA or AMD GPU found: ComfyUI will run on the CPU, very slowly"));
    }

    #[test]
    fn an_amd_choice_on_windows_falls_back_to_cpu() {
        let (gpu, warning) = choose_gpu(Some(Gpu::Amd), |_| false, false);
        assert_eq!(gpu, Gpu::Cpu);
        assert!(warning.is_some());
    }

    #[test]
    fn an_explicit_gpu_is_kept() {
        assert_eq!(choose_gpu(Some(Gpu::Intel), |_| false, true), (Gpu::Intel, None));
        assert_eq!(choose_gpu(Some(Gpu::Cpu), |_| false, true), (Gpu::Cpu, None));
    }

    #[test]
    fn the_folders_are_where_the_next_commands_expect_them() {
        let dir = Path::new("/data");
        assert_eq!(uv_dir(dir), Path::new("/data/uv"));
        assert_eq!(venv_dir(dir), Path::new("/data/venv"));
        assert_eq!(comfyui_dir(dir), Path::new("/data/ComfyUI"));
        assert_eq!(add_on_dir(dir, "nodes"), Path::new("/data/ComfyUI/custom_nodes/nodes"));
        assert_eq!(downloads_dir(dir), Path::new("/data/.downloads"));
        assert_eq!(uv_cache_dir(dir), Path::new("/data/.uv-cache"));
        assert_eq!(python_install_dir(dir), Path::new("/data/python"));
    }

    #[test]
    fn the_venv_command_is_plain() {
        assert_eq!(
            venv_args(Path::new("/data")),
            vec!["venv", "--python", "3.12", "/data/venv"].into_iter().map(str::to_owned).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_torch_command_pins_torch_and_uses_the_index() {
        let args = torch_args(Path::new("/data"), Gpu::Nvidia);
        let expected = [
            "pip".to_owned(),
            "install".to_owned(),
            "--python".to_owned(),
            path_text(&venv_python(Path::new("/data"))),
            "torch==2.14.1".to_owned(),
            "torchvision".to_owned(),
            "torchaudio".to_owned(),
            "--index-url".to_owned(),
            pins::TORCH_INDEX_NVIDIA.to_owned(),
        ];
        assert_eq!(args, expected);
    }

    #[test]
    fn the_requirements_command_uses_the_venv_python() {
        let args = requirements_args(Path::new("/data"), Path::new("/data/ComfyUI/requirements.txt"));
        let expected = [
            "pip".to_owned(),
            "install".to_owned(),
            "--python".to_owned(),
            path_text(&venv_python(Path::new("/data"))),
            "-r".to_owned(),
            "/data/ComfyUI/requirements.txt".to_owned(),
        ];
        assert_eq!(args, expected);
    }

    #[test]
    fn the_archive_urls_point_at_the_pinned_commits() {
        assert_eq!(comfyui_archive_url(), format!("{}/archive/{}.tar.gz", pins::COMFYUI_REPO, pins::COMFYUI_COMMIT));
        for add_on in &pins::ADD_ONS {
            assert_eq!(add_on.archive_url(), format!("{}/archive/{}.tar.gz", add_on.repo, add_on.commit));
        }
        assert!(uv_archive_url().starts_with(pins::UV_RELEASE_URL));
    }

    #[test]
    fn a_tar_gz_is_unpacked_with_its_top_folder_stripped() {
        let dir = temp_dir();
        let archive = dir.join("ComfyUI.tar.gz");
        write_tar_gz(
            &archive,
            &[("ComfyUI-abc/main.py", b"new main"), ("ComfyUI-abc/custom_nodes/nodes/__init__.py", b"node")],
        );
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("main.py"), b"old main").unwrap();
        fs::write(dest.join("keep.txt"), b"keep").unwrap();

        unpack_tar_gz(&archive, &dest).unwrap();

        assert_eq!(fs::read(dest.join("main.py")).unwrap(), b"new main");
        assert_eq!(fs::read(dest.join("custom_nodes/nodes/__init__.py")).unwrap(), b"node");
        assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"keep");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_zip_is_unpacked_with_its_top_folder_stripped() {
        let dir = temp_dir();
        let archive = dir.join("uv.zip");
        write_zip(
            &archive,
            &[("uv-x86_64-pc-windows-msvc/uv.exe", b"uv binary"), ("uv-x86_64-pc-windows-msvc/uvx.exe", b"uvx")],
        );
        let dest = dir.join("uv");

        unpack_zip(&archive, &dest).unwrap();

        assert_eq!(fs::read(dest.join("uv.exe")).unwrap(), b"uv binary");
        assert_eq!(fs::read(dest.join("uvx.exe")).unwrap(), b"uvx");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn archive_paths_cannot_escape_the_folder() {
        assert!(strip_top(Path::new("../outside")).is_err());
        assert!(strip_top(Path::new("top/../../outside")).is_err());
        assert_eq!(strip_top(Path::new("top/inner/file")).unwrap(), Path::new("inner/file"));
    }

    #[test]
    fn an_empty_folder_is_fine_and_a_used_one_is_refused() {
        let empty = temp_dir();
        assert!(check_dir(&empty).unwrap().is_none());
        let _ = fs::remove_dir_all(&empty);

        let used = temp_dir();
        fs::write(used.join("notes.txt"), b"mine").unwrap();
        let error = check_dir(&used).unwrap_err();
        assert!(error.contains("choose an empty folder"), "{error}");
        let _ = fs::remove_dir_all(used);
    }

    #[test]
    fn a_companion_folder_is_accepted() {
        let dir = temp_dir();
        let installed = Installed {
            comfyui_commit: pins::COMFYUI_COMMIT.to_owned(),
            add_ons: BTreeMap::new(),
            torch_index: pins::TORCH_INDEX_CPU.to_owned(),
            uv_version: pins::UV_VERSION.to_owned(),
        };
        installed::write(&dir, &installed).unwrap();
        fs::write(dir.join("other.txt"), b"ours").unwrap();
        assert_eq!(check_dir(&dir).unwrap(), Some(installed));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let dir = temp_dir();
        let _ = fs::remove_dir_all(&dir);
        run(&dir, Some(Gpu::Cpu), true).unwrap();
        assert!(!dir.exists());
    }
}
