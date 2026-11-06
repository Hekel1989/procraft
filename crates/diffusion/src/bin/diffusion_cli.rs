//! Runs `procraft-diffusion` from a terminal against a ComfyUI server, to try it without the app:
//!
//! ```text
//! diffusion-cli [--server ADDRESS] check
//! diffusion-cli [--server ADDRESS] generate [PROMPT] [--style ID|FILE.toml] [--size WxH] [--count N]
//!     [--seed N] [--strength S --input FILE] [--resolution draft|normal|quality]
//!     [--mask FILE --mode fill|add|remove|expand] [--largest MP] [--nsfw] [-o FILE]
//! ```
//!
//! The server is `--server`, else `PROCRAFT_COMFY_URL`, else `127.0.0.1:8189`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use procraft_diffusion::{
    Address, Connection, Family, Feature, Picture, Plan, Request, Resolution, Selection, SelectionMode, ServerState,
    StyleFile, Worker, largest_for_vram, plan_at,
};

const USAGE: &str = "usage: diffusion-cli [--server ADDRESS] check
       diffusion-cli [--server ADDRESS] generate [PROMPT] [--style ID|FILE.toml] [--size WxH] [--count N]
           [--seed N] [--strength S --input FILE] [--resolution draft|normal|quality]
           [--mask FILE --mode fill|add|remove|expand] [--largest MP] [--nsfw] [-o FILE]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// The value after flag `name`, if given.
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn number<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> Result<T, String> {
    flag(args, name).map_or(Ok(default), |v| v.parse().map_err(|_| format!("{name}: not a number: {v}")))
}

fn resolution(args: &[String]) -> Result<Resolution, String> {
    match flag(args, "--resolution").unwrap_or("quality").to_ascii_lowercase().as_str() {
        "draft" => Ok(Resolution::Draft),
        "normal" => Ok(Resolution::Normal),
        "quality" => Ok(Resolution::Quality),
        other => Err(format!("--resolution: draft, normal or quality, not {other}")),
    }
}

fn selection_mode(args: &[String]) -> Result<SelectionMode, String> {
    match flag(args, "--mode").unwrap_or("fill").to_ascii_lowercase().as_str() {
        "fill" => Ok(SelectionMode::Fill),
        "add" => Ok(SelectionMode::Add),
        "remove" => Ok(SelectionMode::Remove),
        "expand" => Ok(SelectionMode::Expand),
        other => Err(format!("--mode: fill, add, remove or expand, not {other}")),
    }
}

fn cli(args: &[String]) -> Result<(), String> {
    let server = flag(args, "--server")
        .map(str::to_owned)
        .or_else(|| std::env::var("PROCRAFT_COMFY_URL").ok())
        .unwrap_or_else(|| "127.0.0.1:8189".to_owned());
    let address = Address::parse(&server).ok_or_else(|| format!("not an address: {server}"))?;
    // the words that aren't flags or their values
    let mut words = Vec::new();
    let mut skip = false;
    for a in args {
        if std::mem::take(&mut skip) {
            continue;
        }
        if a.starts_with('-') {
            skip = a != "--nsfw";
        } else {
            words.push(a.as_str());
        }
    }
    let worker = Worker::new();
    worker.set_address(Some(address));
    let server = connected(&worker)?;
    match words.as_slice() {
        ["check"] => {
            check(&server);
            Ok(())
        }
        ["generate", prompt] => generate(&worker, &server, prompt, args),
        // a selection can have no prompt at all (Fill and Remove don't need one)
        ["generate"] if flag(args, "--mask").is_some() => generate(&worker, &server, "", args),
        _ => Err(USAGE.to_owned()),
    }
}

