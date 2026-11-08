//! Renders `brand/svg/*.svg` to `brand/png/<name>-<size>.png`, and the square
//! logos to `brand/icons/<name>.icns` (via `iconutil`, macOS only) and `.ico`.
//!
//! Usage: `cargo brand` from anywhere in the workspace.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use resvg::{tiny_skia, usvg};

const SIZES: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];
const ICO_SIZES: [u32; 5] = [16, 32, 64, 128, 256];
/// `iconutil` file names and the pixel size each one holds.
const ICONSET: [(&str, u32); 10] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
];

fn main() -> Result<()> {
    let brand = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("brand/render has no parent directory")?
        .to_path_buf();
    let svg_dir = brand.join("svg");
    let png_dir = brand.join("png");
    let icon_dir = brand.join("icons");
    fs::create_dir_all(&png_dir)?;
    fs::create_dir_all(&icon_dir)?;

    let mut svgs: Vec<PathBuf> = fs::read_dir(&svg_dir)
        .with_context(|| format!("reading {}", svg_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
        .collect();
    svgs.sort();
    if svgs.is_empty() {
        bail!("no SVGs in {}", svg_dir.display());
    }

    for svg in &svgs {
        let name = svg
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("non-UTF-8 SVG file name")?;
        let tree = load(svg)?;
        let square = (tree.size().width() - tree.size().height()).abs() < 0.5;

        for size in SIZES {
            let png = render(&tree, size)?;
            fs::write(png_dir.join(format!("{name}-{size}.png")), png)?;
        }
        if square {
            write_ico(&tree, &icon_dir.join(format!("{name}.ico")))?;
            write_icns(&tree, name, &icon_dir)?;
        }
        println!("rendered {name}");
    }
    Ok(())
}

fn load(path: &Path) -> Result<usvg::Tree> {
    let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    usvg::Tree::from_data(&data, &usvg::Options::default())
        .with_context(|| format!("parsing {}", path.display()))
}

/// Renders at `height` pixels tall, keeping the aspect ratio.
fn render_pixmap(tree: &usvg::Tree, height: u32) -> Result<tiny_skia::Pixmap> {
    let scale = height as f32 / tree.size().height();
    let width = (tree.size().width() * scale).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).context("invalid pixmap size")?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

fn render(tree: &usvg::Tree, height: u32) -> Result<Vec<u8>> {
    render_pixmap(tree, height)?
        .encode_png()
        .context("encoding PNG")
}

/// A 32-bit DIB icon entry: BITMAPINFOHEADER (double height for the mask),
/// bottom-up BGRA rows with straight alpha, then an all-zero AND mask.
fn render_dib(tree: &usvg::Tree, size: u32) -> Result<Vec<u8>> {
    let pixmap = render_pixmap(tree, size)?;
    let width = pixmap.width();
    let height = pixmap.height();
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&i32::try_from(width)?.to_le_bytes());
    out.extend_from_slice(&i32::try_from(height * 2)?.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for row in (0..height).rev() {
        for column in 0..width {
            let pixel = pixmap
                .pixel(column, row)
                .context("pixel out of bounds")?
                .demultiply();
            out.extend_from_slice(&[pixel.blue(), pixel.green(), pixel.red(), pixel.alpha()]);
        }
    }
    let mask_row = width.div_ceil(32) * 4;
    out.resize(out.len() + (mask_row * height) as usize, 0);
    Ok(out)
}

fn write_icns(tree: &usvg::Tree, name: &str, icon_dir: &Path) -> Result<()> {
    if !cfg!(target_os = "macos") {
        println!("skipping {name}.icns (iconutil is macOS-only)");
        return Ok(());
    }
    let iconset = std::env::temp_dir().join(format!("ferrum-brand-{name}.iconset"));
    if iconset.exists() {
        fs::remove_dir_all(&iconset)?;
    }
    fs::create_dir_all(&iconset)?;
    for (file, size) in ICONSET {
        fs::write(iconset.join(file), render(tree, size)?)?;
    }
    let output = icon_dir.join(format!("{name}.icns"));
    let status = Command::new("iconutil")
        .arg("-c")
        .arg("icns")
        .arg(&iconset)
        .arg("-o")
        .arg(&output)
        .status()
        .context("running iconutil")?;
    fs::remove_dir_all(&iconset)?;
    if !status.success() {
        bail!("iconutil failed for {name}: {status}");
    }
    Ok(())
}

/// ICO with classic DIB entries up to 64px (every Windows tool reads those)
/// and PNG-compressed entries above, like Windows' own icons.
fn write_ico(tree: &usvg::Tree, path: &Path) -> Result<()> {
    let images = ICO_SIZES
        .iter()
        .map(|size| {
            let data = if *size <= 64 {
                render_dib(tree, *size)?
            } else {
                render(tree, *size)?
            };
            Ok((*size, data))
        })
        .collect::<Result<Vec<_>>>()?;

    let count = u16::try_from(images.len())?;
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());

    let mut offset = 6 + 16 * u32::from(count);
    for (size, png) in &images {
        // 0 encodes 256 in the one-byte dimension fields.
        let dimension = if *size >= 256 {
            0
        } else {
            u8::try_from(*size)?
        };
        out.push(dimension);
        out.push(dimension);
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&u32::try_from(png.len())?.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += u32::try_from(png.len())?;
    }
    for (_, png) in &images {
        out.extend_from_slice(png);
    }
    fs::write(path, out).with_context(|| format!("writing {}", path.display()))
}
