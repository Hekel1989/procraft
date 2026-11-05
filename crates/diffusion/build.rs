//! Packs the tag CSVs into the binary: each is deflated into `OUT_DIR`, and `tags.rs` embeds the
//! compressed bytes with `include_bytes!`. Both apps then carry the lists without asset plumbing.

use std::env;
use std::fs;
use std::path::PathBuf;

use miniz_oxide::deflate::compress_to_vec;

/// The lists, the source they come from and whether they hold NSFW tags.
const LISTS: [(&str, &str); 4] = [
    ("tags/Danbooru.csv", "danbooru.bin"),
    ("tags/Danbooru NSFW.csv", "danbooru-nsfw.bin"),
    ("tags/e621.csv", "e621.bin"),
    ("tags/e621 NSFW.csv", "e621-nsfw.bin"),
];

fn main() {
    for (source, _) in LISTS {
        println!("cargo:rerun-if-changed={source}");
    }
    let Some(out_dir) = env::var_os("OUT_DIR").map(PathBuf::from) else { return };
    for (source, packed) in LISTS {
        if let Err(e) = pack(source, &out_dir.join(packed)) {
            println!("cargo:warning=packing {source}: {e}");
            std::process::exit(1);
        }
    }
}

/// Deflates `source` into `packed`.
fn pack(source: &str, packed: &std::path::Path) -> std::io::Result<()> {
    let data = fs::read(source)?;
    let compressed = compress_to_vec(&data, 10);
    fs::write(packed, compressed)
}
