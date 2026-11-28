//! Builds `bot/assets/grimlite.swf` from `host/as3/` using Ruffle's `asc.jar`.
//! Usage:
//!   cargo run -p grimlite_host
//!   cargo run -p grimlite_host -- --check

use anyhow::{Context, anyhow, bail, ensure};
use std::path::{Path, PathBuf};
use swf::{
    Color, Compression, DoAbc2, DoAbc2Flag, FileAttributes, Fixed8, FrameLabel, Header, Rectangle,
    SwfStr, SymbolClassLink, Tag, Twips,
};

const MAIN_CLASS: &str = "grimlite.host.Main";
const SWF_VERSION: u8 = 43;
const WIDTH: f64 = 958.0;
const HEIGHT: f64 = 550.0;
const FRAME_RATE: u8 = 30;

fn main() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = root.parent().context("host/ has no parent")?;
    let mut out = repo.join("bot/assets/grimlite.swf");
    let mut check = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--out" => out = args.next().context("--out needs a path")?.into(),
            _ => bail!("usage: grimlite_host [--check] [--out <path>]"),
        }
    }

    let swf_bytes = build(root)?;
    if check {
        let committed =
            std::fs::read(&out).with_context(|| format!("couldn't read {}", out.display()))?;
        ensure!(
            committed == swf_bytes,
            "{} is out of date: run `cargo run -p grimlite_host`",
            out.display()
        );
        println!("{} is up to date ({} bytes)", out.display(), swf_bytes.len());
    } else {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out, &swf_bytes)
            .with_context(|| format!("couldn't write {}", out.display()))?;
        println!("wrote {} ({} bytes)", out.display(), swf_bytes.len());
    }
    Ok(())
}

fn build(root: &Path) -> anyhow::Result<Vec<u8>> {
    let as3 = root.join("as3");
    let ruffle = root.join("../ruffle");
    let tmp = tempfile::tempdir()?;

    build_playerglobal::build_avm2_playerglobal(&ruffle, tmp.path(), false)
        .map_err(|e| anyhow!("couldn't build Ruffle's playerglobal: {e}"))?;
    let playerglobal = tmp.path().join("playerglobal_import.abc");

    let files = collect_as_files(&as3)?;
    ensure!(!files.is_empty(), "no .as files in {}", as3.display());

    let wrapper = tmp.path().join("grimlite.as");
    let includes: String = files
        .iter()
        .map(|p| format!("include \"{}\";\n", p.display().to_string().replace('\\', "/")))
        .collect();
    std::fs::write(&wrapper, includes)?;

    let abc_path = wrapper.with_extension("abc");
    asc::run_asc(asc::AscConfig::new([
        "-AS3".to_string(),
        "-strict".to_string(),
        "-optimize".to_string(),
        "-import".to_string(),
        playerglobal.to_string_lossy().into_owned(),
        wrapper.to_string_lossy().into_owned(),
    ]))?;

    let abc = std::fs::read(&abc_path).context("asc.jar wrote no ABC (see errors above)")?;
    wrap_swf(&abc)
}

fn collect_as_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "as") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn wrap_swf(abc: &[u8]) -> anyhow::Result<Vec<u8>> {
    let header = Header {
        compression: Compression::Zlib,
        version: SWF_VERSION,
        stage_size: Rectangle {
            x_min: Twips::ZERO,
            x_max: Twips::from_pixels(WIDTH),
            y_min: Twips::ZERO,
            y_max: Twips::from_pixels(HEIGHT),
        },
        frame_rate: Fixed8::from_f32(FRAME_RATE.into()),
        num_frames: 1,
    };
    let label = MAIN_CLASS.replace('.', "_");
    let tags = [
        Tag::FileAttributes(
            FileAttributes::IS_ACTION_SCRIPT_3 | FileAttributes::USE_NETWORK_SANDBOX,
        ),
        Tag::ScriptLimits {
            max_recursion_depth: 1000,
            timeout_in_seconds: 60,
        },
        Tag::SetBackgroundColor(Color::BLACK),
        Tag::FrameLabel(FrameLabel {
            label: SwfStr::from_utf8_str(&label),
            is_anchor: false,
        }),
        Tag::DoAbc2(DoAbc2 {
            flags: DoAbc2Flag::LAZY_INITIALIZE,
            name: SwfStr::from_utf8_str(""),
            data: abc,
        }),
        Tag::SymbolClass(vec![SymbolClassLink {
            id: 0,
            class_name: SwfStr::from_utf8_str(MAIN_CLASS),
        }]),
        Tag::ShowFrame,
    ];
    let mut out = Vec::new();
    swf::write_swf(&header, &tags, &mut out)?;
    let read = swf::decompress_swf(&out[..])?;
    let parsed = swf::parse_swf(&read)?;
    ensure!(
        parsed.tags.len() == tags.len(),
        "written SWF parsed as {} tags instead of {}",
        parsed.tags.len(),
        tags.len()
    );
    Ok(out)
}
