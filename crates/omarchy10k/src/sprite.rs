//! Half-block sprite rendering: a small image becomes coloured terminal text.
//!
//! One `▀` (U+2580, UPPER HALF BLOCK) per character cell carries TWO pixel
//! rows — the foreground colour paints the upper pixel, the background colour
//! the lower one. A cell is therefore square-ish, which is why sprites drawn
//! this way keep their proportions in a terminal whose cells are ~1:2.
//!
//! This is the technique `krabby`, `pokeget` and `pokemon-colorscripts` use.
//! It matters that it is **pure ANSI text**: no Kitty/Sixel graphics protocol,
//! so it works over SSH and on Foot and Alacritty, and it degrades to nothing
//! worse than mojibake on a terminal without truecolor (which is why callers
//! gate on `COLORTERM`).
//!
//! NOT for the prompt. Even a small sprite is a dozen rows; the prompt has a
//! sub-5 ms budget and re-renders per keystroke. This is for surfaces drawn
//! once — `intro`, and Gallery thumbnails.

use anyhow::{Context, Result};
use std::path::Path;

/// Straight RGBA8. Alpha is only ever compared against `ALPHA_CUTOFF`.
pub type Rgba = [u8; 4];

/// Below this alpha a pixel is treated as fully transparent and rendered as a
/// gap rather than blended against a guessed background — the terminal's own
/// background shows through, so a sprite sits correctly on any theme.
const ALPHA_CUTOFF: u8 = 128;

const UPPER_HALF: char = '\u{2580}';

/// A decoded image, ready to encode.
pub struct Sprite {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Rgba>,
}

impl Sprite {
    pub fn get(&self, x: usize, y: usize) -> Rgba {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        self.pixels[y * self.width + x]
    }

    /// Nearest-neighbour downscale to fit `max_cols` columns.
    ///
    /// Nearest-neighbour rather than an averaging filter on purpose: pixel art
    /// blurs badly under interpolation, and blurring is precisely what makes a
    /// half-block sprite look muddy. Never upscales — a 16 px sprite stays 16
    /// px rather than being smeared across the terminal.
    pub fn fit_to_cols(&self, max_cols: usize) -> Sprite {
        if max_cols == 0 || self.width == 0 || self.height == 0 {
            return Sprite { width: 0, height: 0, pixels: Vec::new() };
        }
        if self.width <= max_cols {
            return Sprite {
                width: self.width,
                height: self.height,
                pixels: self.pixels.clone(),
            };
        }
        let new_w = max_cols;
        // Preserve aspect. Round to an even height so the two-pixel-per-cell
        // pairing never leaves a dangling half row.
        let scaled_h = (self.height * new_w).div_ceil(self.width);
        let new_h = if scaled_h % 2 == 0 { scaled_h } else { scaled_h + 1 };
        let mut pixels = Vec::with_capacity(new_w * new_h);
        for y in 0..new_h {
            for x in 0..new_w {
                let sx = x * self.width / new_w;
                let sy = (y * self.height / new_h).min(self.height.saturating_sub(1));
                pixels.push(self.get(sx, sy));
            }
        }
        Sprite { width: new_w, height: new_h, pixels }
    }
}

/// Encode to ANSI half-block rows.
///
/// `indent` is prepended to every row so a caller can inset the sprite without
/// post-processing the string. Returns one `String` per text row (no trailing
/// newline), so callers can interleave the sprite with other content — which
/// is exactly what `intro` does to put text beside the mascot.
pub fn encode_rows(sprite: &Sprite, indent: &str) -> Vec<String> {
    let mut rows = Vec::new();
    if sprite.width == 0 || sprite.height == 0 {
        return rows;
    }
    let mut y = 0;
    while y < sprite.height {
        let mut row = String::from(indent);
        let mut active = false; // whether an SGR is currently set
        for x in 0..sprite.width {
            let top = sprite.get(x, y);
            let bot = sprite.get(x, y + 1);
            let top_on = top[3] >= ALPHA_CUTOFF;
            let bot_on = bot[3] >= ALPHA_CUTOFF;

            match (top_on, bot_on) {
                (false, false) => {
                    // Fully transparent cell: reset so the terminal's own
                    // background shows, and emit a plain space.
                    if active {
                        row.push_str("\x1b[0m");
                        active = false;
                    }
                    row.push(' ');
                }
                (true, false) => {
                    row.push_str(&format!("\x1b[38;2;{};{};{}m", top[0], top[1], top[2]));
                    if active {
                        row.push_str("\x1b[49m"); // default background
                    }
                    row.push(UPPER_HALF);
                    active = true;
                }
                (false, true) => {
                    // Only the lower pixel is opaque: flip to LOWER HALF so the
                    // transparent half stays the terminal background instead of
                    // being painted.
                    row.push_str(&format!("\x1b[49;38;2;{};{};{}m", bot[0], bot[1], bot[2]));
                    row.push('\u{2584}');
                    active = true;
                }
                (true, true) => {
                    row.push_str(&format!(
                        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m",
                        top[0], top[1], top[2], bot[0], bot[1], bot[2]
                    ));
                    row.push(UPPER_HALF);
                    active = true;
                }
            }
        }
        if active {
            row.push_str("\x1b[0m");
        }
        rows.push(row);
        y += 2;
    }
    rows
}

