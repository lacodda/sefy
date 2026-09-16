//! The mark the executable carries, held against the level rule of the line.
//!
//! sefy has three masters of one mark — a filled tile (S), the same tile
//! plated and outlined (M), and the plated tile with the metaphor's dots
//! beneath the code (L) — and a rule for which survives at which size. The
//! rule was written down and then not followed: the exporter rasterized the S
//! master for every entry of `icon.ico`, so a 256px icon was a flat teal
//! lozenge and the M and L masters were drawn into nothing at all.
//!
//! Nothing catches that by looking at the file. The container is well-formed,
//! every entry is a valid PNG of the right dimensions, and the only way to see
//! the defect is to look at the pixels — which is why this gate decodes them.
//!
//! The sibling failure is order: Windows picks an image by closest size and
//! ignores the directory order, but readers that take the first entry verbatim
//! exist, and a 16px first entry gives a title bar stretched from sixteen
//! pixels. Cheap to hold, expensive to notice.

use std::path::{Path, PathBuf};

/// The file build.rs feeds to the Windows linker.
///
/// Inside the crate rather than in the repository's `assets/`: `cargo install
/// sefy` is a documented install path and cargo packages only what sits under
/// the crate directory.
const ICO: &[u8] = include_bytes!("../assets/icon.ico");

/// Root of the repository, two levels above this crate.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives at crates/sefy-cli")
        .to_path_buf()
}

/// One image inside an `.ico`, as the directory describes it.
struct Entry {
    size: u32,
    offset: usize,
    length: usize,
}

/// Read the directory of an `.ico`.
///
/// A malformed container gives back what it could read rather than an error:
/// the assertions below are what report a bad file, and they say more about it
/// than a panic in a parser would.
fn entries(ico: &[u8]) -> Vec<Entry> {
    // Header: reserved (2), type (2), count (2). Then 16 bytes per entry.
    let Some(count) = ico
        .get(4..6)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as usize)
    else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let at = 6 + index * 16;
        let Some(entry) = ico.get(at..at + 16) else {
            break;
        };
        // A zero width means 256: the field is one byte and 256 does not fit.
        let size = if entry[0] == 0 {
            256
        } else {
            u32::from(entry[0])
        };
        let length = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        if ico.len() < offset + length {
            continue;
        }
        out.push(Entry {
            size,
            offset,
            length,
        });
    }
    out
}

/// Which master a raster was taken from, read off two pixels.
#[derive(Debug, PartialEq, Eq)]
enum Level {
    /// The filled teal tile: the whole hexagon is the brand colour.
    Small,
    /// The plated tile with an outline, and nothing under the code.
    Medium,
    /// The plated tile with the metaphor's dots under the code.
    Large,
}

/// Decide the level of one image.
///
/// Two samples, both chosen to sit on something only one master puts there:
///
/// - a quarter of the way across, vertically centred: inside the hexagon and
///   clear of the code. Teal on the S master, near-black plate on M and L;
/// - the middle dot of the metaphor, at 50% across and 67% down. Only the L
///   master draws it; on M that spot is bare plate.
///
/// Read together the pair names the master, which is what the rule is about —
/// a gate that only told "filled" from "plated" would accept the M tile at
/// 256px, where there is room for the mark the product is known by.
fn level_of(image: &image::RgbaImage) -> Level {
    let (width, height) = image.dimensions();
    let brightness = |x: u32, y: u32| {
        let pixel = image.get_pixel(x, y).0;
        assert!(
            pixel[3] > 40,
            "the image is transparent at {x},{y}, where the tile should be"
        );
        u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])
    };

    // Measured: 530 on the filled tile, 98 on the plate. The threshold sits
    // between them with room on both sides, so antialiasing cannot flip it.
    if brightness(width / 4, height / 2) > 300 {
        return Level::Small;
    }
    // Measured: 253 on a dot, 98 on bare plate.
    if brightness(width / 2, height * 2 / 3) > 170 {
        Level::Large
    } else {
        Level::Medium
    }
}