/// Waits for the worker to connect.
fn connected(worker: &Worker) -> Result<ServerState, String> {
    loop {
        let server = worker.server();
        match &server.connection {
            Connection::Connected => return Ok(server),
            Connection::Failed(e) => return Err(e.to_string()),
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn check(server: &ServerState) {
    if let Some(stats) = &server.stats {
        let gpu =
            stats.gpu.as_ref().map(|(name, vram)| format!(", {name} ({} GB)", vram / (1 << 30))).unwrap_or_default();
        println!("ComfyUI {}{gpu}", stats.comfyui_version);
    }
    for (family, readiness) in &server.readiness {
        println!("{}:", family.name());
        for feature in Feature::ALL {
            let missing = readiness.missing(feature);
            if missing.is_empty() {
                println!("  ✓ {}", feature.label());
            } else {
                let list: Vec<String> = missing.iter().map(ToString::to_string).collect();
                println!("  ✕ {}: needs {}", feature.label(), list.join(", "));
            }
        }
    }
    println!("styles:");
    for style in server.styles() {
        println!("  {:<40} {} ({} steps, CFG {})", style.id, style.name, style.steps, style.cfg);
    }
}

fn generate(worker: &Worker, server: &ServerState, prompt: &str, args: &[String]) -> Result<(), String> {
    let wanted = flag(args, "--style").unwrap_or("anima");
    // a style file (`Moonlit.toml`) is read as one of the person's own
    let files = match wanted.ends_with(".toml") {
        true => {
            let text = std::fs::read_to_string(wanted).map_err(|e| format!("{wanted}: {e}"))?;
            let name = Path::new(wanted).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            vec![StyleFile { name, text }]
        }
        false => Vec::new(),
    };
    let styles = server.styles_with(&files);
    let style = match files.first() {
        Some(file) => styles.iter().find(|s| s.id == format!("user:{}", file.name)),
        None => styles.iter().find(|s| s.id == wanted || s.name.eq_ignore_ascii_case(wanted)),
    }
    .cloned()
    .ok_or_else(|| format!("no style {wanted} on this server (see `check`)"))?;
    if let Some(problem) = &style.problem {
        return Err(format!("{wanted}: {problem}"));
    }
    for lora in &style.loras {
        println!(
            "LoRA {} at {:.0} %{}",
            lora.file,
            lora.strength * 100.0,
            match lora.trigger.is_empty() {
                true => String::new(),
                false => format!(", trigger words {}", lora.trigger),
            }
        );
    }
    let picture = flag(args, "--input")
        .map(|file| {
            std::fs::read(file)
                .map_err(|e| format!("{file}: {e}"))
                .and_then(|b| Picture::decode(&b).map_err(|e| e.to_string()))
        })
        .transpose()?;
    let size = match (flag(args, "--size"), &picture) {
        (Some(s), _) => {
            let (w, h) = s.split_once('x').ok_or("--size: WxH")?;
            (w.parse().map_err(|_| "--size: WxH")?, h.parse().map_err(|_| "--size: WxH")?)
        }
        (None, Some(p)) => (p.width, p.height),
        (None, None) => (1024, 1024),
    };
    let random = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64 % 1_000_000_000).unwrap_or(0);
    let resolution = resolution(args)?;
    let mask = flag(args, "--mask")
        .map(|file| -> Result<Vec<u8>, String> {
            let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
            let image = image::load_from_memory(&bytes).map_err(|e| format!("{file}: {e}"))?.into_luma8();
            if (image.width(), image.height()) == size {
                Ok(image.into_raw())
            } else {
                let scaled = image::imageops::resize(&image, size.0, size.1, image::imageops::FilterType::Triangle);
                Ok(scaled.into_raw())
            }
        })
        .transpose()?;
    if mask.is_some() && picture.is_none() {
        return Err("--mask: needs --input, the picture the mask selects in".to_owned());
    }
    let selection = match mask {
        Some(mask) => Some(Selection { mask, mode: selection_mode(args)? }),
        None => None,
    };
    let largest: Option<f32> =
        flag(args, "--largest").map(|v| v.parse().map_err(|_| format!("--largest: not a number: {v}"))).transpose()?;
    let request = Request {
        prompt: prompt.to_owned(),
        style,
        strength: number(args, "--strength", 1.0)?,
        seed: number(args, "--seed", random)?,
        count: number(args, "--count", 1)?,
        picture: picture.map(|p| p.resized(size.0, size.1)).transpose().map_err(|e| e.to_string())?,
        selection,
        controls: Vec::new(),
        size,
        resolution,
        largest,
        nsfw: args.iter().any(|a| a == "--nsfw"),
    };
    if request.style.family != Family::Anima {
        return Err("only Anima so far".to_owned());
    }
    let vram = server.stats.as_ref().and_then(|s| s.gpu.as_ref()).map_or(0, |(_, bytes)| *bytes);
    let largest = request.largest.unwrap_or_else(|| largest_for_vram(vram)).min(request.style.family.most_megapixels());
    let one_pass = !request.is_new() || request.selection.is_some();
    let plan = plan_at(size.0, size.1, request.style.model_size(), request.resolution, largest, one_pass);
    print_plan(&plan);
    let out = PathBuf::from(flag(args, "-o").unwrap_or("generated.png"));
    let started = Instant::now();
    let id = worker.submit(request.clone());
    let mut saved = 0;
    // sampling previews seen (the server sends them when started with --preview-method)
    let mut previews = 0;
    let mut last_preview = None;
    loop {
        let Some(run) = worker.run(id) else { return Err("the run went missing".to_owned()) };
        for result in run.results.iter().skip(saved) {
            let file = numbered(&out, saved, request.count);
            let png = result.picture.to_png().map_err(|e| e.to_string())?;
            std::fs::write(&file, png).map_err(|e| format!("{}: {e}", file.display()))?;
            println!("{} (seed {}, {:.1} s)", file.display(), result.seed, started.elapsed().as_secs_f32());
            saved += 1;
        }
        if run.preview.is_some() && run.preview != last_preview {
            previews += 1;
            last_preview = run.preview.clone();
        }
        if run.finished {
            println!("{previews} sampling previews");
            return match run.error {
                Some(e) => Err(e.to_string()),
                None => Ok(()),
            };
        }
        eprint!("\r{:>3.0}%", run.progress * 100.0);
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The sizes and passes of a plan, before it runs.
fn print_plan(plan: &Plan) {
    let passes = plan.passes();
    let mut words = format!(
        "{} × {} target, {passes} pass{}: {} × {}",
        plan.target.width,
        plan.target.height,
        if passes == 1 { "" } else { "es" },
        plan.first.width,
        plan.first.height
    );
    if let Some(second) = plan.second {
        words.push_str(&format!(", then {} × {}", second.width, second.height));
    }
    if plan.capped {
        words.push_str(" (capped)");
    }
    println!("plan: {words}");
}

/// `out` as it is for one picture; `out-1.png`, `out-2.png`… for several.
fn numbered(out: &Path, index: usize, count: u32) -> PathBuf {
    if count <= 1 {
        return out.to_path_buf();
    }
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("generated");
    out.with_file_name(format!("{stem}-{}.png", index + 1))
}