/// Decode a PNG into a `Sprite`.
///
/// PNG only, via the `png` crate rather than `image`: this is the one format
/// worth supporting and `image` would pull a dozen decoders the project never
/// uses.
pub fn load_png(path: &Path) -> Result<Sprite> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("cannot open sprite {}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    // Expand palette (indexed) PNGs to RGB and tRNS chunks to a real alpha
    // channel. Pixel art is very often indexed — it is what ImageMagick and
    // most sprite tools emit — so without this the common case is rejected.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("{} is not a readable PNG", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .with_context(|| format!("cannot decode {}", path.display()))?;

    let (w, h) = (info.width as usize, info.height as usize);
    let bytes = &buf[..info.buffer_size()];
    let pixels = to_rgba(bytes, w * h, info.color_type, info.bit_depth)
        .with_context(|| format!("unsupported PNG format in {}", path.display()))?;
    Ok(Sprite { width: w, height: h, pixels })
}

/// Normalise the common 8-bit PNG colour types to RGBA.
fn to_rgba(
    bytes: &[u8],
    count: usize,
    color: png::ColorType,
    depth: png::BitDepth,
) -> Result<Vec<Rgba>> {
    if depth != png::BitDepth::Eight {
        anyhow::bail!("only 8-bit PNGs are supported (got {depth:?})");
    }
    let stride = match color {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        other => anyhow::bail!("unsupported PNG colour type {other:?}"),
    };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let p = &bytes[i * stride..];
        out.push(match color {
            png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
            png::ColorType::Rgb => [p[0], p[1], p[2], 255],
            png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
            _ => [p[0], p[0], p[0], 255],
        });
    }
    Ok(out)
}