/// Decode one entry.
fn decode(entry: &Entry) -> image::RgbaImage {
    let payload = &ICO[entry.offset..entry.offset + entry.length];
    image::load_from_memory_with_format(payload, image::ImageFormat::Png)
        .unwrap_or_else(|e| panic!("the {}px entry is not a PNG: {e}", entry.size))
        .to_rgba8()
}

/// The container is readable at all. Everything below rests on this.
#[test]
fn the_embedded_icon_has_images() {
    let entries = entries(ICO);
    assert!(!entries.is_empty(), "icon.ico gave no images");
    assert_eq!(
        u16::from_le_bytes([ICO[2], ICO[3]]),
        1,
        "icon.ico is not an icon resource"
    );
    for entry in &entries {
        assert!(entry.length > 0, "the {}px image is empty", entry.size);
    }
}

/// The level rule of the line, held against the actual pixels.
///
/// This is the test the patch exists for. S at 27px and below — the outline
/// and the dots collapse into noise there, and the filled tile is all that
/// reads; M from 28 to 63; L at 64 and up, where there is room for the whole
/// mark. Anything else means a master was drawn and then rasterized nowhere.
#[test]
fn every_size_carries_the_level_that_reads_at_it() {
    for entry in entries(ICO) {
        let image = decode(&entry);
        assert_eq!(
            image.width(),
            entry.size,
            "the {}px entry holds a {}px image",
            entry.size,
            image.width()
        );

        let wanted = match entry.size {
            ..=27 => Level::Small,
            28..=63 => Level::Medium,
            _ => Level::Large,
        };
        assert_eq!(
            level_of(&image),
            wanted,
            "the {}px image is not the {wanted:?} master; the level rule of the line is \
             S up to 27px, M to 63px, L above",
            entry.size
        );
    }
}

/// Largest first.
///
/// Windows picks by closest size and ignores order, but readers that take the
/// first entry verbatim exist — a sibling project's title bar was stretched
/// from a 16px entry for exactly this reason.
#[test]
fn the_largest_image_comes_first() {
    let sizes: Vec<u32> = entries(ICO).iter().map(|entry| entry.size).collect();
    let mut sorted = sizes.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(sizes, sorted, "the images are not ordered largest first");
    assert!(
        sizes.contains(&256) && sizes.contains(&16),
        "the icon does not span the sizes Windows asks for: {sizes:?}"
    );
}

/// Every level is actually in the file.
///
/// The rule above is satisfied vacuously by a container that only holds small
/// sizes. This says the three masters all reached the icon, which is the thing
/// that was false before the patch.
#[test]
fn all_three_masters_reach_the_icon() {
    let levels: Vec<Level> = entries(ICO).iter().map(|e| level_of(&decode(e))).collect();
    assert!(
        levels.contains(&Level::Small)
            && levels.contains(&Level::Medium)
            && levels.contains(&Level::Large),
        "icon.ico does not carry all three levels of the mark: {levels:?}"
    );
}

/// One `.ico`, in the one place the build reads it from.
///
/// A copy under `assets/` beside the SVG masters is the obvious thing to add
/// back — it is where the rest of the artwork lives — and it is exactly what
/// would let the icon Explorer shows drift from the one the exporter draws.
#[test]
fn the_icon_has_no_second_copy() {
    let stray = repo_root().join("assets/icon.ico");
    assert!(
        !stray.exists(),
        "assets/icon.ico exists again; the build reads crates/sefy-cli/assets/icon.ico, \
         so the two would drift and the stale one is the one nobody looks at"
    );
}

/// The icon reaches a `cargo install`.
///
/// build.rs embeds it unconditionally, so a package that does not carry it
/// fails the build on crates.io rather than quietly shipping without a mark —
/// but only if the file is under the crate directory, which is what this says.
#[test]
fn the_packaged_crate_carries_the_icon() {
    let inside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/icon.ico");
    assert!(
        inside.is_file(),
        "the icon is not under the crate directory; cargo would not package it and \
         `cargo install sefy` would fail to build on crates.io"
    );
}
