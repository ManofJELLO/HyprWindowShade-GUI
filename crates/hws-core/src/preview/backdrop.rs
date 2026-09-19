//! The gradient the preview window sits on.
//!
//! A shader is judged against what is behind it. A flat colour hides half of
//! what matters — a dissolve that does not reach zero alpha looks fine on grey
//! and obviously wrong over a light-to-dark ramp, and a tint that lifts blacks
//! is invisible until there are blacks to lift. So the backdrop is a vertical
//! gradient, and the window sits over both ends of it.
//!
//! The PNG is written here rather than with an image crate, which keeps the
//! preview free of an encoding dependency for what is, after all, a few hundred
//! bytes of ramp. It has to be a PNG and not something simpler: gdk-pixbuf
//! loads most formats from modules that are frequently not installed, so BMP is
//! a coin toss, while every wallpaper tool worth the name links libpng directly.
//!
//! If no wallpaper tool is installed the preview still runs —
//! `misc:background_color` gives a flat backdrop and only the gradient is lost.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Wallpaper tools, and how each one is told to show a file.
///
/// `swaybg` first because it is the one most Hyprland setups already have.
/// `stretch`, not `fill`: the gradient is a tall thin ramp, and a mode that
/// preserves its aspect ratio crops it to a sliver that reads as a flat colour.
/// Stretching a vertical ramp to any size leaves it a correct vertical ramp.
const TOOLS: &[(&str, &[&str])] =
    &[("swaybg", &["-i", "{}", "-m", "stretch"]), ("wbg", &["-s", "{}"])];

/// The program and arguments that paint `image`, or `None` when no tool is
/// installed.
pub fn command_for(image: &Path) -> Option<(PathBuf, Vec<String>)> {
    let path = image.to_string_lossy().into_owned();
    TOOLS.iter().find_map(|(name, args)| {
        crate::paths::which(name)
            .map(|program| (program, args.iter().map(|a| a.replace("{}", &path)).collect()))
    })
}