/// Load, fit and encode in one step. `None` when the file is missing or
/// undecodable — a broken mascot must never fail the caller.
pub fn render(path: &Path, max_cols: usize, indent: &str) -> Option<Vec<String>> {
    let sprite = load_png(path).ok()?;
    Some(encode_rows(&sprite.fit_to_cols(max_cols), indent))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, px: Rgba) -> Sprite {
        Sprite { width: w, height: h, pixels: vec![px; w * h] }
    }

    #[test]
    fn two_pixel_rows_collapse_into_one_text_row() {
        // The whole point of half-blocks: a cell carries two pixel rows, so a
        // 4-row image is 2 text rows and the sprite keeps its proportions.
        let rows = encode_rows(&solid(3, 4, [255, 0, 0, 255]), "");
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn odd_height_still_emits_the_final_row() {
        // The last row has no lower neighbour; it must not be dropped.
        let rows = encode_rows(&solid(2, 3, [1, 2, 3, 255]), "");
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn opaque_cell_sets_both_colours() {
        let rows = encode_rows(&solid(1, 2, [10, 20, 30, 255]), "");
        assert!(rows[0].contains("\x1b[38;2;10;20;30m"), "fg missing: {:?}", rows[0]);
        assert!(rows[0].contains("\x1b[48;2;10;20;30m"), "bg missing: {:?}", rows[0]);
        assert!(rows[0].contains('\u{2580}'));
        assert!(rows[0].ends_with("\x1b[0m"), "row must reset");
    }

    #[test]
    fn fully_transparent_cell_is_a_bare_space() {
        // Painting transparency would box the sprite in a colour that fights
        // whatever theme the terminal is using.
        let rows = encode_rows(&solid(2, 2, [255, 255, 255, 0]), "");
        assert_eq!(rows[0], "  ", "expected two plain spaces, got {:?}", rows[0]);
    }

    #[test]
    fn half_transparent_cell_flips_to_lower_block() {
        // Top transparent, bottom opaque -> LOWER HALF with a default
        // background, so the transparent half stays the terminal's own.
        let mut s = solid(1, 2, [9, 9, 9, 255]);
        s.pixels[0] = [0, 0, 0, 0];
        let rows = encode_rows(&s, "");
        assert!(rows[0].contains('\u{2584}'), "expected lower half: {:?}", rows[0]);
        assert!(rows[0].contains("\x1b[49;38;2;9;9;9m"));
    }

    #[test]
    fn indent_is_applied_to_every_row() {
        let rows = encode_rows(&solid(1, 4, [1, 1, 1, 255]), ">>");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.starts_with(">>")));
    }

    #[test]
    fn downscale_preserves_aspect_and_pairs_rows() {
        let s = solid(100, 50, [7, 7, 7, 255]).fit_to_cols(20);
        assert_eq!(s.width, 20);
        // 50 * 20 / 100 = 10, already even.
        assert_eq!(s.height, 10);
        assert_eq!(s.pixels.len(), 200);
    }

    #[test]
    fn downscale_rounds_height_up_to_even() {
        // An odd scaled height would leave a dangling half row.
        let s = solid(100, 45, [7, 7, 7, 255]).fit_to_cols(10);
        assert_eq!(s.width, 10);
        assert_eq!(s.height % 2, 0, "height must be even, got {}", s.height);
    }

    #[test]
    fn small_images_are_never_upscaled() {
        // Smearing a 16px sprite across the terminal looks worse, not bigger.
        let s = solid(16, 16, [3, 3, 3, 255]).fit_to_cols(80);
        assert_eq!(s.width, 16);
        assert_eq!(s.height, 16);
    }

    #[test]
    fn empty_and_degenerate_inputs_are_safe() {
        assert!(encode_rows(&solid(0, 0, [0, 0, 0, 0]), "").is_empty());
        let z = solid(4, 4, [1, 1, 1, 255]).fit_to_cols(0);
        assert_eq!(z.width, 0);
        assert!(encode_rows(&z, "").is_empty());
    }

    #[test]
    fn indexed_png_is_decoded_via_expansion() {
        // Pixel art is commonly palette-indexed; the decoder must expand it
        // rather than reject the format outright.
        let dir = std::env::temp_dir().join(format!("o10k-sprite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("indexed.png");
        let made = std::process::Command::new("magick")
            .args(["-size", "8x8", "xc:none", "-fill", "#ff0000",
                   "-draw", "rectangle 2,2 5,5", "PNG8:"])
            .arg(&path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !made {
            return; // ImageMagick absent on this box; nothing to assert.
        }
        let sprite = load_png(&path).expect("indexed PNG must decode");
        assert_eq!(sprite.width, 8);
        assert_eq!(sprite.height, 8);
        // The drawn square is opaque red; the surround stays transparent.
        assert_eq!(sprite.get(3, 3)[3], 255, "drawn pixel should be opaque");
        assert_eq!(sprite.get(0, 0)[3], 0, "corner should stay transparent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_returns_none_rather_than_failing() {
        // A broken mascot must never take down `intro`.
        assert!(render(Path::new("/nonexistent/mascot.png"), 20, "").is_none());
    }

    #[test]
    fn rgb_png_without_alpha_is_fully_opaque() {
        let px = to_rgba(&[10, 20, 30, 40, 50, 60], 2, png::ColorType::Rgb, png::BitDepth::Eight)
            .expect("rgb converts");
        assert_eq!(px[0], [10, 20, 30, 255]);
        assert_eq!(px[1], [40, 50, 60, 255]);
    }

    #[test]
    fn grayscale_alpha_png_expands_to_rgba() {
        let px = to_rgba(&[128, 255, 64, 0], 2, png::ColorType::GrayscaleAlpha, png::BitDepth::Eight)
            .expect("gray+alpha converts");
        assert_eq!(px[0], [128, 128, 128, 255]);
        assert_eq!(px[1], [64, 64, 64, 0]);
    }

    #[test]
    fn sixteen_bit_png_is_rejected_clearly() {
        let err = to_rgba(&[0; 8], 1, png::ColorType::Rgba, png::BitDepth::Sixteen)
            .expect_err("16-bit unsupported");
        assert!(err.to_string().contains("8-bit"), "got: {err}");
    }
}