/// Write a vertical gradient as a truecolour PNG.
///
/// `top` and `bottom` are RGB. The image is deliberately small — it is stretched
/// over the output, and a preview pane is a few hundred pixels — which keeps the
/// write well under a millisecond even though nothing here compresses.
pub fn write_gradient(
    path: &Path,
    width: u32,
    height: u32,
    top: [u8; 3],
    bottom: [u8; 3],
) -> Result<()> {
    let width = width.max(1);
    let height = height.max(1);

    // PNG rows run top to bottom and each begins with a filter byte; filter 0
    // is "none", which is what an uncompressed image wants.
    let mut raw = Vec::with_capacity((height * (1 + width * 3)) as usize);
    let last = (height - 1) as f32;
    for row in 0..height {
        raw.push(0);
        let t = if last == 0.0 { 0.0 } else { row as f32 / last };
        let pixel =
            [mix(top[0], bottom[0], t), mix(top[1], bottom[1], t), mix(top[2], bottom[2], t)];
        for _ in 0..width {
            raw.extend_from_slice(&pixel);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolour, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut file = std::fs::File::create(path).map_err(|e| Error::io(path, e))?;
    file.write_all(&out).map_err(|e| Error::io(path, e))?;
    Ok(())
}

fn mix(from: u8, to: u8, t: f32) -> u8 {
    (from as f32 + (to as f32 - from as f32) * t.clamp(0.0, 1.0)).round() as u8
}

/// Append one PNG chunk: length, type, payload, CRC of type and payload.
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);

    let mut crc = Crc::new();
    crc.update(kind);
    crc.update(payload);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// Wrap bytes in a zlib stream of uncompressed deflate blocks.
///
/// Deflate's stored block is a length and the bytes themselves, so this is a
/// legal zlib stream that every decoder accepts without any of the machinery
/// that makes compressing one worth a dependency.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    const MAX: usize = 65535;
    let mut out = Vec::with_capacity(data.len() + data.len() / MAX * 5 + 6);

    // Deflate, 32K window, no preset dictionary, and a check byte that makes
    // the two-byte header a multiple of 31.
    out.extend_from_slice(&[0x78, 0x01]);

    let mut chunks = data.chunks(MAX).peekable();
    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xff, 0xff]);
    }
    while let Some(block) = chunks.next() {
        let final_block = chunks.peek().is_none();
        out.push(u8::from(final_block));
        out.extend_from_slice(&(block.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        out.extend_from_slice(block);
    }

    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// The CRC-32 PNG uses, built on first use rather than held as a table.
struct Crc {
    value: u32,
}

impl Crc {
    fn new() -> Self {
        Self { value: 0xffff_ffff }
    }

    fn update(&mut self, data: &[u8]) {
        for byte in data {
            self.value ^= *byte as u32;
            for _ in 0..8 {
                let carry = self.value & 1;
                self.value >>= 1;
                if carry != 0 {
                    self.value ^= 0xedb8_8320;
                }
            }
        }
    }

    fn finish(self) -> u32 {
        self.value ^ 0xffff_ffff
    }
}

/// Prepare the backdrop for a preview, returning how to show it.
///
/// The caller launches this itself, against the nested socket, and only once
/// the instance is headless: a wallpaper tool started earlier binds a layer
/// surface to the output that the preview is about to remove, and does not
/// always follow when a new one appears. Starting it afterwards means there is
/// exactly one output and no question about which one it dressed.
///
/// Best-effort throughout: a failure here costs the gradient, not the preview,
/// so the caller treats `None` as "flat background" rather than as an error.
pub fn prepare(dir: &Path, dark: bool) -> Option<(PathBuf, Vec<String>)> {
    // Light to dark either way round, so both ends of the ramp are present
    // whichever theme the user runs. The dark variant simply starts lower.
    let (top, bottom) = if dark {
        ([0xd8, 0xd8, 0xdc], [0x12, 0x12, 0x16])
    } else {
        ([0xf4, 0xf4, 0xf6], [0x30, 0x30, 0x38])
    };

    let image: PathBuf = dir.join("backdrop.png");
    write_gradient(&image, 8, 256, top, bottom).ok()?;
    command_for(&image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_signature_and_chunks_are_a_png() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("g.png");
        write_gradient(&p, 4, 3, [255, 255, 255], [0, 0, 0]).unwrap();

        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 4);
        assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 3);
        assert_eq!(bytes[24], 8, "eight bits per channel");
        assert_eq!(bytes[25], 2, "truecolour");
        assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND");
    }

    #[test]
    fn the_crc_matches_the_known_value_for_iend() {
        // IEND is empty, so its CRC is a fixed number every PNG in the world
        // ends with — a cheap check that the polynomial is right.
        let mut out = Vec::new();
        chunk(&mut out, b"IEND", &[]);
        assert_eq!(out, vec![0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]);
    }

    #[test]
    fn the_zlib_stream_carries_the_bytes_and_a_correct_adler() {
        let data = b"the quick brown fox";
        let stream = zlib_stored(data);

        assert_eq!(&stream[..2], &[0x78, 0x01]);
        assert_eq!(stream[2], 1, "a single, final, stored block");
        assert_eq!(u16::from_le_bytes(stream[3..5].try_into().unwrap()), data.len() as u16);
        assert_eq!(u16::from_le_bytes(stream[5..7].try_into().unwrap()), !(data.len() as u16));
        assert_eq!(&stream[7..7 + data.len()], data);
        assert_eq!(
            u32::from_be_bytes(stream[stream.len() - 4..].try_into().unwrap()),
            adler32(data)
        );
    }

    #[test]
    fn data_longer_than_a_block_is_split_and_only_the_last_is_final() {
        let data = vec![7u8; 70_000];
        let stream = zlib_stored(&data);
        assert_eq!(stream[2], 0, "the first block is not the last");
        // header(2) + block(5 + 65535) then the second block's header byte.
        assert_eq!(stream[2 + 5 + 65535], 1, "the second block is final");
    }

    #[test]
    fn the_gradient_runs_light_at_the_top_to_dark_at_the_bottom() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("g.png");
        write_gradient(&p, 1, 2, [255, 255, 255], [0, 0, 0]).unwrap();

        // Straight out of the stored block: filter byte, pixel, filter, pixel.
        let bytes = std::fs::read(&p).unwrap();
        let idat = bytes.windows(4).position(|w| w == b"IDAT").unwrap() + 4;
        let raw = &bytes[idat + 2 + 5..];
        assert_eq!(&raw[..4], &[0, 255, 255, 255], "first row is the top, and light");
        assert_eq!(&raw[4..8], &[0, 0, 0, 0], "second row is the bottom, and dark");
    }

    #[test]
    fn a_one_pixel_tall_gradient_does_not_divide_by_zero() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("g.png");
        assert!(write_gradient(&p, 4, 1, [10, 20, 30], [200, 200, 200]).is_ok());
    }

    #[test]
    fn the_image_path_is_substituted_into_the_tool_arguments() {
        // Checked against the table directly: whether any of these tools is
        // installed is a property of the machine, not of the code.
        for (name, args) in TOOLS {
            let rendered: Vec<String> =
                args.iter().map(|a| a.replace("{}", "/tmp/bg.png")).collect();
            assert!(
                rendered.iter().any(|a| a == "/tmp/bg.png"),
                "{name} never receives the image path"
            );
        }
    }
}
